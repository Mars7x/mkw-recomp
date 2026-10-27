use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::process::Command;
#[path = "support/fp_native.rs"]
mod fp_native;
use fp_native::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn word(bytes: u32, bits: u32, operation: u32) -> u32 {
    let fields = if operation < 2 {
        (operation + 2) << 16
    } else if operation >= 10 {
        (operation - 6) << 16
    } else {
        ((operation & 1) << 16) | (((operation - 2) / 2) << 19)
    };
    0x1e200000
        | (if bits == 64 { 1 << 31 } else { 0 })
        | (if bytes == 8 { 1 << 22 } else { 0 })
        | fields
        | (1 << 5)
}
fn conversion_codes() -> String {
    let mut code = String::new();
    for bytes in [4, 8] {
        for bits in [32, 64] {
            for operation in 0..12 {
                let w = word(bytes, bits, operation);
                code += &generate(&[w, 0xd65f03c0], &format!("convert_{w:x}"));
            }
        }
    }
    code
}
#[test]
fn known_conversions_flags_saturation_zero_register_and_refusals() {
    let path = folder("conversion-known");
    let mut code = conversion_codes();
    code += &generate(&[0x9e6203ff, 0xd65f03c0], "zero_input");
    code += &generate(&[0x1e38003f, 0xd65f03c0], "zero_output");
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (bytes, bits, op, input, expected, flags) in [
            (4, 32, 0, 0xffffffffu64, 0xbf800000u64, 0),
            (8, 64, 0, u64::MAX, 0xbff0000000000000, 0),
            (4, 64, 1, u64::MAX, 0x5f800000, 16),
            (4, 32, 8, 0x3fc00000, 1, 16),
            (4, 32, 8, 0xbfc00000, 0xffffffff, 16),
            (4, 64, 8, 0xbfc00000, u64::MAX, 16),
            (4, 32, 8, 0x7fc12345, 0, 1),
            (8, 64, 8, 0x7ff0000000000000, 0x7fffffffffffffff, 1),
            (8, 64, 8, 0xfff0000000000000, 0x8000000000000000, 1),
            (4, 32, 9, 0xbf800000, 0, 1),
            (4, 32, 10, 0x40200000, 3, 16),
        ] {
            let w = word(bytes, bits, op);
            let native = unsafe {
                library
                    .get::<Entry>(format!("convert_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.sp = 99;
            cpu.x[0] = 0xdeadbeefdeadbeef;
            cpu.x[1] = input;
            cpu.vregs[1][0] = input;
            cpu.vregs[0] = [123, 456];
            {
                let mut attached = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
            }
            assert_eq!(
                if op < 2 { cpu.vregs[0][0] } else { cpu.x[0] },
                expected,
                "{w:08x}"
            );
            assert_eq!(cpu.fpsr, 0x08000080 | flags);
            assert_eq!((cpu.n, cpu.z, cpu.c, cpu.v, cpu.sp), (1, 0, 1, 1, 99));
            assert!(cpu.convert.is_none());
        }
        let zero = unsafe { library.get::<Entry>(b"zero_input\0").unwrap() };
        let mut cpu = Cpu {
            sp: 99,
            ..Cpu::default()
        };
        cpu.vregs[31] = [123, 456];
        {
            let mut attached = backend.attach(&mut cpu);
            assert_eq!(unsafe { zero(attached.cpu_mut(), 10) }, 0);
        }
        assert_eq!(cpu.vregs[31], [0, 0]);
        assert_eq!(cpu.sp, 99);
        let zero = unsafe { library.get::<Entry>(b"zero_output\0").unwrap() };
        cpu.vregs[1][0] = 0x3fc00000;
        cpu.x[0] = 777;
        {
            let mut attached = backend.attach(&mut cpu);
            assert_eq!(unsafe { zero(attached.cpu_mut(), 10) }, 0);
        }
        assert_eq!(cpu.x[0], 777);
        assert_eq!(cpu.fpsr, 16);
        let native = unsafe { library.get::<Entry>(b"convert_1e220020\0").unwrap() };
        for control in [0, 1 << 24, 1 << 8, 1 << 19, 1 << 26, 1] {
            let mut cpu = Cpu {
                fpcr: control,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.vregs[0] = [123, 456];
            cpu.x[1] = 1;
            if control == 0 {
                assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
            } else {
                let mut attached = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 3);
            }
            assert_eq!(cpu.vregs[0], [123, 456]);
            assert_eq!(cpu.fpsr, 0x08000080);
        }
    }
}
#[test]
fn conversion_results_and_flags_match_qemu() {
    let path = folder("conversion-reference");
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
    let integer_inputs = [
        0u64,
        1,
        2,
        0x7fffffff,
        0x80000000,
        0xffffffff,
        0x100000001,
        0x1fffffffffffff,
        0x20000000000001,
        0x7fffffffffffffff,
        0x8000000000000000,
        u64::MAX,
        0xfedcba9876543210,
    ];
    let mut cases = Vec::new();
    for bytes in [4, 8] {
        let floats = if bytes == 4 {
            vec![
                0, 0x80000000, 1, 0x807fffff, 0x3f000000, 0xbf000000, 0x3fc00000, 0xbfc00000,
                0x40200000, 0xc0200000, 0x4effffff, 0x4f000000, 0xcf000000, 0x4f800000, 0x5f000000,
                0x5f800000, 0x7f800000, 0xff800000, 0x7fc12345, 0x7f812345,
            ]
        } else {
            vec![
                0,
                0x8000000000000000,
                1,
                0x800fffffffffffff,
                0x3fe0000000000000,
                0xbfe0000000000000,
                0x3ff8000000000000,
                0xbff8000000000000,
                0x4004000000000000,
                0xc004000000000000,
                0x41dfffffffffffff,
                0x41e0000000000000,
                0xc1e0000000000000,
                0x41f0000000000000,
                0x43e0000000000000,
                0x43f0000000000000,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8123456789abc,
                0x7ff0123456789abc,
            ]
        };
        for bits in [32, 64] {
            for operation in 0..12 {
                for rounding in 0..4 {
                    for dn in 0..2 {
                        let inputs = if operation < 2 {
                            integer_inputs.as_slice()
                        } else {
                            floats.as_slice()
                        };
                        for &input in inputs {
                            cases.push(reference::FloatingCase {
                                word: word(bytes, bits, operation),
                                left: input,
                                right: 0,
                                accumulate: 0,
                                nzcv: 0xb0000000,
                                fpcr: (rounding << 22) | (dn << 25),
                                fpsr: 0x08000080,
                            });
                        }
                    }
                }
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (index, chunk) in cases.chunks(2000).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{index}"));
        std::fs::write(&image, reference::conversion_elf(chunk)).unwrap();
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
    assert_eq!(oracle.len(), cases.len() * 48);
    let code = conversion_codes();
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, case) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("convert_{:x}\0", case.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                fpcr: case.fpcr,
                fpsr: case.fpsr,
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.x[0] = reference::INITIAL_X[0];
            cpu.x[1] = case.left;
            cpu.vregs[0] = reference::INITIAL_V[0];
            cpu.vregs[1] = [case.left, reference::INITIAL_V[1][1]];
            {
                let mut attached = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
            }
            let mut actual = Vec::new();
            for value in [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                u64::from(cpu.fpsr),
                (u64::from(cpu.n) << 31)
                    | (u64::from(cpu.z) << 30)
                    | (u64::from(cpu.c) << 29)
                    | (u64::from(cpu.v) << 28),
                cpu.x[0],
                u64::from(cpu.fpcr),
            ] {
                actual.extend(value.to_le_bytes());
            }
            assert_eq!(
                actual,
                &oracle[i * 48..(i + 1) * 48],
                "{opt} fixture {i}, word {:08x}, input {:016x}, fpcr {:08x}",
                case.word,
                case.left,
                case.fpcr
            );
        }
    }
    let version = String::from_utf8(
        Command::new("qemu-aarch64")
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let receipt = serde_json::json!({"reference":version.lines().next(),"synthetic_cases":cases.len(),"integer_widths":[32,64],"floating_widths":[32,64],"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false});
    std::fs::write(
        path.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
