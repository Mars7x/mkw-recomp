use mkw_arm64::{vector_integer::UnsignedAbsDifference, Op};
use mkw_runtime::abi::Cpu;
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(full: bool) -> Vec<u32> {
    let mut words = Vec::new();
    for q in 0..2 {
        for size in 0..3 {
            for base in [0x2e207400, 0x2e207000] {
                for (d, n, m) in [
                    (0, 1, 2),
                    (0, 0, 2),
                    (0, 1, 0),
                    (0, 0, 0),
                    (31, 31, 31),
                    (31, 1, 31),
                ] {
                    if full || d == 0 {
                        words.push(base | (q << 30) | (size << 22) | (m << 16) | (n << 5) | d);
                    }
                }
            }
        }
    }
    words
}
fn decoded(w: u32) -> UnsignedAbsDifference {
    let Op::UnsignedAbsDifference(v) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    v
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("unsigned_abs_{w:x}")))
        .collect()
}
fn input(op: UnsignedAbsDifference, a: u64, b: u64, reverse: bool) -> [[u64; 2]; 32] {
    let mut v =
        std::array::from_fn(|n| [n as u64 + 0x123456789abcdef0, n as u64 + 0xfedcba9876543210]);
    for (r, value) in [(op.lhs, a), (op.rhs, b)] {
        let bytes = usize::from(op.bytes);
        let mask = (1u64 << (bytes * 8)) - 1;
        let mut raw = [0u8; 16];
        for (lane, chunk) in raw.chunks_exact_mut(bytes).enumerate() {
            let x = if reverse && lane % 2 != 0 {
                value ^ mask
            } else {
                value
            };
            chunk.copy_from_slice(&x.to_le_bytes()[..bytes]);
        }
        v[usize::from(r)] = [
            u64::from_le_bytes(raw[..8].try_into().unwrap()),
            u64::from_le_bytes(raw[8..].try_into().unwrap()),
        ];
    }
    v
}
fn expected(op: UnsignedAbsDifference, v: &[[u64; 2]; 32]) -> [u64; 2] {
    let raw = |r: u8| {
        v[usize::from(r)]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>()
    };
    let a = raw(op.lhs);
    let b = raw(op.rhs);
    let width = usize::from(op.bytes);
    let start = if op.upper { 8 } else { 0 };
    let end = if op.long {
        start + 8
    } else {
        usize::from(op.total)
    };
    let mut result = Vec::new();
    for (x, y) in a[start..end]
        .chunks_exact(width)
        .zip(b[start..end].chunks_exact(width))
    {
        let read = |v: &[u8]| {
            v.iter()
                .enumerate()
                .map(|(n, &b)| u64::from(b) << (n * 8))
                .sum::<u64>()
        };
        let diff = read(x).abs_diff(read(y));
        result.extend_from_slice(&diff.to_le_bytes()[..if op.long { width * 2 } else { width }]);
    }
    result.resize(16, 0);
    [
        u64::from_le_bytes(result[..8].try_into().unwrap()),
        u64::from_le_bytes(result[8..].try_into().unwrap()),
    ]
}
fn inputs(w: u32, full: bool) -> Vec<(u64, u64)> {
    let op = decoded(w);
    let bits = op.bytes * 8;
    let mask = (1u64 << bits) - 1;
    let values = [
        0,
        1,
        2,
        mask,
        mask - 1,
        mask / 2,
        mask / 2 + 1,
        mask / 2 - 1,
    ];
    if full && matches!(w, 0x2e227420 | 0x2e227020) {
        return (0..256)
            .flat_map(|a| (0..256).map(move |b| (a, b)))
            .collect();
    }
    values
        .iter()
        .flat_map(|&a| values.iter().map(move |&b| (a, b)))
        .collect()
}
#[test]
fn unsigned_absolute_matches_independent_byte_oracle() {
    let path = folder("unsigned-absolute-known");
    let words = words(true);
    let source = code(&words);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let mut observed = 0;
        for &w in &words {
            let op = decoded(w);
            let f = unsafe {
                library
                    .get::<Entry>(format!("unsigned_abs_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b) in inputs(w, true) {
                for reverse in [false, true] {
                    let before = input(op, a, b, reverse);
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
                    assert_eq!(cpu.vregs, want, "{opt} {w:x} {a:x} {b:x}");
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
        }
        if count == 0 {
            count = observed;
        } else {
            assert_eq!(count, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn unsigned_absolute_matches_synthetic_qemu() {
    let path = folder("unsigned-absolute-reference");
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
        for (a, b) in inputs(w, false) {
            for reverse in [false, true] {
                cases.push(reference::VectorCase {
                    word: w,
                    vectors: input(decoded(w), a, b, reverse)[..4].try_into().unwrap(),
                    fpcr: 0x02c00000,
                    fpsr: 0x08000091,
                    nzcv: 0xb0000000,
                });
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
        let library = build(&source, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let f = unsafe {
                library
                    .get::<Entry>(format!("unsigned_abs_{:x}\0", c.word).as_bytes())
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
