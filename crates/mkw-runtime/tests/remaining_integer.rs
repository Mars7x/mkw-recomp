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
        for base in [0x4ea05400, 0x4ee05400, 0x4ea05c00] {
            out.insert(base | (m << 16) | (n << 5) | d);
        }
        for q in 0..2 {
            for size in 1..3 {
                out.insert(0x0e209000 | (q << 30) | (size << 22) | (m << 16) | (n << 5) | d);
                for lane in 0..if size == 1 { 8 } else { 4 } {
                    let l = if size == 1 {
                        ((lane >> 2) << 11) | (((lane >> 1) & 1) << 21) | ((lane & 1) << 20)
                    } else {
                        ((lane >> 1) << 11) | ((lane & 1) << 21)
                    };
                    let rm = m & if size == 1 { 15 } else { 31 };
                    out.insert(
                        0x0f003000 | (q << 30) | (size << 22) | (rm << 16) | l | (n << 5) | d,
                    );
                }
            }
        }
    }
    for (d, n) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
        if !full && d == 31 {
            continue;
        }
        out.insert(0x4e606800 | (n << 5) | d);
        for q in 0..2 {
            for bits in [8, 16, 32] {
                for shift in 1..=bits {
                    out.insert(0x0f008c00 | (q << 30) | ((bits * 2 - shift) << 16) | (n << 5) | d);
                }
            }
        }
    }
    out.into_iter().collect()
}
fn decoded(w: u32) -> Op {
    mkw_arm64::decode(w, 0).unwrap().op
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("remaining_{w:x}")))
        .collect()
}
fn raw(v: [u64; 2]) -> Vec<u8> {
    v.into_iter().flat_map(u64::to_le_bytes).collect()
}
fn read(b: &[u8]) -> u128 {
    b.iter()
        .enumerate()
        .map(|(i, &v)| u128::from(v) << (i * 8))
        .sum()
}
fn signed(v: u128, bits: usize) -> i128 {
    if v & (1u128 << (bits - 1)) != 0 {
        v as i128 - (1i128 << bits)
    } else {
        v as i128
    }
}
fn vector(value: u64, bytes: usize, reverse: bool) -> [u64; 2] {
    let mask = (1u128 << (bytes * 8)) - 1;
    let mut out = Vec::new();
    for lane in 0..16 / bytes {
        let value = u128::from(value) ^ if reverse && lane % 2 != 0 { mask } else { 0 };
        out.extend_from_slice(&value.to_le_bytes()[..bytes]);
    }
    [
        u64::from_le_bytes(out[..8].try_into().unwrap()),
        u64::from_le_bytes(out[8..].try_into().unwrap()),
    ]
}
fn frame(op: Op, a: u64, b: u64, acc: u64, reverse: bool) -> [[u64; 2]; 32] {
    let mut v =
        std::array::from_fn(|r| [0x123456789abcdef0 + r as u64, 0xfedcba9876543210 + r as u64]);
    match op {
        Op::RoundedRegisterShift(p) => {
            v[usize::from(p.lhs)] = vector(a, usize::from(p.bytes), reverse);
            v[usize::from(p.rhs)] = vector(
                0x123456789abcde00 | (b & 255),
                usize::from(p.bytes),
                reverse,
            );
        }
        Op::RoundedShiftNarrow(p) => {
            v[usize::from(p.src)] = vector(a, usize::from(p.bytes) * 2, reverse)
        }
        Op::LongSignedPairAccumulate { dst, src } => {
            v[usize::from(dst)] = vector(acc, 4, reverse);
            v[usize::from(src)] = vector(a, 2, reverse);
        }
        Op::SaturatingLongAccumulate(p) => {
            v[usize::from(p.dst)] = vector(acc, usize::from(p.bytes) * 2, reverse);
            v[usize::from(p.lhs)] = vector(a, usize::from(p.bytes), reverse);
            v[usize::from(p.rhs)] = vector(b, usize::from(p.bytes), reverse);
        }
        _ => panic!(),
    };
    v
}
fn oracle(op: Op, v: &[[u64; 2]; 32], fpsr: u32) -> (usize, [u64; 2], u32) {
    let mut flags = fpsr;
    let mut out = Vec::new();
    let (dst, upper) = match op {
        Op::RoundedRegisterShift(p) => {
            let bytes = usize::from(p.bytes);
            let bits = bytes * 8;
            let a = raw(v[usize::from(p.lhs)]);
            let b = raw(v[usize::from(p.rhs)]);
            for (x, y) in a.chunks_exact(bytes).zip(b.chunks_exact(bytes)) {
                let input = read(x);
                let n = i32::from(y[0] as i8);
                let value = if n < 0 {
                    let amount = (-n) as usize;
                    if amount >= bits {
                        0
                    } else {
                        (signed(input, bits) + (1i128 << (amount - 1))).div_euclid(1i128 << amount)
                    }
                } else if p.saturating {
                    let a = signed(input, bits);
                    let min = -(1i128 << (bits - 1));
                    let max = (1i128 << (bits - 1)) - 1;
                    let wide = if n as usize >= bits {
                        if a == 0 {
                            0
                        } else if a < 0 {
                            min - 1
                        } else {
                            max + 1
                        }
                    } else {
                        a * (1i128 << n)
                    };
                    let value = wide.clamp(min, max);
                    if value != wide {
                        flags |= 1 << 27;
                    }
                    value
                } else if n as usize >= bits {
                    0
                } else {
                    ((input << (n as usize)) & ((1u128 << bits) - 1)) as i128
                };
                out.extend_from_slice(&value.to_le_bytes()[..bytes]);
            }
            (p.dst, false)
        }
        Op::RoundedShiftNarrow(p) => {
            let bytes = usize::from(p.bytes);
            for chunk in raw(v[usize::from(p.src)]).chunks_exact(bytes * 2) {
                let value = (read(chunk) + (1u128 << (p.amount - 1))) >> p.amount;
                out.extend_from_slice(&value.to_le_bytes()[..bytes]);
            }
            (p.dst, p.upper)
        }
        Op::LongSignedPairAccumulate { dst, src } => {
            let a = raw(v[usize::from(src)]);
            let c = raw(v[usize::from(dst)]);
            for (pair, acc) in a.chunks_exact(4).zip(c.chunks_exact(4)) {
                let sum =
                    signed(read(&pair[..2]), 16) + signed(read(&pair[2..]), 16) + read(acc) as i128;
                out.extend_from_slice(&sum.to_le_bytes()[..4]);
            }
            (dst, false)
        }
        Op::SaturatingLongAccumulate(p) => {
            let bytes = usize::from(p.bytes);
            let bits = bytes * 8;
            let start = if p.upper { 8 } else { 0 };
            let a = raw(v[usize::from(p.lhs)]);
            let b = raw(v[usize::from(p.rhs)]);
            let acc = raw(v[usize::from(p.dst)]);
            let min = -(1i128 << (bits * 2 - 1));
            let max = (1i128 << (bits * 2 - 1)) - 1;
            for (lane, left) in a[start..start + 8].chunks_exact(bytes).enumerate() {
                let ri = p
                    .rhs_lane
                    .map_or(start + lane * bytes, |v| usize::from(v) * bytes);
                let right = &b[ri..ri + bytes];
                let product = signed(read(left), bits) * signed(read(right), bits) * 2;
                let saturated = product.clamp(min, max);
                let sum = saturated
                    + signed(
                        read(&acc[lane * bytes * 2..(lane + 1) * bytes * 2]),
                        bits * 2,
                    );
                let value = sum.clamp(min, max);
                if product != saturated || sum != value {
                    flags |= 1 << 27;
                }
                out.extend_from_slice(&value.to_le_bytes()[..bytes * 2]);
            }
            (p.dst, false)
        }
        _ => panic!(),
    };
    if upper {
        let mut low = v[usize::from(dst)][0].to_le_bytes().to_vec();
        low.extend(out);
        out = low;
    }
    out.resize(16, 0);
    (
        usize::from(dst),
        [
            u64::from_le_bytes(out[..8].try_into().unwrap()),
            u64::from_le_bytes(out[8..].try_into().unwrap()),
        ],
        flags,
    )
}
fn inputs(op: Op) -> Vec<(u64, u64, u64)> {
    let values = |bits: u8| {
        let max = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        vec![
            0,
            1,
            2,
            max,
            max - 1,
            (1u64 << (bits - 1)),
            (1u64 << (bits - 1)) - 1,
            (1u64 << (bits - 1)) + 1,
        ]
    };
    match op {
        Op::RoundedRegisterShift(p) => {
            let bits = p.bytes * 8;
            let mut out = Vec::new();
            for count in 0..256 {
                let mut v = values(bits);
                if count < bits as u64 {
                    let threshold = ((1u64 << (bits - 1)) - 1) >> count;
                    v.extend([threshold, threshold + 1, threshold.saturating_sub(1)]);
                }
                for a in v {
                    out.push((a, count, 0));
                }
            }
            out
        }
        Op::RoundedShiftNarrow(p) => {
            let mut v = values(p.bytes * 16);
            let half = 1u64 << (p.amount - 1);
            v.extend([
                half - 1,
                half,
                half + 1,
                0xffffffff00000000,
                0x7fffffff80000000,
            ]);
            v.into_iter().map(|a| (a, 0, 0)).collect()
        }
        Op::LongSignedPairAccumulate { .. } => values(16)
            .iter()
            .flat_map(|&a| values(32).into_iter().map(move |c| (a, 0, c)))
            .collect(),
        Op::SaturatingLongAccumulate(p) => values(p.bytes * 8)
            .iter()
            .flat_map(|&a| {
                values(p.bytes * 8)
                    .into_iter()
                    .flat_map(move |b| values(p.bytes * 16).into_iter().map(move |c| (a, b, c)))
            })
            .collect(),
        _ => panic!(),
    }
}
#[test]
fn remaining_integers_match_independent_byte_i128_oracles() {
    let path = folder("remaining-integer-known");
    let words = words(true);
    let source = code(&words);
    let mut counts = [0usize; 4];
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let mut observed = [0; 4];
        for &w in &words {
            let op = decoded(w);
            let category = match op {
                Op::RoundedRegisterShift(_) => 0,
                Op::RoundedShiftNarrow(_) => 1,
                Op::LongSignedPairAccumulate { .. } => 2,
                Op::SaturatingLongAccumulate(_) => 3,
                _ => panic!(),
            };
            let f = unsafe {
                lib.get::<Entry>(format!("remaining_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b, c) in inputs(op) {
                for reverse in [false, true] {
                    for qc in 0..2 {
                        let before = frame(op, a, b, c, reverse);
                        let initial = 0x80 | (qc << 27);
                        let (d, value, flags) = oracle(op, &before, initial);
                        let mut want = before;
                        want[d] = value;
                        let mut cpu = Cpu {
                            vregs: before,
                            fpcr: u32::MAX,
                            fpsr: initial,
                            n: 1,
                            c: 1,
                            v: 1,
                            sp: 123,
                            fault_status: 77,
                            fault_address: 91,
                            ..Cpu::default()
                        };
                        cpu.x = std::array::from_fn(|n| n as u64 + 123);
                        let x = cpu.x;
                        assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
                        assert_eq!(cpu.vregs, want, "{opt} {w:x} {a:x} {b:x} {c:x} {reverse}");
                        assert_eq!(cpu.fpsr, flags, "{opt} {w:x} flags");
                        assert_eq!(cpu.x, x);
                        assert_eq!(
                            (
                                cpu.fpcr,
                                cpu.n,
                                cpu.z,
                                cpu.c,
                                cpu.v,
                                cpu.sp,
                                cpu.fault_status,
                                cpu.fault_address
                            ),
                            (u32::MAX, 1, 0, 1, 1, 123, 77, 91)
                        );
                        observed[category] += 1;
                    }
                }
            }
        }
        if counts == [0; 4] {
            counts = observed;
        } else {
            assert_eq!(counts, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"rounded_register_native_cases_per_optimization":counts[0],"rounded_narrow_native_cases_per_optimization":counts[1],"pair_accumulate_native_cases_per_optimization":counts[2],"saturating_long_native_cases_per_optimization":counts[3],"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn remaining_integers_match_synthetic_qemu() {
    let path = folder("remaining-integer-reference");
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
        for (a, b, c) in inputs(op) {
            for reverse in [false, true] {
                for qc in 0..2 {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame(op, a, b, c, reverse)[..4].try_into().unwrap(),
                        fpcr: 0x02c00000,
                        fpsr: 0x80 | (qc << 27),
                        nzcv: 0xb0000000,
                    });
                }
            }
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
                lib.get::<Entry>(format!("remaining_{:x}\0", c.word).as_bytes())
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
            let (d, value, flags) = self::oracle(decoded(c.word), &want, c.fpsr);
            want[d] = value;
            assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
            assert_eq!(cpu.vregs, want);
            assert_eq!(cpu.fpsr, flags);
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
#[test]
fn remaining_integers_obey_shared_exact_budgets() {
    let path = folder("remaining-integer-budgets");
    let words = words(true);
    let source = code(&words);
    type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        for &w in &words {
            let f = unsafe {
                lib.get::<BudgetEntry>(format!("remaining_{w:x}_budget\0").as_bytes())
                    .unwrap()
            };
            let op = decoded(w);
            for initial in 0..3 {
                let before = frame(op, 0x8000000080008000, 0xff, 0x7fffffff7fffffff, true);
                let mut want = before;
                let flags = if initial > 0 {
                    let (d, result, flags) = oracle(op, &before, 0x80);
                    want[d] = result;
                    flags
                } else {
                    0x80
                };
                let mut cpu = Cpu {
                    vregs: before,
                    fpcr: u32::MAX,
                    fpsr: 0x80,
                    module_base: 0x8000,
                    fault_status: 77,
                    fault_address: 91,
                    ..Cpu::default()
                };
                let mut budget = initial;
                assert_eq!(
                    unsafe { f(&mut cpu, &mut budget) },
                    if initial < 2 { 2 } else { 0 }
                );
                assert_eq!(budget, 0);
                assert_eq!(cpu.vregs, want);
                assert_eq!(cpu.fpsr, flags);
                assert_eq!(
                    (cpu.fpcr, cpu.fault_status, cpu.fault_address),
                    (u32::MAX, 77, 91)
                );
                if initial < 2 {
                    assert_eq!(cpu.pc, 0x8000 + initial * 4);
                }
            }
        }
    }
}
