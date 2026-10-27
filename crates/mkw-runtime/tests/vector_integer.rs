use mkw_arm64::{
    vector_integer::{Kind, Reduction, VectorInteger},
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
    let mut words = BTreeSet::new();
    for (scalar, q, size) in [
        (0, 0, 0),
        (0, 0, 1),
        (0, 0, 2),
        (0, 1, 0),
        (0, 1, 1),
        (0, 1, 2),
        (0, 1, 3),
        (1, 1, 3),
    ] {
        let fields = (scalar << 28) | (q << 30) | (size << 22);
        for base in [
            0x0e208400, 0x2e208400, 0x2e208c00, 0x0e208c00, 0x0e203c00, 0x0e203400, 0x2e203c00,
            0x2e203400,
        ] {
            for (rd, rn, rm) in [
                (0, 1, 2),
                (0, 0, 2),
                (0, 1, 0),
                (0, 0, 0),
                (31, 31, 31),
                (31, 1, 31),
            ] {
                if !include31 && (rd == 31 || rn == 31 || rm == 31) {
                    continue;
                }
                words.insert(base | fields | (rm << 16) | (rn << 5) | rd);
            }
        }
        for base in [
            0x0e209800, 0x2e208800, 0x0e208800, 0x2e209800, 0x0e20a800, 0x2e20b800,
        ] {
            for (rd, rn) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                if !include31 && rd == 31 {
                    continue;
                }
                words.insert(base | fields | (rn << 5) | rd);
            }
        }
    }
    for q in 0..2 {
        for size in 0..3 {
            for base in [0x0e206400, 0x0e206c00, 0x2e206400, 0x2e206c00] {
                for (rd, rn, rm) in [
                    (0, 1, 2),
                    (0, 0, 2),
                    (0, 1, 0),
                    (0, 0, 0),
                    (31, 31, 31),
                    (31, 1, 31),
                ] {
                    if !include31 && (rd == 31 || rn == 31 || rm == 31) {
                        continue;
                    }
                    words.insert(base | (q << 30) | (size << 22) | (rm << 16) | (rn << 5) | rd);
                }
            }
        }
    }
    for (q, size) in [(0, 0), (0, 1), (1, 0), (1, 1), (1, 2)] {
        for base in [
            0x0e303800, 0x2e303800, 0x0e31b800, 0x0e30a800, 0x0e31a800, 0x2e30a800, 0x2e31a800,
        ] {
            for (rd, rn) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                if !include31 && rd == 31 {
                    continue;
                }
                words.insert(base | (q << 30) | (size << 22) | (rn << 5) | rd);
            }
        }
    }
    words.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("integer_vector_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> VectorInteger {
    let Op::VectorInteger(v) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    v
}
fn mask(bits: u32) -> u128 {
    (1u128 << bits) - 1
}
fn signed(value: u128, bits: u32) -> i128 {
    if value & (1u128 << (bits - 1)) != 0 {
        value as i128 - (1i128 << bits)
    } else {
        value as i128
    }
}
fn elements(v: [u64; 2], bytes: u8, total: u8) -> Vec<u128> {
    let raw: Vec<_> = v.into_iter().flat_map(u64::to_le_bytes).collect();
    raw[..usize::from(total)]
        .chunks(usize::from(bytes))
        .map(|b| {
            b.iter()
                .enumerate()
                .map(|(i, &v)| u128::from(v) << (i * 8))
                .sum()
        })
        .collect()
}
fn oracle(op: VectorInteger, input: &[[u64; 2]; 32]) -> (usize, [u64; 2]) {
    let mut raw = [0u8; 16];
    let (dst, values, bytes) = match op {
        VectorInteger::WidenArithmetic { .. }
        | VectorInteger::SaturatingShiftNarrow { .. }
        | VectorInteger::RegisterShift { .. }
        | VectorInteger::SaturatingNarrow { .. }
        | VectorInteger::Saturating { .. }
        | VectorInteger::WidenMultiply { .. }
        | VectorInteger::MultiplyHigh { .. } => {
            unreachable!("saturating high products have a separate i128/QC oracle")
        }
        VectorInteger::Lanes {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            kind,
        } => {
            let bits = u32::from(bytes) * 8;
            let left = elements(input[usize::from(lhs)], bytes, total);
            let right = rhs
                .map(|r| elements(input[usize::from(r)], bytes, total))
                .unwrap_or(vec![0; left.len()]);
            let values = left
                .iter()
                .zip(right)
                .map(|(&a, b)| match kind {
                    Kind::Add => ((a as i128 + b as i128) as u128) & mask(bits),
                    Kind::Subtract => ((a as i128 - b as i128) as u128) & mask(bits),
                    Kind::Negate => (-signed(a, bits) as u128) & mask(bits),
                    Kind::UnsignedMax => a.max(b),
                    Kind::UnsignedMin => a.min(b),
                    Kind::SignedMax => (signed(a, bits).max(signed(b, bits)) as u128) & mask(bits),
                    Kind::SignedMin => (signed(a, bits).min(signed(b, bits)) as u128) & mask(bits),
                    _ => {
                        let yes = match kind {
                            Kind::Equal => a == b,
                            Kind::Test => a & b != 0,
                            Kind::UnsignedGe => a >= b,
                            Kind::UnsignedGt => a > b,
                            Kind::SignedGe => signed(a, bits) >= signed(b, bits),
                            Kind::SignedGt => signed(a, bits) > signed(b, bits),
                            Kind::SignedLe => signed(a, bits) <= signed(b, bits),
                            Kind::SignedLt => signed(a, bits) < signed(b, bits),
                            _ => unreachable!(),
                        };
                        if yes {
                            mask(bits)
                        } else {
                            0
                        }
                    }
                })
                .collect::<Vec<_>>();
            (dst, values, bytes)
        }
        VectorInteger::Reduce {
            bytes,
            total,
            dst,
            src,
            kind,
        } => {
            let bits = u32::from(bytes) * 8;
            let values = elements(input[usize::from(src)], bytes, total);
            let output_bytes = if !matches!(kind, Reduction::SignedLong | Reduction::UnsignedLong) {
                bytes
            } else {
                bytes * 2
            };
            let value = if matches!(kind, Reduction::SignedMax | Reduction::SignedMin) {
                let v = values.into_iter().map(|v| signed(v, bits));
                (if kind == Reduction::SignedMax {
                    v.max().unwrap()
                } else {
                    v.min().unwrap()
                }) as u128
            } else if matches!(kind, Reduction::UnsignedMax | Reduction::UnsignedMin) {
                if kind == Reduction::UnsignedMax {
                    values.into_iter().max().unwrap()
                } else {
                    values.into_iter().min().unwrap()
                }
            } else if kind == Reduction::SignedLong {
                values.into_iter().map(|v| signed(v, bits)).sum::<i128>() as u128
            } else {
                values.into_iter().sum::<u128>()
            };
            (
                dst,
                vec![value & mask(u32::from(output_bytes) * 8)],
                output_bytes,
            )
        }
    };
    for (i, value) in values.into_iter().enumerate() {
        for byte in 0..usize::from(bytes) {
            raw[i * usize::from(bytes) + byte] = (value >> (byte * 8)) as u8;
        }
    }
    (
        usize::from(dst),
        [
            u64::from_le_bytes(raw[..8].try_into().unwrap()),
            u64::from_le_bytes(raw[8..].try_into().unwrap()),
        ],
    )
}
fn patterns() -> Vec<u64> {
    let mut v = vec![
        0,
        u64::MAX,
        1,
        1 << 63,
        (1 << 63) - 1,
        0x8080808080808080,
        0x7f7f7f7f7f7f7f7f,
        0x8000800080008000,
        0x7fff7fff7fff7fff,
        0x8000000080000000,
        0x7fffffff7fffffff,
        0xff00ff00ff00ff00,
        0x0101010101010101,
        0x0001000100010001,
        0x0000000100000001,
        0x0123456789abcdef,
    ];
    let mut x = 0xa175ea0f318ca001u64;
    for _ in 0..32 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.push(x);
    }
    v
}
fn frame(values: &[u64], i: usize, reg: usize) -> [u64; 2] {
    let extremes = [
        0,
        u64::MAX,
        0x8080808080808080,
        0x8000800080008000,
        0x8000000080000000,
        0x8000000000000000,
    ];
    if i < extremes.len() {
        [extremes[i]; 2]
    } else {
        [
            values[(i + reg * 3) % values.len()],
            values[(i + reg * 3 + 7) % values.len()],
        ]
    }
}
#[test]
fn integer_lanes_masks_and_reductions_match_wider_math() {
    let path = folder("integer-vector-known");
    let words = words(true);
    let code = codes(&words);
    let values = patterns();
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for w in &words {
            let op = decoded(*w);
            let native = unsafe {
                lib.get::<Entry>(format!("integer_vector_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for i in 0..values.len() {
                let initial_fpsr = if i % 2 == 0 { 0x0800009f } else { 0x0000009f };
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpcr: 0x02c00000,
                    fpsr: initial_fpsr,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                    *v = frame(&values, i, reg);
                }
                let before = cpu.vregs;
                let general = cpu.x;
                let (dst, expected) = oracle(op, &before);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                for (reg, old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == dst { expected } else { *old },
                        "{opt} {w:x} pattern {i} v{reg}"
                    );
                }
                assert_eq!(cpu.x, general);
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x02c00000, initial_fpsr)
                );
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"cases_per_optimization":words.len()*values.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true})).unwrap()).unwrap();
}
#[test]
fn integer_vector_results_match_synthetic_qemu() {
    let path = folder("integer-vector-reference");
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
    let values = patterns();
    let mut cases = Vec::new();
    for w in &words {
        for i in 0..values.len() {
            cases.push(reference::VectorCase {
                word: *w,
                vectors: std::array::from_fn(|r| frame(&values, i, r)),
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
                lib.get::<Entry>(format!("integer_vector_{:x}\0", c.word).as_bytes())
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
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(
                actual,
                &oracle[i * 32..(i + 1) * 32],
                "{opt} fixture {i} {:x}",
                c.word
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
