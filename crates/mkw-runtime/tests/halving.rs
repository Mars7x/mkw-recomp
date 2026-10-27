use mkw_arm64::{vector_integer::Halving, Op};
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(include31: bool) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for q in 0..2 {
        for size in 0..3 {
            for base in [0x0e200400, 0x0e202400, 0x2e200400] {
                for (dst, lhs, rhs) in [
                    (0, 1, 2),
                    (0, 0, 2),
                    (0, 1, 0),
                    (0, 0, 0),
                    (31, 31, 31),
                    (31, 1, 31),
                ] {
                    if !include31 && dst == 31 {
                        continue;
                    }
                    out.insert(base | (q << 30) | (size << 22) | (rhs << 16) | (lhs << 5) | dst);
                }
            }
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("halving_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> Halving {
    let Op::HalvingInteger(v) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    v
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
fn oracle(op: Halving, before: &[[u64; 2]; 32], initial: u32) -> (usize, [u64; 2], u32) {
    let Halving {
        bytes,
        total,
        dst,
        lhs,
        rhs,
        signed,
        subtract,
    } = op;
    let bytes = usize::from(bytes);
    let mut output = [0u8; 16];
    let left = raw(before[usize::from(lhs)]);
    let right = raw(before[usize::from(rhs)]);
    let mask = (1i128 << (bytes * 8)) - 1;
    for lane in 0..usize::from(total) / bytes {
        let start = lane * bytes;
        let number = |source: &[u8; 16]| {
            let n = source[start..start + bytes]
                .iter()
                .enumerate()
                .map(|(n, &v)| i128::from(v) << (n * 8))
                .sum::<i128>();
            if signed && source[start + bytes - 1] & 128 != 0 {
                n - (1i128 << (bytes * 8))
            } else {
                n
            }
        };
        let a = number(&left);
        let b = number(&right);
        let wide = if subtract { a - b } else { a + b };
        let value = wide.div_euclid(2) & mask;
        for byte in 0..bytes {
            output[start + byte] = (value >> (byte * 8)) as u8;
        }
    }
    (usize::from(dst), pack(output), initial)
}
fn pairs(bits: u32) -> Vec<(u64, u64)> {
    let min = -(1i128 << (bits - 1));
    let max = (1i128 << (bits - 1)) - 1;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let values = [min, min + 1, -1, 0, 1, max - 1, max];
    let mut pairs = Vec::new();
    for a in values {
        for b in values {
            pairs.push(((a as u64) & mask, (b as u64) & mask));
        }
    }
    let mut x = 0x8c4af101cab87365u64;
    for _ in 0..16 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let a = x & mask;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        pairs.push((a, x & mask));
    }
    pairs
}
fn inputs(op: Halving, a: u64, b: u64, reverse: bool) -> [[u64; 2]; 32] {
    let Halving {
        bytes, lhs, rhs, ..
    } = op;
    let size = usize::from(bytes);
    let mut frame = std::array::from_fn(|r| {
        [
            0x0123456789abcdefu64.wrapping_add(r as u64),
            0xfedcba9876543210u64.wrapping_sub(r as u64),
        ]
    });
    let vector = |v: u64| {
        let mut data = [0u8; 16];
        for lane in 0..16 / size {
            let value = v.wrapping_add(if reverse {
                (16 / size - 1 - lane) as u64
            } else {
                lane as u64
            });
            for byte in 0..size {
                data[lane * size + byte] = (value >> (byte * 8)) as u8;
            }
        }
        pack(data)
    };
    frame[usize::from(lhs)] = vector(a);
    frame[usize::from(rhs)] = vector(b);
    frame
}
#[test]
fn halving_match_independent_i128() {
    let path = folder("halving-known");
    let words = words(true);
    let code = codes(&words);
    let mut cases = 0;
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let mut count = 0;
        for &w in &words {
            let op = decoded(w);
            let Halving { bytes, .. } = op;
            let native = unsafe {
                library
                    .get::<Entry>(format!("halving_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b) in pairs(u32::from(bytes) * 8) {
                for reverse in [false, true] {
                    for qc in 0..2 {
                        let before = inputs(op, a, b, reverse);
                        let initial = 0x9f | (qc << 27);
                        let mut cpu = Cpu {
                            sp: 99,
                            n: 1,
                            z: 0,
                            c: 1,
                            v: 1,
                            fpcr: 0x02c00000,
                            fpsr: initial,
                            vregs: before,
                            ..Cpu::default()
                        };
                        cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                        let x = cpu.x;
                        let (dst, value, flags) = oracle(op, &before, initial);
                        let mut expected = before;
                        expected[dst] = value;
                        assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                        assert_eq!(
                            cpu.vregs, expected,
                            "{opt} {w:x} {a:x} {b:x} reverse{reverse}"
                        );
                        assert_eq!(cpu.fpsr, flags, "{opt} {w:x} {a:x} {b:x}");
                        assert_eq!(cpu.x, x);
                        assert_eq!(
                            (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr),
                            (99, 1, 0, 1, 1, 0x02c00000)
                        );
                        count += 1;
                    }
                }
            }
        }
        if cases == 0 {
            cases = count;
        } else {
            assert_eq!(count, cases);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"cases_per_optimization":cases,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn halving_match_synthetic_qemu() {
    let path = folder("halving-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\"}\n",
        )
        .unwrap();
        return;
    }
    let words = words(false);
    let mut cases = Vec::new();
    for &w in &words {
        let op = decoded(w);
        let Halving { bytes, .. } = op;
        for (a, b) in pairs(u32::from(bytes) * 8) {
            for reverse in [false, true] {
                for qc in 0..2 {
                    let frame = inputs(op, a, b, reverse);
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame[..4].try_into().unwrap(),
                        fpcr: 0x02c00000,
                        fpsr: 0x9f | (qc << 27),
                        nzcv: 0xb0000000,
                    });
                }
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (i, chunk) in cases.chunks(2000).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{i}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .arg(&image)
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        oracle.extend(output.stdout);
    }
    assert_eq!(oracle.len(), cases.len() * 32);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("halving_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
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
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
