use mkw_runtime::{
    abi::Cpu,
    bridge::{MemoryBridge, MemoryOrder},
    floating::FloatingBackend,
    GuestAddr, Memory, Permissions,
};
use std::process::Command;
#[path = "support/fp_native.rs"]
mod fp_native;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
use fp_native::*;
fn fp_word(bytes: u8, operation: u32, zero: bool) -> u32 {
    if (7..=9).contains(&operation) {
        return (match operation {
            7 => 0x1e214000,
            8 => 0x1e20c000,
            9 => 0x1e21c000,
            _ => unreachable!(),
        }) | (if bytes == 8 { 1 << 22 } else { 0 })
            | (1 << 5);
    }
    let base = match operation {
        0 => 0x1e202800,
        1 => 0x1e203800,
        2 => 0x1e200800,
        3 => 0x1e201800,
        4 => 0x1e202000,
        5 => 0x1e202010,
        6 => 0x1f000c00,
        10 => 0x1e208800,
        11 => 0x1f008c00,
        12 => 0x1f200c00,
        13 => 0x1f208c00,
        15 => 0x1e204800,
        16 => 0x1e205800,
        17 => 0x1e206800,
        18 => 0x1e207800,
        _ => unreachable!(),
    };
    base | (if bytes == 8 { 1 << 22 } else { 0 }) | (if zero { 8 } else { 2 << 16 }) | (1 << 5)
}
#[test]
fn fp_state_known_results_flags_refusals_and_thread_isolation() {
    let path = folder("fp-known");
    let code = generate(&[fp_word(4, 0, false), 0xd65f03c0], "add")
        + &generate(
            &[
                0xd51b4400, 0xd51b4421, 0xd53b4402, 0xd53b4423, 0xd51b4204, 0xd53b4205, 0xd65f03c0,
            ],
            "state",
        )
        + &generate(&[fp_word(4, 6, false), 0xd65f03c0], "fused")
        + &generate(&[0x1f027c20, 0xd65f03c0], "fused31")
        + &generate(
            &[0xd51b441f, 0xd51b443f, 0xd53b421f, 0xd65f03c0],
            "zero_state",
        );
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let fp = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        let add = unsafe { *library.get::<Entry>(b"add\0").unwrap() };
        let state = unsafe { *library.get::<Entry>(b"state\0").unwrap() };
        for name in [b"fused\0".as_slice(), b"fused31\0"] {
            let entry = unsafe { *library.get::<Entry>(name).unwrap() };
            let mut cpu = Cpu::default();
            cpu.vregs[1][0] = 0x3f800001;
            cpu.vregs[2][0] = 0x3f7ffffe;
            cpu.vregs[3][0] = 0xbf800000;
            cpu.vregs[31][0] = 0xbf800000;
            cpu.vregs[0][1] = 123;
            let mut attached = fp.attach(&mut cpu);
            assert_eq!(unsafe { entry(attached.cpu_mut(), 10) }, 0);
            assert_eq!(attached.cpu_mut().vregs[0], [0xa8800000, 0]);
            assert_eq!(attached.cpu_mut().fpsr, 0);
        }
        let zero = unsafe { *library.get::<Entry>(b"zero_state\0").unwrap() };
        let mut zero_cpu = Cpu {
            sp: 99,
            fpcr: 0x02400000,
            fpsr: 0x0800009f,
            n: 1,
            z: 0,
            c: 1,
            v: 1,
            ..Cpu::default()
        };
        assert_eq!(unsafe { zero(&mut zero_cpu, 10) }, 0);
        assert_eq!(
            (
                zero_cpu.fpcr,
                zero_cpu.fpsr,
                zero_cpu.sp,
                zero_cpu.n,
                zero_cpu.z,
                zero_cpu.c,
                zero_cpu.v
            ),
            (0, 0, 99, 1, 0, 1, 1)
        );
        let mut cpu = Cpu::default();
        cpu.x[0] = 0x02400000;
        cpu.x[1] = u64::MAX;
        cpu.x[4] = 0xa1234567;
        cpu.x[30] = 0xdead;
        assert_eq!(unsafe { state(&mut cpu, 30) }, 0);
        assert_eq!(
            (cpu.fpcr, cpu.fpsr, cpu.x[2], cpu.x[3], cpu.x[5]),
            (0x02400000, 0x0800009f, 0x02400000, 0x0800009f, 0xa0000000)
        );
        cpu.x[0] = 1;
        assert_eq!(unsafe { state(&mut cpu, 30) }, 3);
        assert_eq!(cpu.fpcr, 0x02400000);
        for control in [0u32, 1 << 24, 1 << 8, 1 << 19, 1 << 26, 1] {
            let mut cpu = Cpu {
                fpcr: control,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.vregs[0] = [123, 456];
            cpu.vregs[1][0] = 0xfeedbeef3f800000;
            cpu.vregs[2][0] = 0x33800000;
            if control == 0 {
                let mut attached = fp.attach(&mut cpu);
                assert_eq!(unsafe { add(attached.cpu_mut(), 10) }, 0);
                assert_eq!(attached.cpu_mut().vregs[0], [0x3f800000, 0]);
                assert_eq!(attached.cpu_mut().fpsr, 0x08000090);
            } else {
                let mut attached = fp.attach(&mut cpu);
                assert_eq!(unsafe { add(attached.cpu_mut(), 10) }, 3);
                assert_eq!(attached.cpu_mut().vregs[0], [123, 456]);
                assert_eq!(attached.cpu_mut().fpsr, 0x08000080);
            }
            assert!(cpu.floating.is_none());
        }
        let mut missing = Cpu::default();
        missing.vregs[0] = [123, 456];
        assert_eq!(unsafe { add(&mut missing, 10) }, 3);
        assert_eq!(missing.vregs[0], [123, 456]);
        std::thread::scope(|scope| {
            for rounding in 0..4 {
                let fp = &fp;
                scope.spawn(move || {
                    let mut cpu = Cpu {
                        fpcr: rounding << 22,
                        ..Cpu::default()
                    };
                    cpu.vregs[1][0] = 0xfeedbeef3f800000;
                    cpu.vregs[2][0] = 0x33800000;
                    let mut attached = fp.attach(&mut cpu);
                    for _ in 0..1000 {
                        assert_eq!(unsafe { add(attached.cpu_mut(), 10) }, 0);
                        assert_eq!(
                            attached.cpu_mut().vregs[0][0],
                            if rounding == 1 {
                                0x3f800001
                            } else {
                                0x3f800000
                            }
                        );
                        assert_eq!(attached.cpu_mut().fpsr, 16);
                    }
                });
            }
        });
    }
}
#[test]
fn scalar_fp_matches_qemu_rounding_nan_subnormal_and_exception_fixtures() {
    let path = folder("fp-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\",\"hardware_reference\":false}\n",
        )
        .unwrap();
        return;
    }
    let mut cases = Vec::new();
    for bytes in [4u8, 8] {
        let values = if bytes == 4 {
            vec![
                0, 0x80000000, 1, 0x007fffff, 0x00800000, 0x00800001, 0x3f000000, 0x3f800000,
                0x3f800001, 0x33800000, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc12345, 0x7f812345,
                0xffc54321,
            ]
        } else {
            vec![
                0,
                0x8000000000000000,
                1,
                0x000fffffffffffff,
                0x0010000000000000,
                0x0010000000000001,
                0x3fe0000000000000,
                0x3ff0000000000000,
                0x3ff0000000000001,
                0x3ca0000000000000,
                0x7fefffffffffffff,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8123456789abc,
                0x7ff0123456789abc,
                0xfff8123456789abc,
            ]
        };
        let mut pairs = Vec::new();
        for i in 0..values.len() {
            pairs.push((values[i], values[(i + 3) % values.len()]));
            pairs.push((values[i], values[i]));
            pairs.push((values[i], values[(i + 7) % values.len()]));
        }
        for &left in &values[13..] {
            for &right in &values[13..] {
                pairs.push((left, right));
            }
        }
        let sign = if bytes == 4 { 1u64 << 31 } else { 1u64 << 63 };
        pairs.extend([
            (0, sign),
            (sign, 0),
            (sign, sign),
            (values[11], 0),
            (0, values[11]),
            (values[7], values[9]),
            (values[7] | sign, values[9] | sign),
            (
                values[4],
                if bytes == 4 {
                    0x3f7fffff
                } else {
                    0x3fefffffffffffff
                },
            ),
        ]);
        let mut random = 0x419fc9a5d37b0281u64;
        for _ in 0..16 {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let left = random;
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let right = random;
            pairs.push(if bytes == 4 {
                (left & 0xffffffff, right & 0xffffffff)
            } else {
                (left, right)
            });
        }
        for rounding in 0..4 {
            for dn in 0..2 {
                for operation in (0..14).chain(15..19) {
                    for (i, &(left, right)) in pairs.iter().enumerate() {
                        cases.push(reference::FloatingCase {
                            word: fp_word(bytes, operation, false),
                            left,
                            right,
                            accumulate: if i % 3 == 0 {
                                left ^ (if bytes == 4 { 1 << 31 } else { 1 << 63 })
                            } else {
                                right
                            },
                            nzcv: 0xb0000000,
                            fpcr: (rounding << 22) | (dn << 25),
                            fpsr: if i % 2 == 0 { 0x08000080 } else { 0 },
                        });
                    }
                }
            }
        }
        let one = values[7];
        let qnan = values[13];
        let snan = values[14];
        let inf = values[11];
        for (left, right, accumulate) in [
            (inf, 0, qnan),
            (0, inf, qnan),
            (inf, 0, snan),
            (snan, one, qnan),
            (qnan, one, snan),
            (qnan, snan, qnan),
            (qnan, snan, snan),
            (one, one, qnan),
            (one, one, snan),
        ] {
            for operation in [6, 11, 12, 13] {
                for dn in 0..2 {
                    cases.push(reference::FloatingCase {
                        word: fp_word(bytes, operation, false),
                        left,
                        right,
                        accumulate,
                        nzcv: 0xb0000000,
                        fpcr: dn << 25,
                        fpsr: 0,
                    });
                }
            }
        }
        for operation in 4..6 {
            for &left in &values {
                cases.push(reference::FloatingCase {
                    word: fp_word(bytes, operation, true),
                    left,
                    right: 0x7ff0123456789abc,
                    accumulate: 0,
                    nzcv: 0xb0000000,
                    fpcr: 0,
                    fpsr: 0,
                });
            }
        }
    }
    for ftype in [0, 1, 3] {
        for condition in 0..16 {
            for flags in 0..16 {
                cases.push(reference::FloatingCase {
                    word: 0x1e220c20 | (ftype << 22) | (condition << 12),
                    left: 0xfff0123456789abc,
                    right: 0x7ff8fedcba987654,
                    accumulate: 0,
                    nzcv: flags << 28,
                    fpcr: 0,
                    fpsr: 0x0800009f,
                });
            }
        }
    }
    for bytes in [4, 8] {
        for signaling in 0..2 {
            for condition in 0..16 {
                for flags in 0..16 {
                    for left in [
                        if bytes == 4 {
                            0x3f800000
                        } else {
                            0x3ff0000000000000
                        },
                        if bytes == 4 {
                            0x7f812345
                        } else {
                            0x7ff0123456789abc
                        },
                    ] {
                        cases.push(reference::FloatingCase {
                            word: 0x1e220420
                                | (if bytes == 8 { 1 << 22 } else { 0 })
                                | (signaling << 4)
                                | (condition << 12)
                                | condition,
                            left,
                            right: 0,
                            accumulate: 0,
                            nzcv: flags << 28,
                            fpcr: 0,
                            fpsr: 0x08000080,
                        });
                    }
                }
            }
            for nzcv in 0..16 {
                cases.push(reference::FloatingCase {
                    word: 0x1e220420
                        | (if bytes == 8 { 1 << 22 } else { 0 })
                        | (signaling << 4)
                        | nzcv,
                    left: 0x7ff0123456789abc,
                    right: 0,
                    accumulate: 0,
                    nzcv: 0,
                    fpcr: 1 << 24,
                    fpsr: 0x08000080,
                });
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (index, chunk) in cases.chunks(2500).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{index}"));
        std::fs::write(&image, reference::floating_elf(chunk)).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        let qemu = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .current_dir(&path)
            .arg(&image)
            .output()
            .unwrap();
        assert!(
            qemu.status.success(),
            "{}",
            String::from_utf8_lossy(&qemu.stderr)
        );
        oracle.extend(qemu.stdout);
    }
    assert_eq!(oracle.len(), cases.len() * 32);
    let words: std::collections::BTreeSet<_> = cases.iter().map(|c| c.word).collect();
    let mut code = String::new();
    for word in &words {
        code += &generate(&[*word, 0xd65f03c0], &format!("fp_{word:x}"));
    }
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let fp = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, case) in cases.iter().enumerate() {
            let entry = unsafe {
                library
                    .get::<Entry>(format!("fp_{:x}\0", case.word).as_bytes())
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
            cpu.n = (case.nzcv >> 31) & 1;
            cpu.z = (case.nzcv >> 30) & 1;
            cpu.c = (case.nzcv >> 29) & 1;
            cpu.v = (case.nzcv >> 28) & 1;
            cpu.vregs[0] = reference::INITIAL_V[0];
            cpu.vregs[1] = [case.left, reference::INITIAL_V[1][1]];
            cpu.vregs[2] = [case.right, reference::INITIAL_V[2][1]];
            cpu.vregs[3] = [case.accumulate, reference::INITIAL_V[3][1]];
            {
                let mut attached = fp.attach(&mut cpu);
                assert_eq!(unsafe { entry(attached.cpu_mut(), 10) }, 0);
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
            ] {
                actual.extend(value.to_le_bytes());
            }
            assert_eq!(
                actual,
                &oracle[i * 32..(i + 1) * 32],
                "{opt} fixture {i} word {:08x} lhs {:016x} rhs {:016x} fpcr {:08x}",
                case.word,
                case.left,
                case.right,
                case.fpcr
            );
        }
    }
    let version = Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .unwrap();
    let version = String::from_utf8(version.stdout).unwrap();
    let receipt = serde_json::json!({"reference":version.lines().next(),"synthetic_cases":cases.len(),"operations":["fadd","fsub","fmul","fdiv","fcmp","fcmpe","fmadd","fcsel","fneg","fabs","fsqrt","fnmul","fmsub","fnmadd","fnmsub","fmax","fmin","fmaxnm","fminnm","fccmp","fccmpe"],"widths":[32,64],"raw_select_widths":[16,32,64],"rounding_modes":4,"default_nan_modes":2,"flush_modes_supported":false,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false});
    std::fs::write(
        path.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}

fn memory() -> std::pin::Pin<Box<MemoryBridge>> {
    let mut memory = Memory::default();
    memory
        .map(GuestAddr(0x1000), vec![0; 32], Permissions::RW, "synthetic")
        .unwrap();
    memory.seal();
    MemoryBridge::new(memory)
}
#[test]
fn data_barriers_preserve_tags_refuse_missing_backends_and_publish() {
    let path = folder("data-barriers");
    let mut code = String::new();
    let mut words = Vec::new();
    for kind in 0..2 {
        for option in 0..16 {
            if option & 3 != 0 {
                let word = if kind == 0 { 0xd50330bf } else { 0xd503309f } | (option << 8);
                words.push((kind, option, word));
                code += &generate(&[word, 0xd65f03c0], &format!("barrier_{kind}_{option}"));
            }
        }
    }
    // Writer: data=42; DMB ISHST; ready=1. Reader: poll; DMB ISHLD; data.
    code += &generate(
        &[
            0xd2800541, 0xf9000001, 0xd5033abf, 0xd2800022, 0xf9000402, 0xd65f03c0,
        ],
        "writer",
    );
    code += &generate(
        &[0xf9400402, 0xb4ffffe2, 0xd50339bf, 0xf9400001, 0xd65f03c0],
        "reader",
    );
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let bridge = memory();
        for &(kind, option, _) in &words {
            let native = unsafe {
                library
                    .get::<Entry>(format!("barrier_{kind}_{option}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.x[0] = 123;
            cpu.x[30] = 0xdead;
            assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
            assert_eq!(
                (attached.cpu_mut().x[0], attached.cpu_mut().pc),
                (123, 0xdead)
            );
            assert_eq!(
                (
                    attached.cpu_mut().n,
                    attached.cpu_mut().z,
                    attached.cpu_mut().c,
                    attached.cpu_mut().v
                ),
                (1, 0, 1, 1)
            );
            assert_eq!(bridge.barrier_counts()[kind as usize][option as usize], 1);
        }
        assert!(bridge.barrier_access(0, 0).is_err());
        assert!(bridge.barrier_access(2, 15).is_err());
        assert!(bridge.barrier_access(0, 16).is_err());
        let writer = unsafe { *library.get::<Entry>(b"writer\0").unwrap() };
        let reader = unsafe { *library.get::<Entry>(b"reader\0").unwrap() };
        let start = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let start = &start;
            let bridge = &bridge;
            scope.spawn(move || {
                let mut cpu = Cpu::default();
                cpu.x[0] = 0x1000;
                let mut attached = bridge.as_ref().attach(&mut cpu);
                start.wait();
                assert_eq!(unsafe { writer(attached.cpu_mut(), 100) }, 0);
            });
            let mut cpu = Cpu::default();
            cpu.x[0] = 0x1000;
            let mut attached = bridge.as_ref().attach(&mut cpu);
            start.wait();
            let mut done = false;
            for _ in 0..1000 {
                let status = unsafe { reader(attached.cpu_mut(), 10000) };
                if status == 0 {
                    done = true;
                    break;
                }
                assert_eq!(status, 2);
            }
            assert!(done, "writer did not publish within diagnostic budget");
            assert_eq!(attached.cpu_mut().x[1], 42);
        });
        let mut data = [0];
        bridge
            .access(MemoryOrder::Plain, false, GuestAddr(0x1000), &mut data, 8)
            .unwrap();
        assert_eq!(data[0], 42);
    }
}

#[test]
fn conditional_compare_skips_false_path_and_fnmul_preserves_nan_sign_rules() {
    let path = folder("conditional-fp-known");
    let mut code = String::new();
    for bytes in [4, 8] {
        for signaling in 0..2 {
            for cond in 0..16 {
                let word = 0x1e220420
                    | (if bytes == 8 { 1 << 22 } else { 0 })
                    | (signaling << 4)
                    | (cond << 12)
                    | cond;
                code += &generate(
                    &[word, 0xd65f03c0],
                    &format!("cc_{bytes}_{signaling}_{cond}"),
                );
            }
        }
        code += &generate(
            &[fp_word(bytes, 10, false), 0xd65f03c0],
            &format!("nm_{bytes}"),
        );
    }
    let masks = [
        0xf0f0u16, 0x0f0f, 0xcccc, 0x3333, 0xff00, 0x00ff, 0xaaaa, 0x5555, 0x0c0c, 0xf3f3, 0xaa55,
        0x55aa, 0x0a05, 0xf5fa, 0xffff, 0xffff,
    ];
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let fp = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for bytes in [4, 8] {
            for signaling in 0..2 {
                for (cond, mask) in masks.iter().enumerate() {
                    let native = unsafe {
                        library
                            .get::<Entry>(format!("cc_{bytes}_{signaling}_{cond}\0").as_bytes())
                            .unwrap()
                    };
                    for flags in 0..16u32 {
                        let holds = *mask & (1 << flags) != 0;
                        let initial = Cpu {
                            n: flags >> 3,
                            z: (flags >> 2) & 1,
                            c: (flags >> 1) & 1,
                            v: flags & 1,
                            fpcr: 1 << 24,
                            fpsr: 0x08000080,
                            ..Cpu::default()
                        };
                        let mut cpu = initial;
                        cpu.vregs[0] = [123, 456];
                        assert_eq!(unsafe { native(&mut cpu, 10) }, if holds { 3 } else { 0 });
                        let actual = (cpu.n << 3) | (cpu.z << 2) | (cpu.c << 1) | cpu.v;
                        assert_eq!(actual, if holds { flags } else { cond as u32 });
                        assert_eq!(cpu.fpsr, 0x08000080);
                        for snan in [false, true] {
                            let mut cpu = Cpu {
                                n: flags >> 3,
                                z: (flags >> 2) & 1,
                                c: (flags >> 1) & 1,
                                v: flags & 1,
                                fpsr: 0x08000080,
                                ..Cpu::default()
                            };
                            cpu.vregs[0] = [123, 456];
                            cpu.vregs[1][0] = if bytes == 4 {
                                if snan {
                                    0x7f812345
                                } else {
                                    0x7fc12345
                                }
                            } else if snan {
                                0x7ff0123456789abc
                            } else {
                                0x7ff8123456789abc
                            };
                            let mut attached = fp.attach(&mut cpu);
                            assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
                            let cpu = attached.cpu_mut();
                            assert_eq!(
                                (cpu.n << 3) | (cpu.z << 2) | (cpu.c << 1) | cpu.v,
                                if holds { 3 } else { cond as u32 }
                            );
                            assert_eq!(
                                cpu.fpsr,
                                0x08000080 | u32::from(holds && (snan || signaling != 0))
                            );
                            assert_eq!(cpu.vregs[0], [123, 456]);
                        }
                    }
                }
            }
            let native = unsafe {
                library
                    .get::<Entry>(format!("nm_{bytes}\0").as_bytes())
                    .unwrap()
            };
            for dn in [0, 1u32 << 25] {
                let mut cpu = Cpu {
                    fpcr: dn,
                    ..Cpu::default()
                };
                cpu.vregs[1][0] = if bytes == 4 {
                    0x7fc12345
                } else {
                    0x7ff8123456789abc
                };
                cpu.vregs[2][0] = if bytes == 4 {
                    0x3f800000
                } else {
                    0x3ff0000000000000
                };
                let expected = if bytes == 4 {
                    if dn != 0 {
                        0xffc00000
                    } else {
                        0xffc12345
                    }
                } else if dn != 0 {
                    0xfff8000000000000
                } else {
                    0xfff8123456789abc
                };
                let mut attached = fp.attach(&mut cpu);
                assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
                assert_eq!(attached.cpu_mut().vregs[0], [expected, 0]);
                assert_eq!(attached.cpu_mut().fpsr, 0);
            }
        }
    }
}
#[test]
fn fused_sign_inputs_single_rounding_nan_priority_and_refusals() {
    let path = folder("fused-sign-known");
    let mut code = String::new();
    for bytes in [4, 8] {
        for operation in 11..14 {
            code += &generate(
                &[fp_word(bytes, operation, false), 0xd65f03c0],
                &format!("fused_{bytes}_{operation}"),
            );
        }
    }
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for bytes in [4, 8] {
            for operation in 11..14 {
                let native = unsafe {
                    library
                        .get::<Entry>(format!("fused_{bytes}_{operation}\0").as_bytes())
                        .unwrap()
                };
                let (two, three, ten, one, qnan, sign) = if bytes == 4 {
                    (
                        0x40000000,
                        0x40400000,
                        0x41200000,
                        0x3f800000,
                        0x7fc12345,
                        1u64 << 31,
                    )
                } else {
                    (
                        0x4000000000000000,
                        0x4008000000000000,
                        0x4024000000000000,
                        0x3ff0000000000000,
                        0x7ff8123456789abc,
                        1u64 << 63,
                    )
                };
                let ordinary = if bytes == 4 {
                    match operation {
                        11 => 0x40800000,
                        12 => 0xc1800000,
                        _ => 0xc0800000,
                    }
                } else {
                    match operation {
                        11 => 0x4010000000000000,
                        12 => 0xc030000000000000,
                        _ => 0xc010000000000000,
                    }
                };
                for (left, right, acc, expected) in [
                    (two, three, ten, ordinary),
                    (
                        qnan,
                        one,
                        two,
                        if operation == 13 { qnan } else { qnan ^ sign },
                    ),
                    (
                        one,
                        one,
                        qnan,
                        if operation == 11 { qnan } else { qnan ^ sign },
                    ),
                ] {
                    let mut cpu = Cpu {
                        fpsr: 0x08000080,
                        ..Cpu::default()
                    };
                    cpu.vregs[0] = [123, 456];
                    cpu.vregs[1][0] = left;
                    cpu.vregs[2][0] = right;
                    cpu.vregs[3][0] = acc;
                    {
                        let mut a = backend.attach(&mut cpu);
                        assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                    }
                    assert_eq!(cpu.vregs[0], [expected, 0], "{bytes}/{operation}");
                    assert_eq!(cpu.fpsr, 0x08000080);
                }
                let mut cpu = Cpu {
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.vregs[0] = [123, 456];
                assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
                assert_eq!(cpu.vregs[0], [123, 456]);
                cpu.fpcr = 1 << 24;
                {
                    let mut a = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 3);
                }
                assert_eq!(cpu.vregs[0], [123, 456]);
                assert_eq!(cpu.fpsr, 0x08000080);
                if bytes == 4 {
                    cpu.fpcr = 0;
                    cpu.vregs[1][0] = 0x3f800001;
                    cpu.vregs[2][0] = 0x3f7ffffe;
                    cpu.vregs[3][0] = if operation == 12 { one ^ sign } else { one };
                    {
                        let mut a = backend.attach(&mut cpu);
                        assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                    }
                    assert_eq!(
                        cpu.vregs[0],
                        [
                            if operation == 13 {
                                0xa8800000
                            } else {
                                0x28800000
                            },
                            0
                        ]
                    );
                    assert_eq!(cpu.fpsr, 0x08000080);
                }
            }
        }
    }
}

