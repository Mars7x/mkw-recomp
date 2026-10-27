use mkw_arm64::{
    simd::{CryptoKind, Simd},
    Op,
};
use mkw_runtime::abi::Cpu;
use std::{process::Command, sync::OnceLock};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(include31: bool) -> Vec<u32> {
    let mut out = Vec::new();
    for base in [0x4e284800, 0x4e285800, 0x4e286800, 0x4e287800] {
        for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
            if !include31 && dst == 31 {
                continue;
            }
            out.push(base | (src << 5) | dst);
        }
    }
    out
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("aes_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> (usize, usize, CryptoKind) {
    let Op::Simd(Simd::Crypto { dst, src, kind }) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    (usize::from(dst), usize::from(src), kind)
}
fn raw(v: [u64; 2]) -> [u8; 16] {
    let mut b = [0; 16];
    b[..8].copy_from_slice(&v[0].to_le_bytes());
    b[8..].copy_from_slice(&v[1].to_le_bytes());
    b
}
fn pack(v: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(v[..8].try_into().unwrap()),
        u64::from_le_bytes(v[8..].try_into().unwrap()),
    ]
}
// Polynomial long multiplication/reduction deliberately differs from xtime/powers.
fn polynomial(a: u8, b: u8) -> u8 {
    let mut product = 0u16;
    for bit in 0..8 {
        if b & (1 << bit) != 0 {
            product ^= u16::from(a) << bit;
        }
    }
    for bit in (8..16).rev() {
        if product & (1 << bit) != 0 {
            product ^= 0x11b << (bit - 8);
        }
    }
    product as u8
}
fn tables() -> &'static ([u8; 256], [u8; 256]) {
    static TABLES: OnceLock<([u8; 256], [u8; 256])> = OnceLock::new();
    TABLES.get_or_init(|| {
        let forward = std::array::from_fn(|n| {
            let inverse = if n == 0 {
                0
            } else {
                (1..=255u16)
                    .find(|&v| polynomial(n as u8, v as u8) == 1)
                    .unwrap() as u8
            };
            let mut out = 0;
            for bit in 0..8 {
                let mut parity = (0x63 >> bit) & 1;
                for at in [
                    bit,
                    (bit + 4) % 8,
                    (bit + 5) % 8,
                    (bit + 6) % 8,
                    (bit + 7) % 8,
                ] {
                    parity ^= (inverse >> at) & 1;
                }
                out |= parity << bit;
            }
            out
        });
        let mut reverse = [0; 256];
        for (n, &v) in forward.iter().enumerate() {
            reverse[usize::from(v)] = n as u8;
        }
        (forward, reverse)
    })
}
fn mix(source: [u8; 16], inverse: bool) -> [u8; 16] {
    let coefficients = if inverse {
        [14, 11, 13, 9]
    } else {
        [2, 3, 1, 1]
    };
    std::array::from_fn(|at| {
        let row = at % 4;
        let column = at / 4;
        let mut out = 0;
        for (k, coefficient) in coefficients.into_iter().enumerate() {
            out ^= polynomial(source[column * 4 + (row + k) % 4], coefficient);
        }
        out
    })
}
fn oracle(kind: CryptoKind, state: [u8; 16], source: [u8; 16]) -> [u8; 16] {
    match kind {
        CryptoKind::MixColumns => mix(source, false),
        CryptoKind::InverseMixColumns => mix(source, true),
        CryptoKind::EncryptRound | CryptoKind::DecryptRound => {
            let inverse = kind == CryptoKind::DecryptRound;
            let table = if inverse { &tables().1 } else { &tables().0 };
            std::array::from_fn(|at| {
                let row = at % 4;
                let column = at / 4;
                let from = row
                    + 4 * if inverse {
                        (column + 4 - row) % 4
                    } else {
                        (column + row) % 4
                    };
                table[usize::from(state[from] ^ source[from])]
            })
        }
    }
}
fn patterns() -> Vec<([u8; 16], [u8; 16])> {
    let mut out = Vec::new();
    for n in 0..256u16 {
        let n = n as u8;
        out.push(([n; 16], [0; 16]));
        out.push((std::array::from_fn(|i| (i as u8).wrapping_mul(17)), [n; 16]));
        out.push((
            std::array::from_fn(|i| n.rotate_left((i % 8) as u32)),
            std::array::from_fn(|i| (i as u8).wrapping_mul(23).wrapping_add(n.wrapping_mul(3))),
        ));
    }
    let mut x = 0x8c4af101cab87365u64;
    for _ in 0..64 {
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        };
        out.push((
            std::array::from_fn(|_| next()),
            std::array::from_fn(|_| next()),
        ));
    }
    out
}
fn frame(w: u32, left: [u8; 16], right: [u8; 16]) -> [[u64; 2]; 32] {
    let (dst, src, _) = decoded(w);
    let mut frame = std::array::from_fn(|r| [r as u64 + 123, r as u64 + 456]);
    frame[dst] = pack(left);
    frame[src] = pack(right);
    frame
}
#[test]
fn aes_matches_independent_polynomial_and_byte_oracles() {
    let path = folder("aes-known");
    let words = words(true);
    let code = codes(&words);
    let patterns = patterns();
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for &w in &words {
            let (dst, src, kind) = decoded(w);
            let native = unsafe {
                library
                    .get::<Entry>(format!("aes_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (i, &(left, right)) in patterns.iter().enumerate() {
                let nzcv = ((i as u32) & 15) << 28;
                let before = frame(w, left, right);
                let flags = 0x9f | (((i as u32) & 1) << 27);
                let mut cpu = Cpu {
                    sp: 99,
                    n: (nzcv >> 31) & 1,
                    z: (nzcv >> 30) & 1,
                    c: (nzcv >> 29) & 1,
                    v: (nzcv >> 28) & 1,
                    fpcr: 0x02c00000,
                    fpsr: flags,
                    vregs: before,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                let x = cpu.x;
                let mut expected = before;
                expected[dst] = pack(oracle(kind, raw(before[dst]), raw(before[src])));
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                assert_eq!(cpu.vregs, expected, "{opt} {w:x} pattern{i}");
                assert_eq!(cpu.x, x);
                assert_eq!((cpu.sp, cpu.fpcr, cpu.fpsr), (99, 0x02c00000, flags));
                assert_eq!(
                    (cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28),
                    nzcv
                );
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"cases_per_optimization":words.len()*patterns.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn aes_matches_synthetic_qemu() {
    let path = folder("aes-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            serde_json::to_vec(&serde_json::json!({"qemu_reference":"unavailable"})).unwrap(),
        )
        .unwrap();
        return;
    }
    let words = words(false);
    let mut cases = Vec::new();
    for &w in &words {
        for (i, (left, right)) in patterns().into_iter().enumerate() {
            let v = frame(w, left, right);
            cases.push(reference::VectorCase {
                word: w,
                vectors: v[..4].try_into().unwrap(),
                fpcr: 0x02c00000,
                fpsr: 0x9f | (((i as u32) & 1) << 27),
                nzcv: ((i as u32) & 15) << 28,
            });
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (i, chunk) in cases.chunks(2000).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{i}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        let out = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .current_dir(&path)
            .arg(&image)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        oracle.extend(out.stdout);
    }
    assert_eq!(oracle.len(), cases.len() * 32);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("aes_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: (c.nzcv >> 31) & 1,
                z: (c.nzcv >> 30) & 1,
                c: (c.nzcv >> 29) & 1,
                v: (c.nzcv >> 28) & 1,
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
            let actual: Vec<_> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                u64::from(cpu.fpsr),
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(
                actual,
                &oracle[i * 32..(i + 1) * 32],
                "{opt} {:x} {i}",
                c.word
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
fn round_keys() -> [[u8; 16]; 11] {
    let mut keys = [[0; 16]; 11];
    keys[0] = std::array::from_fn(|i| i as u8);
    for round in 1..=10 {
        let old = keys[round - 1];
        let mut t = [old[13], old[14], old[15], old[12]].map(|v| tables().0[usize::from(v)]);
        t[0] ^= [1, 2, 4, 8, 16, 32, 64, 128, 27, 54][round - 1];
        for i in 0..16 {
            keys[round][i] = old[i] ^ if i < 4 { t[i] } else { keys[round][i - 4] };
        }
    }
    keys
}
#[test]
fn aes128_encrypt_and_decrypt_match_standard_known_answer() {
    // Public standard vector: original FIPS 197 (2001), Appendix C.1.
    let path = folder("aes-standard-vector");
    let plaintext = std::array::from_fn(|i| (i as u8) * 17);
    let ciphertext = [
        0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4, 0xc5,
        0x5a,
    ];
    let keys = round_keys();
    let program = |inverse: bool| {
        let mut words = Vec::new();
        for r in 0..9 {
            words.push((if inverse { 0x4e285800 } else { 0x4e284800 }) | ((r + 1) << 5));
            words.push(if inverse { 0x4e287800 } else { 0x4e286800 });
        }
        words.push((if inverse { 0x4e285800 } else { 0x4e284800 }) | (10 << 5));
        words.extend([0x6e2b1c00, 0xd65f03c0]);
        words
    };
    let mut code = generate(&program(false), "aes128_encrypt");
    code += &generate(&program(true), "aes128_decrypt");
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for inverse in [false, true] {
            let name = if inverse {
                b"aes128_decrypt\0".as_slice()
            } else {
                b"aes128_encrypt\0".as_slice()
            };
            let native = unsafe { library.get::<Entry>(name).unwrap() };
            let mut cpu = Cpu {
                fpcr: 0x02c00000,
                fpsr: 0x0800009f,
                ..Cpu::default()
            };
            cpu.vregs[0] = pack(if inverse { ciphertext } else { plaintext });
            if inverse {
                cpu.vregs[1] = pack(keys[10]);
                for r in 1..10 {
                    cpu.vregs[r + 1] = pack(mix(keys[10 - r], true));
                }
                cpu.vregs[11] = pack(keys[0]);
            } else {
                for (r, key) in keys.iter().enumerate() {
                    cpu.vregs[r + 1] = pack(*key);
                }
            }
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 30) }, 0);
            assert_eq!(
                raw(cpu.vregs[0]),
                if inverse { plaintext } else { ciphertext },
                "{opt} inverse{inverse}"
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!((cpu.fpcr, cpu.fpsr), (0x02c00000, 0x0800009f));
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":2,"standard":"FIPS197 Appendix C.1 AES-128","fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
