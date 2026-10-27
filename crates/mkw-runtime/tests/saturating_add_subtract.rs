use mkw_arm64::{vector_integer::VectorInteger, Op};
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(include31: bool) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for (scalar, q) in [(0, 0), (0, 1), (1, 1)] {
        for size in 0..4 {
            if scalar == 0 && q == 0 && size == 3 {
                continue;
            }
            for base in [0x0e200c00, 0x2e200c00, 0x0e202c00, 0x2e202c00] {
                for (dst, lhs, rhs) in [
                    (0, 1, 2),
                    (0, 0, 2),
                    (0, 1, 0),
                    (0, 0, 0),
                    (31, 31, 31),
                    (31, 1, 31),
                ] {
                    if !include31 && (dst == 31 || lhs == 31 || rhs == 31) {
                        continue;
                    }
                    out.insert(
                        base | (scalar << 28)
                            | (q << 30)
                            | (size << 22)
                            | (rhs << 16)
                            | (lhs << 5)
                            | dst,
                    );
                }
            }
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("saturating_add_subtract_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> VectorInteger {
    let Op::VectorInteger(v) = mkw_arm64::decode(w, 0).unwrap().op else {
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
fn signed(v: u128, bits: u32) -> i128 {
    if v & (1u128 << (bits - 1)) != 0 {
        v as i128 - (1i128 << bits)
    } else {
        v as i128
    }
}
fn oracle(op: VectorInteger, before: &[[u64; 2]; 32], initial: u32) -> (usize, [u64; 2], u32) {
    let VectorInteger::Saturating {
        bytes,
        total,
        dst,
        lhs,
        rhs,
        signed: sign,
        subtract,
    } = op
    else {
        panic!()
    };
    let b = usize::from(bytes);
    let bits = u32::from(bytes) * 8;
    let left = raw(before[usize::from(lhs)]);
    let right = raw(before[usize::from(rhs)]);
    let mut output = [0u8; 16];
    let mut flags = initial;
    let value = |v: &[u8]| {
        v.iter()
            .enumerate()
            .map(|(n, &byte)| u128::from(byte) << (n * 8))
            .sum::<u128>()
    };
    let min = if sign { -(1i128 << (bits - 1)) } else { 0 };
    let max = if sign {
        (1i128 << (bits - 1)) - 1
    } else {
        (1i128 << bits) - 1
    };
    for lane in 0..usize::from(total) / b {
        let convert = |v: u128| if sign { signed(v, bits) } else { v as i128 };
        let a = convert(value(&left[lane * b..(lane + 1) * b]));
        let c = convert(value(&right[lane * b..(lane + 1) * b]));
        let result = if subtract { a - c } else { a + c };
        let clipped = result.clamp(min, max);
        if clipped != result {
            flags |= 1 << 27;
        }
        for byte in 0..b {
            output[lane * b + byte] = ((clipped as u128) >> (byte * 8)) as u8;
        }
    }
    (usize::from(dst), pack(output), flags)
}
fn pairs(bits: u32) -> Vec<(u64, u64)> {
    let min = -(1i128 << (bits - 1));
    let max = (1i128 << (bits - 1)) - 1;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let values = [
        min,
        min + 1,
        min / 2,
        -3,
        -2,
        -1,
        0,
        1,
        2,
        3,
        max / 2,
        max - 1,
        max,
    ];
    let mut pairs = Vec::new();
    for a in values {
        for b in values {
            pairs.push(((a as u64) & mask, (b as u64) & mask));
        }
    }
    let mut x = 0x8c4af101cab87365u64;
    for _ in 0..32 {
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
fn inputs(op: VectorInteger, a: u64, b: u64, reverse: bool) -> [[u64; 2]; 32] {
    let VectorInteger::Saturating {
        bytes,
        total,
        lhs,
        rhs,
        ..
    } = op
    else {
        panic!()
    };
    let size = usize::from(bytes);
    let used = usize::from(total) / size;
    let mut frame = std::array::from_fn(|r| {
        [
            0x0123456789abcdefu64.wrapping_add(r as u64),
            0xfedcba9876543210u64.wrapping_sub(r as u64),
        ]
    });
    let vector = |v: u64| {
        let mut out = [0u8; 16];
        for lane in 0..16 / size {
            let delta = if lane < used && reverse {
                used - 1 - lane
            } else {
                lane
            };
            let value = v.wrapping_add(delta as u64);
            for byte in 0..size {
                out[lane * size + byte] = (value >> (byte * 8)) as u8;
            }
        }
        pack(out)
    };
    frame[usize::from(lhs)] = vector(a);
    frame[usize::from(rhs)] = vector(b);
    frame
}
#[test]
fn saturating_add_subtract_match_independent_i128_and_sticky_qc() {
    let path = folder("saturating-add-subtract-known");
    let words = words(true);
    let code = codes(&words);
    let mut cases = 0;
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let mut count = 0;
        for &w in &words {
            let op = decoded(w);
            let VectorInteger::Saturating { bytes, .. } = op else {
                panic!()
            };
            let native = unsafe {
                library
                    .get::<Entry>(format!("saturating_add_subtract_{w:x}\0").as_bytes())
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
fn saturating_add_subtract_match_synthetic_qemu() {
    let path = folder("saturating-add-subtract-reference");
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
        let VectorInteger::Saturating { bytes, .. } = op else {
            panic!()
        };
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
                    .get::<Entry>(format!("saturating_add_subtract_{:x}\0", c.word).as_bytes())
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
