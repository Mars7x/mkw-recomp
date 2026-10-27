use mkw_arm64::{
    simd_shift::{Kind, Shift},
    Op,
};
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(include31: bool) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for (scalar, q, bytes) in [
        (0, 0, 1),
        (0, 0, 2),
        (0, 0, 4),
        (0, 1, 1),
        (0, 1, 2),
        (0, 1, 4),
        (0, 1, 8),
        (1, 1, 8),
    ] {
        let bits = bytes * 8;
        for base in [0x0f005400, 0x2f005400, 0x0f000400, 0x2f000400, 0x2f004400] {
            let right = base & 0x5000 != 0x5000;
            for amount in if right { 1..bits + 1 } else { 0..bits } {
                let imm = if right {
                    2 * bits - amount
                } else {
                    bits + amount
                };
                for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                    if !include31 && dst == 31 {
                        continue;
                    }
                    out.insert(base | (scalar << 28) | (q << 30) | (imm << 16) | (src << 5) | dst);
                }
            }
        }
    }
    for bytes in [1, 2, 4] {
        let bits = bytes * 8;
        for upper in 0..2 {
            for base in [0x0f00a400, 0x2f00a400] {
                for amount in 0..bits {
                    for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                        if !include31 && dst == 31 {
                            continue;
                        }
                        out.insert(
                            base | (upper << 30) | ((bits + amount) << 16) | (src << 5) | dst,
                        );
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("simd_shift_{w:x}")))
        .collect()
}
fn signed(a: u128, bits: u32) -> i128 {
    if a & (1u128 << (bits - 1)) != 0 {
        a as i128 - (1i128 << bits)
    } else {
        a as i128
    }
}
fn oracle(op: Shift, before: &[[u64; 2]; 32]) -> [u64; 2] {
    let source = before[usize::from(op.src)];
    let previous = before[usize::from(op.dst)];
    let bits = u32::from(op.bytes) * 8;
    let widen = matches!(op.kind, Kind::UnsignedWiden | Kind::SignedWiden);
    let out_bits = if widen { bits * 2 } else { bits };
    let output_mask = (1u128 << out_bits) - 1;
    let source: Vec<_> = source.into_iter().flat_map(u64::to_le_bytes).collect();
    let old: Vec<_> = previous.into_iter().flat_map(u64::to_le_bytes).collect();
    let start = if op.upper { 8 } else { 0 };
    let input_total = if widen { 8 } else { usize::from(op.total) };
    let mut raw = [0; 16];
    for (lane, chunk) in source[start..start + input_total]
        .chunks(usize::from(op.bytes))
        .enumerate()
    {
        let a: u128 = chunk
            .iter()
            .enumerate()
            .map(|(i, b)| u128::from(*b) << (i * 8))
            .sum();
        let out_bytes = (out_bits / 8) as usize;
        let previous: u128 = old[lane * out_bytes..(lane + 1) * out_bytes]
            .iter()
            .enumerate()
            .map(|(i, b)| u128::from(*b) << (i * 8))
            .sum();
        let n = u32::from(op.amount);
        let value = match op.kind {
            Kind::Left | Kind::UnsignedWiden => a << n,
            Kind::UnsignedRight => a >> n,
            Kind::SignedRight => (signed(a, bits) >> n) as u128,
            Kind::SignedWiden => (signed(a, bits) << n) as u128,
            Kind::InsertLeft => {
                let mut v = 0u128;
                for bit in 0..bits {
                    let selected = if bit < n {
                        (previous >> bit) & 1
                    } else {
                        (a >> (bit - n)) & 1
                    };
                    v |= selected << bit;
                }
                v
            }
            Kind::InsertRight => {
                let mut v = 0u128;
                for bit in 0..bits {
                    let selected = if bit >= bits - n {
                        (previous >> bit) & 1
                    } else {
                        (a >> (bit + n)) & 1
                    };
                    v |= selected << bit;
                }
                v
            }
        } & output_mask;
        for i in 0..out_bytes {
            raw[lane * out_bytes + i] = (value >> (i * 8)) as u8;
        }
    }
    [
        u64::from_le_bytes(raw[..8].try_into().unwrap()),
        u64::from_le_bytes(raw[8..].try_into().unwrap()),
    ]
}
fn values() -> Vec<u64> {
    vec![
        0,
        u64::MAX,
        1,
        1 << 63,
        0x7fffffffffffffff,
        0x8080808080808080,
        0x7f7f7f7f7f7f7f7f,
        0x8000800080008000,
        0x8000000080000000,
        0xff00aa5580017ffe,
        0x0123456789abcdef,
        0xfedcba9876543210,
    ]
}
#[test]
fn immediate_shifts_and_widening_match_independent_wide_bit_math() {
    let path = folder("simd-shift-known");
    let words = words(true);
    let code = codes(&words);
    let values = values();
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for w in &words {
            let Op::SimdShift(op) = mkw_arm64::decode(*w, 0).unwrap().op else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("simd_shift_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for i in 0..values.len() {
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpcr: 0x02c00000,
                    fpsr: 0x0800009f,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                    *v = [
                        values[(i + reg * 3) % values.len()],
                        values[(i + reg * 3 + 5) % values.len()],
                    ];
                }
                let before = cpu.vregs;
                let general = cpu.x;
                let expected = oracle(op, &before);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                for (reg, &old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == usize::from(op.dst) {
                            expected
                        } else {
                            old
                        },
                        "{opt} {w:x} pattern{i} v{reg}"
                    );
                }
                assert_eq!(cpu.x, general);
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x02c00000, 0x0800009f)
                );
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"cases_per_optimization":words.len()*values.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true})).unwrap()).unwrap();
}
#[test]
fn immediate_shift_and_widening_results_match_qemu() {
    let path = folder("simd-shift-reference");
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
    let values = values();
    for w in &words {
        for i in 0..values.len() {
            cases.push(reference::VectorCase {
                word: *w,
                vectors: std::array::from_fn(|r| {
                    [
                        values[(i + r * 3) % values.len()],
                        values[(i + r * 3 + 5) % values.len()],
                    ]
                }),
                fpcr: 0x02c00000,
                fpsr: 0x0800009f,
                nzcv: 0xb0000000,
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
        let lib = build(&code, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("simd_shift_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
            let actual: Vec<_> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                u64::from(cpu.fpsr),
                0xb0000000,
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(
                actual,
                &oracle[i * 32..(i + 1) * 32],
                "{opt} fixture{i} {:x}",
                c.word
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
