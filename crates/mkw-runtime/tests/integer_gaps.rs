use mkw_arm64::Op;
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(full: bool) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for base in [0x0ea0a400, 0x0ea0ac00] {
        for (d, n, m) in [
            (0, 1, 2),
            (0, 0, 2),
            (0, 1, 0),
            (0, 0, 0),
            (31, 31, 31),
            (31, 1, 31),
        ] {
            if !full && d == 31 {
                continue;
            }
            out.insert(base | (m << 16) | (n << 5) | d);
        }
    }
    for (d, n) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
        if !full && d == 31 {
            continue;
        }
        for q in 0..2 {
            for size in 0..3 {
                out.insert(0x0e202800 | (q << 30) | (size << 22) | (n << 5) | d);
            }
        }
        for amount in 0..32 {
            out.insert(0x4f007400 | ((32 + amount) << 16) | (n << 5) | d);
        }
    }
    out.into_iter().collect()
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("integer_gap_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> Op {
    mkw_arm64::decode(w, 0).unwrap().op
}
fn layout(op: Op) -> (u8, u8, u8, u8, Option<u8>) {
    match op {
        Op::PairwiseMinMax(v) => (v.bytes, v.total, v.dst, v.lhs, Some(v.rhs)),
        Op::PairwiseLongSum(v) => (v.bytes, v.total, v.dst, v.src, None),
        Op::SaturatingShiftImmediate(v) => (v.bytes, v.total, v.dst, v.src, None),
        _ => panic!(),
    }
}
fn raw(v: [u64; 2]) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&v[0].to_le_bytes());
    b[8..].copy_from_slice(&v[1].to_le_bytes());
    b
}
fn pack(b: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(b[..8].try_into().unwrap()),
        u64::from_le_bytes(b[8..].try_into().unwrap()),
    ]
}
fn elements(v: [u64; 2], bytes: usize, total: usize) -> Vec<i128> {
    raw(v)[..total]
        .chunks_exact(bytes)
        .map(|chunk| {
            let value = chunk
                .iter()
                .enumerate()
                .map(|(n, &v)| i128::from(v) << (n * 8))
                .sum::<i128>();
            if chunk[bytes - 1] & 128 != 0 {
                value - (1i128 << (bytes * 8))
            } else {
                value
            }
        })
        .collect()
}
fn oracle(op: Op, before: &[[u64; 2]; 32], initial: u32) -> (usize, [u64; 2], u32) {
    let (b, total, d, n, m) = layout(op);
    let b = usize::from(b);
    let total = usize::from(total);
    let left = elements(before[usize::from(n)], b, total);
    let (mut width, mut flags) = (b, initial);
    let result: Vec<i128> = match op {
        Op::PairwiseMinMax(v) => {
            let right = elements(before[usize::from(m.unwrap())], b, total);
            left.chunks_exact(2)
                .chain(right.chunks_exact(2))
                .map(|pair| {
                    if v.minimum {
                        pair[0].min(pair[1])
                    } else {
                        pair[0].max(pair[1])
                    }
                })
                .collect()
        }
        Op::PairwiseLongSum(_) => {
            width *= 2;
            left.chunks_exact(2).map(|pair| pair[0] + pair[1]).collect()
        }
        Op::SaturatingShiftImmediate(v) => {
            let min = -(1i128 << (b * 8 - 1));
            let max = (1i128 << (b * 8 - 1)) - 1;
            left.iter()
                .map(|&value| {
                    let shifted = value * (1i128 << v.amount);
                    let clipped = shifted.clamp(min, max);
                    if clipped != shifted {
                        flags |= 1 << 27;
                    }
                    clipped
                })
                .collect()
        }
        _ => panic!(),
    };
    let mut bytes = [0u8; 16];
    for (lane, value) in result.into_iter().enumerate() {
        for byte in 0..width {
            bytes[lane * width + byte] = ((value as u128) >> (byte * 8)) as u8;
        }
    }
    (usize::from(d), pack(bytes), flags)
}
fn input(op: Op, a: u64, b: u64, reverse: bool) -> [[u64; 2]; 32] {
    let (bytes, _, _, n, m) = layout(op);
    let bytes = usize::from(bytes);
    let mut f = std::array::from_fn(|n| [n as u64 + 17, 0xfedcba9876543210]);
    let vector = |value: u64| {
        let mut out = [0u8; 16];
        for lane in 0..16 / bytes {
            let delta = if reverse { 16 / bytes - 1 - lane } else { lane };
            let value = value.wrapping_add(delta as u64);
            for byte in 0..bytes {
                out[lane * bytes + byte] = (value >> (byte * 8)) as u8;
            }
        }
        pack(out)
    };
    f[usize::from(n)] = vector(a);
    if let Some(m) = m {
        f[usize::from(m)] = vector(b);
    }
    f
}
fn inputs(op: Op) -> Vec<(u64, u64)> {
    let (bytes, _, _, _, m) = layout(op);
    let bits = bytes * 8;
    let min = -(1i128 << (bits - 1));
    let max = (1i128 << (bits - 1)) - 1;
    let mut values = BTreeSet::from([min, min + 1, -1, 0, 1, max - 1, max]);
    if let Op::SaturatingShiftImmediate(v) = op {
        let scale = 1i128 << v.amount;
        for boundary in [min / scale, max / scale] {
            for delta in [-1, 0, 1] {
                values.insert(boundary + delta);
            }
        }
    }
    let mask = (1u64 << bits) - 1;
    let mut state = 0x8c4af101cab87365u64;
    for _ in 0..16 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        values.insert(i128::from(state & mask));
    }
    let values: Vec<_> = values.into_iter().map(|v| (v as u64) & mask).collect();
    if m.is_none() {
        values.into_iter().map(|a| (a, 0)).collect()
    } else {
        values
            .iter()
            .flat_map(|&a| values.iter().map(move |&b| (a, b)))
            .collect()
    }
}
#[test]
fn integer_gap_operations_match_independent_byte_i128() {
    let path = folder("integer-gaps-known");
    let words = words(true);
    let source = code(&words);
    let mut counts = [0usize; 3];
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let mut observed = [0usize; 3];
        for &w in &words {
            let op = decoded(w);
            let category = match op {
                Op::PairwiseMinMax(_) => 0,
                Op::PairwiseLongSum(_) => 1,
                Op::SaturatingShiftImmediate(_) => 2,
                _ => panic!(),
            };
            let native = unsafe {
                library
                    .get::<Entry>(format!("integer_gap_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b) in inputs(op) {
                for reverse in [false, true] {
                    for qc in 0..2 {
                        let before = input(op, a, b, reverse);
                        let initial = 0x80 | (qc << 27);
                        let mut cpu = Cpu {
                            sp: 99,
                            n: 1,
                            c: 1,
                            v: 1,
                            fpcr: u32::MAX,
                            fpsr: initial,
                            fault_status: 77,
                            fault_address: 0x1234,
                            vregs: before,
                            ..Cpu::default()
                        };
                        cpu.x = std::array::from_fn(|n| n as u64 + 123);
                        let x = cpu.x;
                        let (d, value, flags) = oracle(op, &before, initial);
                        let mut expected = before;
                        expected[d] = value;
                        assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                        assert_eq!(cpu.vregs, expected, "{opt} {w:x} {a:x}/{b:x}/{reverse}");
                        assert_eq!(cpu.fpsr, flags, "{opt} flags {w:x} {a:x}/{b:x}");
                        assert_eq!(cpu.x, x);
                        assert_eq!(
                            (
                                cpu.sp,
                                cpu.n,
                                cpu.z,
                                cpu.c,
                                cpu.v,
                                cpu.fpcr,
                                cpu.fault_status,
                                cpu.fault_address
                            ),
                            (99, 1, 0, 1, 1, u32::MAX, 77, 0x1234)
                        );
                        observed[category] += 1;
                    }
                }
            }
        }
        if counts == [0; 3] {
            counts = observed;
        } else {
            assert_eq!(counts, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"pairwise_minmax_native_cases_per_optimization":counts[0],"long_pair_sum_native_cases_per_optimization":counts[1],"saturating_immediate_native_cases_per_optimization":counts[2],"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn integer_gap_operations_match_synthetic_qemu() {
    let path = folder("integer-gaps-reference");
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
        let op = decoded(w);
        for (a, b) in inputs(op) {
            for reverse in [false, true] {
                for qc in 0..2 {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: input(op, a, b, reverse)[..4].try_into().unwrap(),
                        fpcr: 0x02c00000,
                        fpsr: 0x80 | (qc << 27),
                        nzcv: 0xb0000000,
                    });
                }
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
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
        let library = build(&source, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("integer_gap_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                c: 1,
                v: 1,
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            let (_, value, flags) = self::oracle(decoded(c.word), &before, c.fpsr);
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
            assert_eq!((cpu.vregs[0], cpu.fpsr), (value, flags));
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
