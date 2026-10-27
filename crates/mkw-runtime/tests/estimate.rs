use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(full: bool) -> Vec<u32> {
    let mut words = BTreeSet::new();
    for q in 0..1 {
        for step in [false] {
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
                let w = if step {
                    0x0e20fc00 | (m << 16)
                } else {
                    0x0ea1d800
                };
                words.insert(w | (q << 30) | (n << 5) | d);
            }
        }
    }
    words.into_iter().collect()
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("reciprocal_{w:x}")))
        .collect()
}
fn frame(w: u32, a: u32, b: u32) -> [[u64; 2]; 32] {
    let mut f = std::array::from_fn(|n| [0x1234567800000000 | n as u64, 0xfedcba9876543210]);
    let n = ((w >> 5) & 31) as usize;
    let m = ((w >> 16) & 31) as usize;
    let broadcast = |v: u32| [u64::from(v) | (u64::from(v) << 32); 2];
    if w & 0xbffffc00 != 0x0ea1d800 {
        f[m] = broadcast(b);
    }
    f[n] = broadcast(a);
    f
}
fn estimate_expected(a: u32, fpcr: u32) -> (u32, u32) {
    let sign = a & 0x80000000;
    let abs = a & 0x7fffffff;
    if abs > 0x7f800000 {
        return (
            if fpcr & (1 << 25) != 0 {
                0x7fc00000
            } else {
                a | 0x400000
            },
            u32::from(a & 0x400000 == 0),
        );
    }
    if abs == 0x7f800000 {
        return (sign, 0);
    }
    if abs == 0 {
        return (sign | 0x7f800000, 2);
    }
    let mut scaled = f64::from(f32::from_bits(abs));
    if scaled < 2f64.powi(-128) {
        let mode = (fpcr >> 22) & 3;
        let infinite = mode == 0 || (mode == 1 && sign == 0) || (mode == 2 && sign != 0);
        return (sign | if infinite { 0x7f800000 } else { 0x7f7fffff }, 20);
    }
    let mut power = 0;
    while scaled < 0.5 {
        scaled *= 2.0;
        power += 1;
    }
    while scaled >= 1.0 {
        scaled *= 0.5;
        power -= 1;
    }
    let midpoint = ((scaled * 512.0).floor() + 0.5) / 512.0;
    // Independent real-valued midpoint quantization; no bit normalization/division formula.
    let approximation = (256.0 / midpoint + 0.5).floor() / 256.0;
    (
        sign | ((approximation * 2f64.powi(power)) as f32).to_bits(),
        0,
    )
}
#[test]
fn estimate_estimates_match_independent_midpoint_quantization() {
    let path = folder("estimate-known");
    let words = words(true);
    let source = code(&words);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        let mut observed = 0;
        for &w in &words {
            if w & 0xbffffc00 != 0x0ea1d800 {
                continue;
            }
            let native = unsafe {
                library
                    .get::<Entry>(format!("reciprocal_{w:x}\0").as_bytes())
                    .unwrap()
            };
            // Every mantissa bucket at lower/upper edges across every normal exponent.
            let mut values = BTreeSet::new();
            for exponent in 1..255u32 {
                for bucket in 0..256 {
                    values.insert((exponent << 23) | (bucket << 15));
                    values.insert((exponent << 23) | (bucket << 15) | 32767);
                }
            }
            values.extend([
                0, 1, 0x1fffff, 0x200000, 0x200001, 0x3fffff, 0x400000, 0x7fffff, 0x7f800000,
                0x7fc12345, 0x7f812345,
            ]);
            for a in values {
                for sign in [0, 1 << 31] {
                    let a = a | sign;
                    for fpcr in if a & 0x7f800000 == 0 || a & 0x7f800000 == 0x7f800000 {
                        vec![0, 1 << 22, 2 << 22, 3 << 22, 1 << 25, (1 << 25) | (3 << 22)]
                    } else {
                        vec![0]
                    } {
                        let before = frame(w, a, 0);
                        let mut cpu = Cpu {
                            sp: 99,
                            n: 1,
                            c: 1,
                            v: 1,
                            fpcr,
                            fpsr: 0x08000080,
                            vregs: before,
                            ..Cpu::default()
                        };
                        cpu.x = std::array::from_fn(|r| r as u64 + 123);
                        let x = cpu.x;
                        let (value, flags) = estimate_expected(a, fpcr);
                        let mut expected = before;
                        expected[(w & 31) as usize] = [
                            u64::from(value) | (u64::from(value) << 32),
                            if w & (1 << 30) != 0 {
                                u64::from(value) | (u64::from(value) << 32)
                            } else {
                                0
                            },
                        ];
                        {
                            let mut attached = backend.attach(&mut cpu);
                            assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
                        }
                        assert_eq!(cpu.vregs, expected, "{opt} {w:x} {a:x}/{fpcr:x}");
                        assert_eq!(cpu.fpsr, 0x08000080 | flags);
                        assert_eq!(cpu.x, x);
                        assert_eq!(
                            (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr),
                            (99, 1, 0, 1, 1, fpcr)
                        );
                        observed += 1;
                    }
                }
            }
        }
        if count == 0 {
            count = observed;
        } else {
            assert_eq!(observed, count);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn estimate_missing_backend_and_unsupported_fpcr_refuse_without_commit() {
    let path = folder("estimate-profile-known");
    let words = words(true);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for w in &words {
            let native = unsafe {
                library
                    .get::<Entry>(format!("reciprocal_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (fpcr, missing) in [
                (1 << 24, false),
                (1 << 26, false),
                (1 << 8, false),
                (1, false),
                (0, true),
            ] {
                let before = frame(*w, 0x3f800000, 0);
                let mut cpu = Cpu {
                    fpcr,
                    fpsr: 0x80,
                    vregs: before,
                    ..Cpu::default()
                };
                let rc = if missing {
                    unsafe { native(&mut cpu, 10) }
                } else {
                    let mut attachment = backend.attach(&mut cpu);
                    unsafe { native(attachment.cpu_mut(), 10) }
                };
                assert_eq!(rc, 3);
                assert_eq!(cpu.vregs, before);
                assert_eq!(cpu.fpsr, 0x80);
            }
        }
    }
}
#[test]
fn estimate_matches_synthetic_qemu() {
    let path = folder("estimate-reference");
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
    let values = [
        0, 1, 0x1fffff, 0x200000, 0x200001, 0x3fffff, 0x400000, 0x7fffff, 0x800000, 0x3f000000,
        0x3f7fffff, 0x3f800000, 0x3f800001, 0x3ffffffe, 0x3fffffff, 0x40000000, 0x7e800000,
        0x7effffff, 0x7f000000, 0x7f7fffff, 0x7f800000, 0x7fc12345, 0x7f812345, 0xffc54321,
        0xff854321, 0x80000000,
    ];
    let mut pairs = Vec::new();
    for a in values {
        for b in values {
            pairs.push((a, b));
            pairs.push((a ^ (1 << 31), b));
        }
    }
    let mut state = 0x12345678u32;
    for _ in 0..256 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = state;
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        pairs.push((a, state));
    }
    let mut cases = Vec::new();
    for &w in &words {
        for &(a, b) in &pairs {
            for rounding in 0..4 {
                for dn in 0..2 {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame(w, a, b)[..4].try_into().unwrap(),
                        fpcr: (rounding << 22) | (dn << 25),
                        fpsr: 0x80 | ((a & 1) << 27),
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
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("reciprocal_{:x}\0", c.word).as_bytes())
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
            {
                let mut attachment = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(attachment.cpu_mut(), 10) }, 0);
            }
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
                "{opt} {i} {:x} {:x?}/{:x}",
                c.word,
                c.vectors,
                c.fpcr
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
