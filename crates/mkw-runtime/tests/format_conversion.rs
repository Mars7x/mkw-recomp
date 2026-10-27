use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::process::Command;
#[path = "support/fp_native.rs"]
mod fp_native;
use fp_native::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn word(bytes: u32, overlap: bool) -> u32 {
    (if bytes == 4 { 0x1e22c000 } else { 0x1e624000 }) | if overlap { 0 } else { 1 << 5 }
}
fn codes() -> String {
    let mut code = String::new();
    for bytes in [4, 8] {
        for overlap in [false, true] {
            let w = word(bytes, overlap);
            code += &generate(&[w, 0xd65f03c0], &format!("format_{w:x}"));
        }
    }
    code
}
#[test]
fn format_known_patterns_rounding_nan_payloads_flags_and_refusals() {
    let path = folder("format-known");
    let code = codes();
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (bytes, input, rounding, expected, flags) in [
            (4, 0x3f800000u64, 0, 0x3ff0000000000000u64, 0),
            (4, 0x80000000, 0, 0x8000000000000000, 0),
            (4, 1, 0, 0x36a0000000000000, 0),
            (
                4,
                0x7f812345,
                0,
                0x7ff0000000000000 | (0x012345u64 << 29) | 0x0008000000000000,
                1,
            ),
            (8, 0x3ff0000000000000, 0, 0x3f800000, 0),
            (8, 0x3ff0000010000000, 0, 0x3f800000, 16),
            (8, 0x3ff0000010000000, 1, 0x3f800001, 16),
            (8, 0x7fefffffffffffff, 0, 0x7f800000, 20),
            (8, 0x7fefffffffffffff, 3, 0x7f7fffff, 20),
            (8, 1, 0, 0, 24),
            (8, 1, 1, 1, 24),
            (
                8,
                0xfff0123456789abc,
                0,
                0xff800000 | (0x0123456789abcu64 >> 29) | 0x00400000,
                1,
            ),
        ] {
            for overlap in [false, true] {
                let w = word(bytes, overlap);
                let native = unsafe {
                    library
                        .get::<Entry>(format!("format_{w:x}\0").as_bytes())
                        .unwrap()
                };
                let mut cpu = Cpu {
                    fpcr: rounding << 22,
                    fpsr: 0x08000080,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    ..Cpu::default()
                };
                cpu.vregs[0] = [123, 456];
                cpu.vregs[usize::from(!overlap)] = [input, u64::MAX];
                {
                    let mut a = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                }
                assert_eq!(
                    cpu.vregs[0],
                    [expected, 0],
                    "{bytes} {input:x} rounding {rounding}"
                );
                assert_eq!(cpu.fpsr, 0x08000080 | flags);
                assert_eq!((cpu.n, cpu.z, cpu.c, cpu.v), (1, 0, 1, 1));
            }
        }
        let native = unsafe { library.get::<Entry>(b"format_1e22c020\0").unwrap() };
        for control in [0, 1 << 24, 1 << 8, 1 << 19, 1 << 26, 1] {
            let mut cpu = Cpu {
                fpcr: control,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.vregs[0] = [123, 456];
            cpu.vregs[1][0] = 0x3f800000;
            if control == 0 {
                assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
            } else {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 3);
            }
            assert_eq!(cpu.vregs[0], [123, 456]);
            assert_eq!(cpu.fpsr, 0x08000080);
        }
    }
}
#[test]
fn format_results_and_flags_match_qemu() {
    let path = folder("format-reference");
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
    let mut cases = Vec::new();
    for bytes in [4, 8] {
        let mut inputs = if bytes == 4 {
            vec![
                0, 0x80000000, 1, 0x007fffff, 0x00800000, 0x3f800000, 0x3f800001, 0x7f7fffff,
                0x7f800000, 0xff800000, 0x7fc12345, 0x7f812345, 0xffc12345, 0xff812345,
            ]
        } else {
            vec![
                0,
                0x8000000000000000,
                1,
                0x000fffffffffffff,
                0x0010000000000000,
                0x3ff0000000000000,
                0x3ff0000010000000,
                0x3ff0000010000001,
                0x47efffffe0000000,
                0x47effffff0000000,
                0x47efffffe0000001,
                0x3810000000000000,
                0x380fffffe0000000,
                0x36a0000000000000,
                0x369fffffffffffff,
                0x7fefffffffffffff,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8123456789abc,
                0x7ff0123456789abc,
                0xfff8123456789abc,
                0xfff0123456789abc,
            ]
        };
        let mut random = 0x9168eabf03a04c77u64;
        for _ in 0..256 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            inputs.push(if bytes == 4 {
                random & 0xffffffff
            } else {
                random
            });
        }
        for input in inputs {
            for rounding in 0..4 {
                for dn in 0..2 {
                    for overlap in [false, true] {
                        let mut vectors = reference::INITIAL_V;
                        vectors[usize::from(!overlap)] = [input, u64::MAX];
                        cases.push(reference::VectorCase {
                            word: word(bytes, overlap),
                            vectors,
                            fpcr: (rounding << 22) | (dn << 25),
                            fpsr: 0x08000080,
                            nzcv: 0xb0000000,
                        });
                    }
                }
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (index, chunk) in cases.chunks(2000).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{index}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .arg(&image)
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
    let code = codes();
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("format_{:x}\0", c.word).as_bytes())
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
            {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
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
                "{opt} fixture {i} word {:x} fpcr {:x}",
                c.word,
                c.fpcr
            );
            assert_eq!(cpu.vregs[1..4], c.vectors[1..4]);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
