use mkw_arm64::{
    vector_integer::{HashKind, SecureHash},
    Op,
};
use mkw_runtime::abi::Cpu;
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
const TERNARY: [u32; 7] = [
    0x5e000000, 0x5e001000, 0x5e002000, 0x5e003000, 0x5e004000, 0x5e005000, 0x5e006000,
];
fn word(base: u32, d: u32, n: u32, m: u32) -> u32 {
    base | (m << 16) | (n << 5) | d
}
fn words(full: bool) -> Vec<u32> {
    let mut words = Vec::new();
    for base in TERNARY {
        for (d, n, m) in [
            (0, 1, 2),
            (0, 0, 2),
            (0, 1, 0),
            (0, 0, 0),
            (31, 31, 31),
            (31, 1, 31),
        ] {
            if full || d == 0 {
                words.push(word(base, d, n, m));
            }
        }
    }
    for base in [0x5e280800, 0x5e281800, 0x5e282800] {
        for (d, n) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
            if full || d == 0 {
                words.push(word(base, d, n, 0));
            }
        }
    }
    words
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("secure_hash_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> SecureHash {
    let Op::SecureHash(v) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    v
}
fn lanes(v: [u64; 2]) -> [u32; 4] {
    [
        v[0] as u32,
        (v[0] >> 32) as u32,
        v[1] as u32,
        (v[1] >> 32) as u32,
    ]
}
fn pack(v: [u32; 4]) -> [u64; 2] {
    [
        u64::from(v[0]) | (u64::from(v[1]) << 32),
        u64::from(v[2]) | (u64::from(v[3]) << 32),
    ]
}
fn truth(a: u32, b: u32, c: u32, table: u8) -> u32 {
    (0..32)
        .map(|bit| {
            let index = ((a >> bit) & 1) * 4 + ((b >> bit) & 1) * 2 + ((c >> bit) & 1);
            u32::from((table >> index) & 1) << bit
        })
        .sum()
}
fn sum(v: &[u32]) -> u32 {
    v.iter().map(|&x| u64::from(x)).sum::<u64>() as u32
}
fn sigma(v: u32, big: bool, second: bool) -> u32 {
    match (big, second) {
        (false, false) => v.rotate_right(7) ^ v.rotate_right(18) ^ (v >> 3),
        (false, true) => v.rotate_right(17) ^ v.rotate_right(19) ^ (v >> 10),
        (true, false) => v.rotate_right(2) ^ v.rotate_right(13) ^ v.rotate_right(22),
        (true, true) => v.rotate_right(6) ^ v.rotate_right(11) ^ v.rotate_right(25),
    }
}
fn sha1_rounds(mut state: [u32; 5], wk: [u32; 4], table: u8) -> [u32; 5] {
    for value in wk {
        state = [
            sum(&[
                state[0].rotate_left(5),
                truth(state[1], state[2], state[3], table),
                state[4],
                value,
            ]),
            state[0],
            state[1].rotate_right(2),
            state[2],
            state[3],
        ];
    }
    state
}
fn sha256_rounds(mut s: [u32; 8], wk: [u32; 4]) -> [u32; 8] {
    for value in wk {
        let t = sum(&[
            s[7],
            sigma(s[4], true, true),
            truth(s[4], s[5], s[6], 0xca),
            value,
        ]);
        s = [
            sum(&[t, sigma(s[0], true, false), truth(s[0], s[1], s[2], 0xe8)]),
            s[0],
            s[1],
            s[2],
            sum(&[s[3], t]),
            s[4],
            s[5],
            s[6],
        ];
    }
    s
}
fn expected(op: SecureHash, v: &[[u64; 2]; 32]) -> [u64; 2] {
    use HashKind::*;
    let d = lanes(v[usize::from(op.dst)]);
    let n = lanes(v[usize::from(op.lhs)]);
    let m = lanes(v[usize::from(op.rhs)]);
    let output = match op.kind {
        Sha1Rotate => [n[0].rotate_right(2), 0, 0, 0],
        Sha1Choose | Sha1Parity | Sha1Majority => {
            let table = match op.kind {
                Sha1Choose => 0xca,
                Sha1Parity => 0x96,
                Sha1Majority => 0xe8,
                _ => unreachable!(),
            };
            sha1_rounds([d[0], d[1], d[2], d[3], n[0]], m, table)[..4]
                .try_into()
                .unwrap()
        }
        Sha1Schedule0 => {
            let original = u128::from(v[usize::from(op.dst)][0])
                | (u128::from(v[usize::from(op.dst)][1]) << 64);
            let next = u128::from(v[usize::from(op.lhs)][0]) << 64;
            let right = u128::from(v[usize::from(op.rhs)][0])
                | (u128::from(v[usize::from(op.rhs)][1]) << 64);
            let value = (original >> 64) ^ next ^ original ^ right;
            lanes([value as u64, (value >> 64) as u64])
        }
        Sha1Schedule1 => {
            let mut out = [0; 4];
            for i in 0..4 {
                let following = if i == 3 { out[0] } else { n[i + 1] };
                out[i] = (d[i] ^ following).rotate_left(1);
            }
            out
        }
        Sha256Schedule0 => std::array::from_fn(|i| {
            sum(&[
                d[i],
                sigma(if i == 3 { n[0] } else { d[i + 1] }, false, false),
            ])
        }),
        Sha256Schedule1 => {
            let mut out = [0; 4];
            let diagonal = [n[1], n[2], n[3], m[0]];
            for i in 0..4 {
                out[i] = sum(&[
                    d[i],
                    diagonal[i],
                    sigma(if i < 2 { m[i + 2] } else { out[i - 2] }, false, true),
                ]);
            }
            out
        }
        Sha256Hash | Sha256Hash2 => {
            let state: [u32; 8] = if op.kind == Sha256Hash {
                [d, n]
            } else {
                [n, d]
            }
            .concat()
            .try_into()
            .unwrap();
            let result = sha256_rounds(state, m);
            result[if op.kind == Sha256Hash { 0..4 } else { 4..8 }]
                .try_into()
                .unwrap()
        }
    };
    pack(output)
}
fn input(seed: u32) -> [[u64; 2]; 32] {
    let mut x = u64::from(seed) + 0x123456789abcdef;
    std::array::from_fn(|r| {
        let mut out = [0; 2];
        for (h, v) in out.iter_mut().enumerate() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *v = match seed {
                0 => 0,
                1 => u64::MAX,
                2 => 0xaaaaaaaaaaaaaaaa,
                3 => 0x5555555555555555,
                4..=131 => {
                    let bit = seed - 4;
                    let low = if bit / 64 == h as u32 {
                        1u64 << (bit % 64)
                    } else {
                        0
                    };
                    low.rotate_left(r as u32)
                }
                _ => x,
            };
        }
        out
    })
}
#[test]
fn secure_hash_matches_independent_truth_and_round_oracles() {
    let path = folder("secure-hash-known");
    let words = words(true);
    let source = code(&words);
    let mut counts = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let mut observed = 0;
        for &w in &words {
            let op = decoded(w);
            let f = unsafe {
                lib.get::<Entry>(format!("secure_hash_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for seed in 0..516 {
                let before = input(seed);
                let mut want = before;
                want[usize::from(op.dst)] = expected(op, &before);
                let mut cpu = Cpu {
                    vregs: before,
                    fpcr: u32::MAX,
                    fpsr: 0x08000091,
                    n: 1,
                    c: 1,
                    v: 1,
                    sp: 91,
                    fault_status: 77,
                    fault_address: 0x1234,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|n| 123 + n as u64);
                let x = cpu.x;
                assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
                assert_eq!(cpu.vregs, want, "{opt} {w:x} seed{seed}");
                assert_eq!(cpu.x, x);
                assert_eq!(
                    (
                        cpu.fpcr,
                        cpu.fpsr,
                        cpu.n,
                        cpu.z,
                        cpu.c,
                        cpu.v,
                        cpu.sp,
                        cpu.fault_status,
                        cpu.fault_address
                    ),
                    (u32::MAX, 0x08000091, 1, 0, 1, 1, 91, 77, 0x1234)
                );
                observed += 1;
            }
        }
        if counts == 0 {
            counts = observed;
        } else {
            assert_eq!(counts, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":counts,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn secure_hash_matches_synthetic_qemu() {
    let path = folder("secure-hash-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\"}",
        )
        .unwrap();
        return;
    }
    let words = words(false);
    let mut cases = Vec::new();
    for &w in &words {
        for seed in 0..148 {
            cases.push(reference::VectorCase {
                word: w,
                vectors: input(seed)[..4].try_into().unwrap(),
                fpcr: 0x02c00000,
                fpsr: 0x08000091,
                nzcv: 0xb0000000,
            });
        }
    }
    use std::os::unix::fs::PermissionsExt;
    let mut oracle = Vec::new();
    for (i, chunk) in cases.chunks(1500).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{i}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        let result = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .arg(&image)
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        oracle.extend(result.stdout);
    }
    assert_eq!(oracle.len(), cases.len() * 32);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let f = unsafe {
                lib.get::<Entry>(format!("secure_hash_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                n: 1,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let mut want = cpu.vregs;
            want[0] = expected(decoded(c.word), &want);
            assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
            assert_eq!(cpu.vregs, want);
            let actual: Vec<u8> = [
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
                "{opt} {i} {:x}",
                c.word
            );
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
// Published SHA constants and known answers are numerical standard data, not copied code.
const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];
fn padded(message: &[u8]) -> Vec<u8> {
    let mut out = message.to_vec();
    out.push(128);
    while out.len() % 64 != 56 {
        out.push(0);
    }
    out.extend_from_slice(&(message.len() as u64 * 8).to_be_bytes());
    out
}
fn full_hash_code() -> String {
    let mut out = generate(
        &[
            word(0x5e003000, 0, 1, 2),
            word(0x5e281800, 0, 3, 0),
            0xd65f03c0,
        ],
        "sha1_schedule",
    );
    out += &generate(
        &[
            word(0x5e282800, 0, 1, 0),
            word(0x5e006000, 0, 2, 3),
            0xd65f03c0,
        ],
        "sha256_schedule",
    );
    for (base, name) in [
        (0x5e000000, "sha1_choose"),
        (0x5e001000, "sha1_parity"),
        (0x5e002000, "sha1_majority"),
    ] {
        out += &generate(
            &[
                word(0x5e280800, 3, 0, 0),
                word(base, 0, 1, 2),
                0x1e204061,
                0xd65f03c0,
            ],
            name,
        );
    }
    // Save ABCD before updating it; H2 consumes the old state.
    out += &generate(
        &[
            0x4ea01c03,
            word(0x5e004000, 0, 1, 2),
            word(0x5e005000, 1, 3, 2),
            0xd65f03c0,
        ],
        "sha256_rounds",
    );
    out
}
fn native_schedule(lib: &libloading::Library, block: &[u8], sha256: bool) -> Vec<u32> {
    let name = if sha256 {
        b"sha256_schedule\0".as_slice()
    } else {
        b"sha1_schedule\0".as_slice()
    };
    let f = unsafe { lib.get::<Entry>(name).unwrap() };
    let mut words: Vec<u32> = block
        .chunks_exact(4)
        .map(|c| u32::from_be_bytes(c.try_into().unwrap()))
        .collect();
    let mut independent = words.clone();
    for t in 16..if sha256 { 64 } else { 80 } {
        let v = if sha256 {
            sum(&[
                independent[t - 16],
                sigma(independent[t - 15], false, false),
                independent[t - 7],
                sigma(independent[t - 2], false, true),
            ])
        } else {
            (independent[t - 3] ^ independent[t - 8] ^ independent[t - 14] ^ independent[t - 16])
                .rotate_left(1)
        };
        independent.push(v);
    }
    for t in (16..independent.len()).step_by(4) {
        let mut cpu = Cpu::default();
        for n in 0..4 {
            cpu.vregs[n] = pack(words[t - 16 + n * 4..t - 12 + n * 4].try_into().unwrap());
        }
        assert_eq!(unsafe { f(&mut cpu, 20) }, 0);
        let next = lanes(cpu.vregs[0]);
        assert_eq!(&next, &independent[t..t + 4]);
        words.extend(next);
    }
    assert_eq!(words, independent);
    words
}
fn native_digest(lib: &libloading::Library, message: &[u8], sha256: bool) -> String {
    let mut hash = if sha256 {
        vec![
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ]
    } else {
        vec![0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0]
    };
    for block in padded(message).chunks_exact(64) {
        let words = native_schedule(lib, block, sha256);
        let mut cpu = Cpu {
            fpcr: u32::MAX,
            fpsr: 0x08000091,
            n: 1,
            c: 1,
            v: 1,
            ..Cpu::default()
        };
        cpu.vregs[0] = pack(hash[..4].try_into().unwrap());
        cpu.vregs[1] = if sha256 {
            pack(hash[4..].try_into().unwrap())
        } else {
            [u64::from(hash[4]), 0]
        };
        for t in (0..words.len()).step_by(4) {
            let name = if sha256 {
                "sha256_rounds"
            } else {
                match t {
                    0..=19 => "sha1_choose",
                    20..=39 | 60.. => "sha1_parity",
                    _ => "sha1_majority",
                }
            };
            let k = if sha256 {
                0
            } else {
                match t {
                    0..=19 => 0x5a827999,
                    20..=39 => 0x6ed9eba1,
                    40..=59 => 0x8f1bbcdc,
                    _ => 0xca62c1d6,
                }
            };
            let wk = std::array::from_fn(|lane| {
                words[t + lane].wrapping_add(if sha256 { K256[t + lane] } else { k })
            });
            cpu.vregs[2] = pack(wk);
            let f = unsafe { lib.get::<Entry>(format!("{name}\0").as_bytes()).unwrap() };
            assert_eq!(unsafe { f(&mut cpu, 20) }, 0);
        }
        let mut result = lanes(cpu.vregs[0]).to_vec();
        if sha256 {
            result.extend(lanes(cpu.vregs[1]));
        } else {
            result.push(cpu.vregs[1][0] as u32);
        }
        for (i, &value) in result.iter().enumerate() {
            hash[i] = hash[i].wrapping_add(value);
        }
        assert_eq!(
            (cpu.fpcr, cpu.fpsr, cpu.n, cpu.z, cpu.c, cpu.v),
            (u32::MAX, 0x08000091, 1, 0, 1, 1)
        );
    }
    hash.iter().map(|w| format!("{w:08x}")).collect()
}
#[test]
fn complete_native_sha_hashes_match_standard_known_answers() {
    let path = folder("secure-hash-complete");
    let source = full_hash_code();
    let messages: [(&[u8], &str, &str); 3] = [
        (
            b"",
            "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "a9993e364706816aba3e25717850c26c9cd0d89d",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
    ];
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        for (message, sha1, sha256) in messages {
            assert_eq!(native_digest(&lib, message, false), sha1);
            assert_eq!(native_digest(&lib, message, true), sha256);
            assert_eq!(mkw_formats::sha256(message), sha256);
        }
        for size in [1, 55, 56, 63, 64, 65, 127, 128, 1025] {
            let message: Vec<u8> = (0..size).map(|n| (n * 73) as u8).collect();
            assert_eq!(
                native_digest(&lib, &message, true),
                mkw_formats::sha256(&message)
            );
        }
    }
    std::fs::write(path.join("receipt.json"),"{\"known_hashes_per_optimization\":6,\"additional_independent_sha256_messages_per_optimization\":9,\"schedule_words_independently_checked\":true,\"native_optimizations\":[\"O0\",\"O2\"],\"fatal_ubsan\":true,\"game_code_executed\":false}").unwrap();
}
#[test]
fn secure_hash_uses_shared_budget_and_preserves_failure_state() {
    let path = folder("secure-hash-budget");
    let words = words(true);
    let source = code(&words);
    type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        for &w in &words {
            let f = unsafe {
                lib.get::<BudgetEntry>(format!("secure_hash_{w:x}_budget\0").as_bytes())
                    .unwrap()
            };
            for initial in 0..=2 {
                let before = input(191);
                let mut cpu = Cpu {
                    vregs: before,
                    module_base: 0x8000,
                    fpcr: u32::MAX,
                    fpsr: 0x08000091,
                    n: 1,
                    c: 1,
                    v: 1,
                    fault_status: 77,
                    fault_address: 91,
                    ..Cpu::default()
                };
                let mut budget = initial;
                let mut want = before;
                if initial > 0 {
                    let op = decoded(w);
                    want[usize::from(op.dst)] = expected(op, &before);
                }
                assert_eq!(
                    unsafe { f(&mut cpu, &mut budget) },
                    if initial < 2 { 2 } else { 0 }
                );
                assert_eq!(budget, 0);
                assert_eq!(cpu.vregs, want);
                if initial < 2 {
                    assert_eq!(cpu.pc, 0x8000 + initial * 4);
                }
                assert_eq!(
                    (
                        cpu.fpcr,
                        cpu.fpsr,
                        cpu.n,
                        cpu.z,
                        cpu.c,
                        cpu.v,
                        cpu.fault_status,
                        cpu.fault_address
                    ),
                    (u32::MAX, 0x08000091, 1, 0, 1, 1, 77, 91)
                );
            }
        }
    }
}
