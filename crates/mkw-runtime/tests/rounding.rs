use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
#[derive(Clone, Copy)]
struct Case {
    word: u32,
    bytes: u8,
    total: u8,
    operation: u8,
    dst: u8,
    src: u8,
}
fn instructions() -> Vec<Case> {
    let scalar = [
        0x1e244000, 0x1e24c000, 0x1e254000, 0x1e25c000, 0x1e264000, 0x1e274000, 0x1e27c000,
    ];
    let vector = [
        0x0e218800, 0x0ea18800, 0x0e219800, 0x0ea19800, 0x2e218800, 0x2e219800, 0x2ea19800,
    ];
    let mut out = Vec::new();
    for (bytes, total, fields, is_scalar) in [
        (4, 4, 0, true),
        (8, 8, 1 << 22, true),
        (4, 8, 0, false),
        (4, 16, 1 << 30, false),
        (8, 16, (1 << 30) | (1 << 22), false),
    ] {
        for (i, base) in (if is_scalar { scalar } else { vector })
            .into_iter()
            .enumerate()
        {
            for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                out.push(Case {
                    word: base | fields | (u32::from(src) << 5) | u32::from(dst),
                    bytes,
                    total,
                    operation: 19 + i as u8,
                    dst,
                    src,
                });
            }
        }
    }
    out
}
fn codes(cases: &[Case]) -> String {
    cases
        .iter()
        .map(|c| generate(&[c.word, 0xd65f03c0], &format!("round_{:x}", c.word)))
        .collect()
}
fn values(bytes: u8) -> Vec<u64> {
    let mut out = if bytes == 4 {
        vec![
            0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x3f000000, 0xbf000000,
            0x3fc00000, 0xbfc00000, 0x40200000, 0xc0200000, 0x3e800000, 0xbe800000, 0x3f800000,
            0xbf800000, 0x4b7fffff, 0x4b800000, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc12345,
            0x7f812345, 0xffc12345, 0xff812345,
        ]
    } else {
        vec![
            0,
            0x8000000000000000,
            1,
            0x8000000000000001,
            0x000fffffffffffff,
            0x0010000000000000,
            0x3fe0000000000000,
            0xbfe0000000000000,
            0x3ff8000000000000,
            0xbff8000000000000,
            0x4004000000000000,
            0xc004000000000000,
            0x3fd0000000000000,
            0xbfd0000000000000,
            0x3ff0000000000000,
            0xbff0000000000000,
            0x432fffffffffffff,
            0x4330000000000000,
            0x7fefffffffffffff,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8123456789abc,
            0x7ff0123456789abc,
            0xfff8123456789abc,
            0xfff0123456789abc,
        ]
    };
    let mut random = 0xafe9123d0768a5cbu64;
    for _ in 0..16 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        out.push(if bytes == 4 {
            random & 0xffffffff
        } else {
            random
        });
    }
    out
}
fn frame(values: &[u64], bytes: u8, i: usize, reg: usize) -> [u64; 2] {
    let mut out = [0; 2];
    for lane in 0..16 / usize::from(bytes) {
        let offset = lane * usize::from(bytes) * 8;
        out[offset / 64] |= values[(i + reg * 3 + lane * 5) % values.len()] << (offset % 64);
    }
    out
}
fn oracle(input: u64, bytes: u8, op: u8, fpcr: u32) -> (u64, u32) {
    let (exponent, fraction, quiet, default) = if bytes == 4 {
        (0x7f800000, 0x007fffff, 0x00400000, 0x7fc00000)
    } else {
        (
            0x7ff0000000000000,
            0x000fffffffffffff,
            0x0008000000000000,
            0x7ff8000000000000,
        )
    };
    if input & exponent == exponent && input & fraction != 0 {
        return (
            if fpcr & (1 << 25) != 0 {
                default
            } else {
                input | quiet
            },
            u32::from(input & quiet == 0),
        );
    }
    let mode = if op <= 23 {
        op - 19
    } else {
        ((fpcr >> 22) & 3) as u8
    };
    if bytes == 4 {
        let a = f32::from_bits(input as u32);
        let v = match mode {
            0 => a.round_ties_even(),
            1 => a.ceil(),
            2 => a.floor(),
            3 => a.trunc(),
            4 => a.round(),
            _ => unreachable!(),
        };
        (
            u64::from(v.to_bits()),
            if op == 24 && a != v { 16 } else { 0 },
        )
    } else {
        let a = f64::from_bits(input);
        let v = match mode {
            0 => a.round_ties_even(),
            1 => a.ceil(),
            2 => a.floor(),
            3 => a.trunc(),
            4 => a.round(),
            _ => unreachable!(),
        };
        (v.to_bits(), if op == 24 && a != v { 16 } else { 0 })
    }
}
unsafe extern "C" fn fail_second(
    _: u32,
    _: u32,
    _: u32,
    fpsr: u32,
    input: u64,
    _: u64,
    _: u64,
    result: *mut u64,
    status: *mut u32,
) -> i32 {
    if input == 2 {
        return 3;
    }
    unsafe {
        *result = 99;
        *status = fpsr | 16;
    }
    0
}
#[test]
fn integral_rounding_modes_nan_flags_aliases_and_refusals() {
    let path = folder("rounding-known");
    let cases = instructions();
    let code = codes(&cases);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for c in &cases {
            let native = unsafe {
                lib.get::<Entry>(format!("round_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let values = values(c.bytes);
            for i in 0..values.len() {
                for mode in 0..4 {
                    for dn in 0..2 {
                        let fpcr = (mode << 22) | (dn << 25);
                        let mut cpu = Cpu {
                            fpcr,
                            fpsr: 0x08000080,
                            n: 1,
                            z: 0,
                            c: 1,
                            v: 1,
                            sp: 99,
                            ..Cpu::default()
                        };
                        cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                        for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                            *v = frame(&values, c.bytes, i, reg);
                        }
                        let before = cpu.vregs;
                        let general = cpu.x;
                        let mut expected = [0u64; 2];
                        let mut flags = 0;
                        for lane in 0..c.total / c.bytes {
                            let bits = c.bytes * 8;
                            let offset = lane * bits;
                            let mask = if c.bytes == 4 { 0xffffffff } else { u64::MAX };
                            let input = (before[usize::from(c.src)][usize::from(offset / 64)]
                                >> (offset % 64))
                                & mask;
                            let (value, raised) = oracle(input, c.bytes, c.operation, fpcr);
                            expected[usize::from(offset / 64)] |= value << (offset % 64);
                            flags |= raised;
                        }
                        {
                            let mut a = backend.attach(&mut cpu);
                            assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                        }
                        for (reg, old) in before.iter().enumerate() {
                            assert_eq!(
                                cpu.vregs[reg],
                                if reg == usize::from(c.dst) {
                                    expected
                                } else {
                                    *old
                                },
                                "{opt} {:x} pattern {i} mode{mode} DN{dn} v{reg}",
                                c.word
                            );
                        }
                        assert_eq!(cpu.fpsr, 0x08000080 | flags);
                        assert_eq!(cpu.x, general);
                        assert_eq!(
                            (cpu.fpcr, cpu.n, cpu.z, cpu.c, cpu.v, cpu.sp),
                            (fpcr, 1, 0, 1, 1, 99)
                        );
                        if opt == "-O0" {
                            count += 1;
                        }
                    }
                }
            }
            for control in [0, 1 << 8, 1 << 12, 1 << 24, 1 << 19, 1 << 26] {
                let mut cpu = Cpu {
                    fpcr: control,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.vregs[usize::from(c.dst)] = [123, 456];
                let before = cpu.vregs;
                if control == 0 {
                    assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
                } else {
                    let mut a = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 3);
                }
                assert_eq!(cpu.vregs, before);
                assert_eq!(cpu.fpsr, 0x08000080);
            }
        }
        let native = unsafe { lib.get::<Entry>(b"round_6e219820\0").unwrap() };
        let mut cpu = Cpu {
            floating: Some(fail_second),
            fpsr: 0x08000080,
            ..Cpu::default()
        };
        cpu.vregs[0] = [123, 456];
        cpu.vregs[1] = [0x0000000200000001, 0];
        assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
        assert_eq!(cpu.vregs[0], [123, 456]);
        assert_eq!(cpu.fpsr, 0x08000080);
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"cases_per_optimization":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true})).unwrap()).unwrap();
}
#[test]
fn rounding_results_and_flags_match_synthetic_qemu() {
    let path = folder("rounding-reference");
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
    let instructions: Vec<_> = instructions().into_iter().filter(|c| c.dst == 0).collect();
    let mut cases = Vec::new();
    for c in &instructions {
        let values = values(c.bytes);
        for i in 0..values.len() {
            for mode in 0..4 {
                for dn in 0..2 {
                    cases.push(reference::VectorCase {
                        word: c.word,
                        vectors: std::array::from_fn(|r| frame(&values, c.bytes, i, r)),
                        fpcr: (mode << 22) | (dn << 25),
                        fpsr: 0x08000080,
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
    let code = codes(&instructions);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("round_{:x}\0", c.word).as_bytes())
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
                "{opt} fixture {i} {:x} FPCR {:x}",
                c.word,
                c.fpcr
            );
            assert_eq!(cpu.vregs[1..], before[1..]);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