#[test]
fn minmax_signed_zero_nan_priority_number_selection_and_refusals() {
    let path = folder("minmax-known");
    let mut code = String::new();
    for bytes in [4, 8] {
        for operation in 15..19 {
            code += &generate(
                &[fp_word(bytes, operation, false), 0xd65f03c0],
                &format!("minmax_{bytes}_{operation}"),
            );
        }
    }
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for bytes in [4, 8] {
            for operation in 15..19 {
                let native = unsafe {
                    library
                        .get::<Entry>(format!("minmax_{bytes}_{operation}\0").as_bytes())
                        .unwrap()
                };
                let (one, two, qnan, snan, other_snan, sign, default) = if bytes == 4 {
                    (
                        0x3f800000,
                        0x40000000,
                        0x7fc12345,
                        0x7f812345,
                        0x7f823456,
                        1u64 << 31,
                        0x7fc00000,
                    )
                } else {
                    (
                        0x3ff0000000000000,
                        0x4000000000000000,
                        0x7ff8123456789abc,
                        0x7ff0123456789abc,
                        0x7ff023456789abcd,
                        1u64 << 63,
                        0x7ff8000000000000,
                    )
                };
                let quiet = if bytes == 4 { 1u64 << 22 } else { 1u64 << 51 };
                let maximum = operation == 15 || operation == 17;
                let number = operation >= 17;
                for dn in 0..2 {
                    for (a, b, expected, flags) in [
                        (one, two, if maximum { two } else { one }, 0),
                        (
                            one ^ sign,
                            two ^ sign,
                            if maximum { one ^ sign } else { two ^ sign },
                            0,
                        ),
                        (0, sign, if maximum { 0 } else { sign }, 0),
                        (sign, 0, if maximum { 0 } else { sign }, 0),
                        (sign, sign, sign, 0),
                        (qnan, one, if number { one } else { qnan }, 0),
                        (one, qnan, if number { one } else { qnan }, 0),
                        (snan, one, snan | quiet, 1),
                        (one, snan, snan | quiet, 1),
                        (qnan, other_snan, other_snan | quiet, 1),
                        (snan, other_snan, snan | quiet, 1),
                        (qnan, qnan ^ sign, qnan, 0),
                    ] {
                        let is_nan = (expected
                            & if bytes == 4 {
                                0x7f800000
                            } else {
                                0x7ff0000000000000
                            })
                            == if bytes == 4 {
                                0x7f800000
                            } else {
                                0x7ff0000000000000
                            };
                        let expected = if dn == 1 && is_nan { default } else { expected };
                        let mut cpu = Cpu {
                            fpcr: dn << 25,
                            fpsr: 0x08000080,
                            n: 1,
                            z: 0,
                            c: 1,
                            v: 1,
                            ..Cpu::default()
                        };
                        cpu.vregs[0] = [123, 456];
                        cpu.vregs[1][0] = a;
                        cpu.vregs[2][0] = b;
                        {
                            let mut a = backend.attach(&mut cpu);
                            assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                        }
                        assert_eq!(
                            cpu.vregs[0],
                            [expected, 0],
                            "{bytes}/{operation} {a:x} {b:x} DN{dn}"
                        );
                        assert_eq!(cpu.fpsr, 0x08000080 | flags);
                        assert_eq!((cpu.n, cpu.z, cpu.c, cpu.v), (1, 0, 1, 1));
                    }
                }
                for control in [0, 1 << 24, 1 << 8] {
                    let mut cpu = Cpu {
                        fpcr: control,
                        fpsr: 0x08000080,
                        ..Cpu::default()
                    };
                    cpu.vregs[0] = [123, 456];
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
    }
}
