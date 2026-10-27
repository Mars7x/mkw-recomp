use mkw_arm64::floating::{CONVERT_FIXED_SCALE, CONVERT_SCALE_SHIFT};
use mkw_runtime::{
    abi::{ConvertCallback, Cpu},
    floating::FloatingBackend,
};
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
#[derive(Clone, Copy)]
struct Case {
    word: u32,
    bits: u32,
    bytes: u8,
    total: u8,
    op: u32,
    scale: u32,
    vector: bool,
    src: u8,
}
fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for bits in [32, 64] {
        for bytes in [4, 8] {
            for op in [0, 1, 8, 9] {
                for scale in 1..=bits {
                    let base = if op < 2 {
                        0x1e020000 | (op << 16)
                    } else {
                        0x1e180000 | ((op - 8) << 16)
                    };
                    out.push(Case {
                        word: base
                            | (u32::from(bits == 64) << 31)
                            | (u32::from(bytes == 8) << 22)
                            | ((64 - scale) << 10)
                            | 0x20,
                        bits,
                        bytes,
                        total: bytes,
                        op,
                        scale,
                        vector: false,
                        src: 1,
                    });
                }
            }
        }
    }
    for (bytes, total, scalar) in [
        (4, 4, true),
        (8, 8, true),
        (4, 8, false),
        (4, 16, false),
        (8, 16, false),
    ] {
        let bits = u32::from(bytes) * 8;
        for op in [0, 1, 8, 9] {
            for scale in 1..=bits {
                for src in [1, 0] {
                    let word = (if op < 2 { 0x0f00e400 } else { 0x0f00fc00 })
                        | ((op & 1) << 29)
                        | (u32::from(scalar) << 28)
                        | (u32::from(scalar || total == 16) << 30)
                        | ((2 * bits - scale) << 16)
                        | (u32::from(src) << 5);
                    out.push(Case {
                        word,
                        bits,
                        bytes,
                        total,
                        op,
                        scale,
                        vector: true,
                        src,
                    });
                }
            }
        }
    }
    out
}
fn codes(cases: &[Case]) -> String {
    cases
        .iter()
        .map(|c| generate(&[c.word, 0xd65f03c0], &format!("scaled_{:x}", c.word)))
        .collect()
}
fn inputs(bytes: u8, op: u32) -> Vec<u64> {
    if op < 2 {
        return vec![
            0,
            1,
            2,
            0x7fffffff,
            0x80000000,
            0xffffffff,
            0x100000001,
            0x20000000000001,
            0x7fffffffffffffff,
            0x8000000000000000,
            u64::MAX,
            0xfedcba9876543210,
        ];
    }
    if bytes == 4 {
        vec![
            0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x3f000000, 0xbf000000,
            0x3fc00000, 0xbfc00000, 0x4effffff, 0xcf000000, 0x5f000000, 0x7f7fffff, 0xff7fffff,
            0x7f800000, 0xff800000, 0x7fc12345, 0x7f812345, 0xffc12345,
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
            0x41dfffffffffffff,
            0xc1e0000000100000,
            0x43dfffffffffffff,
            0x43e0000000000000,
            0x7fefffffffffffff,
            0xffefffffffffffff,
            0x7ff0000000000000,
            0xfff0000000000000,
            0x7ff8123456789abc,
            0x7ff0123456789abc,
        ]
    }
}
fn oracle(input: u64, bits: u32, bytes: u8, op: u32, scale: u32, mode: u32) -> (u64, u32) {
    let width_mask = (1u128 << bits) - 1;
    let p = if bytes == 4 { 24 } else { 53 };
    let bias = if bytes == 4 { 127 } else { 1023 };
    let exponent_mask = if bytes == 4 { 255 } else { 2047 };
    let sign_shift = u32::from(bytes) * 8 - 1;
    if op < 2 {
        let raw = u128::from(input) & width_mask;
        let negative = op == 0 && raw & (1u128 << (bits - 1)) != 0;
        let magnitude = if negative { (1u128 << bits) - raw } else { raw };
        if magnitude == 0 {
            return (0, 0);
        }
        let leading = 127 - magnitude.leading_zeros();
        let mut exponent = leading as i32 - scale as i32;
        let (mut mantissa, remainder, shift) = if leading >= p - 1 {
            let shift = leading - (p - 1);
            (
                magnitude >> shift,
                magnitude & ((1u128 << shift) - 1),
                shift,
            )
        } else {
            (magnitude << ((p - 1) - leading), 0, 0)
        };
        let increment = if remainder == 0 {
            false
        } else {
            match mode {
                0 => {
                    let half = 1u128 << (shift - 1);
                    remainder > half || (remainder == half && mantissa & 1 != 0)
                }
                1 => !negative,
                2 => negative,
                3 => false,
                _ => unreachable!(),
            }
        };
        if increment {
            mantissa += 1;
            if mantissa == 1u128 << p {
                mantissa >>= 1;
                exponent += 1;
            }
        }
        let result = (u64::from(negative) << sign_shift)
            | (((exponent + bias) as u64) << (p - 1))
            | ((mantissa & ((1u128 << (p - 1)) - 1)) as u64);
        return (result, u32::from(remainder != 0) * 16);
    }
    let sign = (input >> sign_shift) & 1 != 0;
    let exp = ((input >> (p - 1)) & exponent_mask) as i32;
    let fraction = u128::from(input) & ((1u128 << (p - 1)) - 1);
    let invalid = || {
        if op == 9 {
            if sign {
                0
            } else {
                width_mask as u64
            }
        } else if sign {
            (1u64) << (bits - 1)
        } else {
            ((1u128 << (bits - 1)) - 1) as u64
        }
    };
    if exp == exponent_mask as i32 {
        return (if fraction != 0 { 0 } else { invalid() }, 1);
    }
    let significand = if exp == 0 {
        fraction
    } else {
        fraction | (1u128 << (p - 1))
    };
    let shift = (if exp == 0 { 1 - bias } else { exp - bias }) - (p - 1) as i32 + scale as i32;
    let (magnitude, discarded, overflow) = if shift >= 0 {
        let shift = shift as u32;
        if shift >= 128 || significand > (u128::MAX >> shift) {
            (0, false, true)
        } else {
            (significand << shift, false, false)
        }
    } else {
        let shift = (-shift) as u32;
        if shift >= 128 {
            (0, significand != 0, false)
        } else {
            (
                significand >> shift,
                significand & ((1u128 << shift) - 1) != 0,
                false,
            )
        }
    };
    let bound = if op == 9 {
        if sign {
            0
        } else {
            width_mask
        }
    } else if sign {
        1u128 << (bits - 1)
    } else {
        (1u128 << (bits - 1)) - 1
    };
    if overflow || magnitude > bound {
        return (invalid(), 1);
    }
    let result = if sign {
        (-(magnitude as i128)) as u128 & width_mask
    } else {
        magnitude
    };
    (result as u64, u32::from(discarded) * 16)
}
fn frame(values: &[u64], bytes: u8, i: usize) -> [u64; 2] {
    let mut out = [0; 2];
    let mask = if bytes == 4 { 0xffffffff } else { u64::MAX };
    for lane in 0..16 / usize::from(bytes) {
        let offset = lane * usize::from(bytes) * 8;
        out[offset / 64] |= (values[(i + lane * 3) % values.len()] & mask) << (offset % 64);
    }
    out
}
#[test]
fn scaled_helpers_match_exact_integer_rational_oracle_and_reject_noncanonical_ops() {
    let path = folder("scaled-helper-known");
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = backend(&path, opt);
        let convert = unsafe { lib.get::<ConvertCallback>(b"mkw_convert\0").unwrap() };
        for bits in [32, 64] {
            for bytes in [4, 8] {
                for op in [0, 1, 8, 9] {
                    for scale in 1..=bits {
                        for input in inputs(bytes, op) {
                            for mode in 0..4 {
                                for dn in 0..2 {
                                    let operation =
                                        CONVERT_FIXED_SCALE | (scale << CONVERT_SCALE_SHIFT) | op;
                                    let (mut result, mut flags) = (123, 456);
                                    let (expected, raised) =
                                        oracle(input, bits, bytes, op, scale, mode);
                                    assert_eq!(
                                        unsafe {
                                            convert(
                                                operation,
                                                bits,
                                                u32::from(bytes),
                                                (mode << 22) | (dn << 25),
                                                0x08000080,
                                                input,
                                                &mut result,
                                                &mut flags,
                                            )
                                        },
                                        0
                                    );
                                    assert_eq!((result,flags),(expected,0x08000080|raised),"{opt} op{op} bits{bits} bytes{bytes} scale{scale} input{input:x} mode{mode} DN{dn}");
                                    if opt == "-O0" {
                                        count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for operation in [
            0x10008, 0x14108, 0x12002, 0x12088, 0x112008, 0x18008, 0x10000000,
        ] {
            let (mut result, mut flags) = (123, 456);
            assert_eq!(
                unsafe { convert(operation, 32, 4, 0, 0, 1, &mut result, &mut flags) },
                3
            );
            assert_eq!((result, flags), (123, 456));
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"cases_per_optimization":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"independent_rational_oracle":true})).unwrap()).unwrap();
}
#[test]
fn scaled_aot_results_and_flags_match_synthetic_qemu() {
    let path = folder("scaled-aot-reference");
    let instructions = cases();
    let qemu_available = Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_ok();
    let mut checks = 0;
    for vector in [false, true] {
        let filtered: Vec<_> = instructions
            .iter()
            .copied()
            .filter(|c| c.vector == vector)
            .collect();
        let code = codes(&filtered);
        let mut frames = Vec::new();
        let mut owners = Vec::new();
        for c in &filtered {
            let values = inputs(c.bytes, c.op);
            for i in 0..values.len() {
                for mode in 0..4 {
                    for dn in 0..2 {
                        let mut vectors = reference::INITIAL_V;
                        vectors[usize::from(c.src)] = if vector {
                            frame(&values, c.bytes, i)
                        } else {
                            [values[i], reference::INITIAL_V[1][1]]
                        };
                        frames.push(reference::VectorCase {
                            word: c.word,
                            vectors,
                            fpcr: (mode << 22) | (dn << 25),
                            fpsr: 0x08000080,
                            nzcv: 0xb0000000,
                        });
                        owners.push((*c, values[i]));
                    }
                }
            }
        }
        let mut oracle = Vec::new();
        use std::os::unix::fs::PermissionsExt;
        if qemu_available {
            for (i, chunk) in frames.chunks(2000).enumerate() {
                let image = path.join(format!("synthetic-aarch64-{}-{i}", u8::from(vector)));
                let elf = if vector {
                    reference::vector_elf(chunk)
                } else {
                    let gp: Vec<_> = chunk
                        .iter()
                        .map(|c| reference::FloatingCase {
                            word: c.word,
                            left: c.vectors[1][0],
                            right: 0,
                            accumulate: 0,
                            fpcr: c.fpcr,
                            fpsr: c.fpsr,
                            nzcv: c.nzcv,
                        })
                        .collect();
                    reference::conversion_elf(&gp)
                };
                std::fs::write(&image, elf).unwrap();
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
        }
        let stride = if vector { 32 } else { 48 };
        if qemu_available {
            assert_eq!(oracle.len(), frames.len() * stride);
        }
        for opt in ["-O0", "-O2"] {
            let lib = build(&code, &path, opt);
            let fp_lib = backend(&path, opt);
            let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
            for (i, c) in frames.iter().enumerate() {
                let (info, _) = owners[i];
                let native = unsafe {
                    lib.get::<Entry>(format!("scaled_{:x}\0", c.word).as_bytes())
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
                if vector {
                    cpu.vregs[..4].copy_from_slice(&c.vectors);
                } else {
                    cpu.vregs[0] = reference::INITIAL_V[0];
                    cpu.vregs[1] = [c.vectors[1][0], reference::INITIAL_V[1][1]];
                    cpu.x[0] = reference::INITIAL_X[0];
                    cpu.x[1] = c.vectors[1][0];
                }
                let before = cpu.vregs;
                let general = cpu.x;
                let mut expected = [0; 2];
                let mut flags = 0;
                if vector {
                    for lane in 0..info.total / info.bytes {
                        let offset = lane * info.bytes * 8;
                        let mask = if info.bytes == 4 {
                            0xffffffff
                        } else {
                            u64::MAX
                        };
                        let input = (before[usize::from(info.src)][usize::from(offset / 64)]
                            >> (offset % 64))
                            & mask;
                        let (v, f) = self::oracle(
                            input,
                            info.bits,
                            info.bytes,
                            info.op,
                            info.scale,
                            (c.fpcr >> 22) & 3,
                        );
                        expected[usize::from(offset / 64)] |= (v & mask) << (offset % 64);
                        flags |= f;
                    }
                } else {
                    let (v, f) = self::oracle(
                        if info.op < 2 {
                            cpu.x[1]
                        } else {
                            cpu.vregs[1][0]
                        },
                        info.bits,
                        info.bytes,
                        info.op,
                        info.scale,
                        (c.fpcr >> 22) & 3,
                    );
                    expected[0] = v;
                    flags = f;
                }
                {
                    let mut a = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                }
                assert_eq!(
                    cpu.fpsr,
                    0x08000080 | flags,
                    "{opt} {:x} fixture{i}",
                    c.word
                );
                if vector || info.op < 2 {
                    assert_eq!(cpu.vregs[0], expected, "{opt} {:x} fixture{i}", c.word);
                    assert_eq!(cpu.x, general);
                } else {
                    assert_eq!(cpu.x[0], expected[0]);
                    assert_eq!(cpu.vregs, before);
                }
                if vector {
                    assert_eq!(cpu.vregs[1..], before[1..]);
                }
                let mut values = vec![
                    cpu.vregs[0][0],
                    cpu.vregs[0][1],
                    u64::from(cpu.fpsr),
                    0xb0000000,
                ];
                if !vector {
                    values.extend([cpu.x[0], u64::from(cpu.fpcr)]);
                }
                let actual: Vec<_> = values.into_iter().flat_map(u64::to_le_bytes).collect();
                if qemu_available {
                    assert_eq!(
                        actual,
                        &oracle[i * stride..(i + 1) * stride],
                        "{opt} {:x} fixture{i}",
                        c.word
                    );
                }
                if opt == "-O0" {
                    checks += 1;
                }
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":checks,"qemu_reference":if qemu_available{"available"}else{"unavailable"},"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}

static LANE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe extern "C" fn fail_second(
    operation: u32,
    bits: u32,
    bytes: u32,
    _: u32,
    fpsr: u32,
    _: u64,
    result: *mut u64,
    status: *mut u32,
) -> i32 {
    assert_eq!(
        (operation, bits, bytes),
        (CONVERT_FIXED_SCALE | (16 << CONVERT_SCALE_SHIFT) | 8, 32, 4)
    );
    let n = LANE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n == 1 {
        return 3;
    }
    unsafe {
        *result = 17;
        *status = fpsr | 16;
    }
    0
}
#[test]
fn scaled_register31_refusals_and_deferred_lane_commit() {
    let path = folder("scaled-aot-known");
    let words = [
        0x1e0283ff, 0x9e4303ff, 0x1e1883ff, 0x4f20ffff, 0x4f40e7ff, 0x5f40e7ff, 0x4f30fc20,
    ];
    let code: String = words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("edge_{w:x}")))
        .collect();
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (w, input, expected, flags) in [
            (0x1e0283ffu32, [123, 456], [0, 0], 0),
            (0x9e4303ff, [123, 456], [0, 0], 0),
            (0x1e1883ff, [0x7fc12345, 456], [0x7fc12345, 456], 1),
            (
                0x4f20ffff,
                [0x3f0000003f000000; 2],
                [0x7fffffff7fffffff; 2],
                1,
            ),
            (
                0x4f40e7ff,
                [0x8000000000000000; 2],
                [0xbfe0000000000000; 2],
                0,
            ),
            (
                0x5f40e7ff,
                [0x8000000000000000; 2],
                [0xbfe0000000000000, 0],
                0,
            ),
        ] {
            let native = unsafe {
                lib.get::<Entry>(format!("edge_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                sp: 99,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.x[0] = 777;
            cpu.vregs[31] = input;
            {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            assert_eq!(cpu.vregs[31], expected, "{w:x}");
            assert_eq!((cpu.x[0], cpu.sp, cpu.fpsr), (777, 99, 0x08000080 | flags));
        }
        for &w in &words {
            let native = unsafe {
                lib.get::<Entry>(format!("edge_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for control in [0, 1 << 8, 1 << 12, 1 << 24, 1 << 19, 1 << 26] {
                let mut cpu = Cpu {
                    fpcr: control,
                    fpsr: 0x08000080,
                    sp: 99,
                    ..Cpu::default()
                };
                cpu.vregs[0] = [123, 456];
                cpu.vregs[31] = [123, 456];
                let before = cpu.vregs;
                if control == 0 {
                    assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
                } else {
                    let mut a = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 3);
                }
                assert_eq!(cpu.vregs, before);
                assert_eq!((cpu.fpsr, cpu.sp), (0x08000080, 99));
            }
        }
        let native = unsafe { lib.get::<Entry>(b"edge_4f30fc20\0").unwrap() };
        LANE_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
        let mut cpu = Cpu {
            convert: Some(fail_second),
            fpsr: 0x08000080,
            ..Cpu::default()
        };
        cpu.vregs[0] = [123, 456];
        assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
        assert_eq!(LANE_CALLS.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert_eq!(cpu.vregs[0], [123, 456]);
        assert_eq!(cpu.fpsr, 0x08000080);
    }
}
