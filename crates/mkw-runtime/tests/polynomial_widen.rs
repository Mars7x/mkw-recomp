use mkw_arm64::Op;
use mkw_runtime::abi::Cpu;
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(full: bool) -> Vec<u32> {
    let mut out = Vec::new();
    for q in 0..2 {
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
            out.push(0x0ee0e000 | (q << 30) | (m << 16) | (n << 5) | d);
        }
        for (d, n) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
            if !full && d == 31 {
                continue;
            }
            out.push(0x0e617800 | (q << 30) | (n << 5) | d);
        }
    }
    out
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("poly_widen_{w:x}")))
        .collect()
}
fn op(w: u32) -> Op {
    mkw_arm64::decode(w, 0).unwrap().op
}
fn polynomial(a: u64, b: u64) -> [u64; 2] {
    // Independent coefficient convolution: sum each diagonal modulo two.
    let mut out = [0u64; 2];
    for degree in 0usize..127 {
        let mut parity = 0;
        for left in degree.saturating_sub(63)..=degree.min(63) {
            parity ^= ((a >> left) & 1) & ((b >> (degree - left)) & 1);
        }
        out[degree / 64] |= parity << (degree % 64);
    }
    out
}
fn widen(bits: u32, fpcr: u32) -> (u64, u32) {
    if bits & 0x7f800000 == 0x7f800000 && bits & 0x7fffff != 0 {
        let invalid = u32::from(bits & 0x400000 == 0);
        let value = if fpcr & (1 << 25) != 0 {
            0x7ff8000000000000
        } else {
            (u64::from(bits >> 31) << 63) | 0x7ff8000000000000 | (u64::from(bits & 0x7fffff) << 29)
        };
        (value, invalid)
    } else {
        ((f32::from_bits(bits) as f64).to_bits(), 0)
    }
}
fn expected(op: Op, v: &[[u64; 2]; 32], fpcr: u32, fpsr: u32) -> (usize, [u64; 2], u32) {
    match op {
        Op::PolynomialMultiply(p) => (
            usize::from(p.dst),
            polynomial(
                v[usize::from(p.lhs)][usize::from(p.upper)],
                v[usize::from(p.rhs)][usize::from(p.upper)],
            ),
            fpsr,
        ),
        Op::WidenSingleVector { dst, src, upper } => {
            let raw = v[usize::from(src)][usize::from(upper)];
            let (a, af) = widen(raw as u32, fpcr);
            let (b, bf) = widen((raw >> 32) as u32, fpcr);
            (usize::from(dst), [a, b], fpsr | af | bf)
        }
        _ => panic!(),
    }
}
fn values(op: Op, full: bool) -> Vec<(u64, u64)> {
    match op {
        Op::PolynomialMultiply(_) => {
            let mut v: Vec<u64> = vec![
                0,
                1,
                u64::MAX,
                0xaaaaaaaaaaaaaaaa,
                0x5555555555555555,
                0x8000000000000000,
            ];
            if full {
                v.extend((0..64).map(|n| 1u64 << n));
            } else {
                v.extend([0x0123456789abcdef, 0xfedcba9876543210]);
            }
            v.iter()
                .flat_map(|&a| v.iter().map(move |&b| (a, b)))
                .collect()
        }
        Op::WidenSingleVector { .. } => {
            let mut v: Vec<u32> = vec![
                0, 0x80000000, 1, 0x007fffff, 0x00800000, 0x3f800000, 0xbf800000, 0x7f7fffff,
                0xff7fffff, 0x7f800000, 0xff800000, 0x7fc00000, 0xffc12345, 0x7f800001, 0xff800001,
            ];
            if full {
                for exponent in 0..256 {
                    for fraction in [0, 1, 0x3fffff, 0x7fffff] {
                        for sign in 0..2 {
                            v.push((sign << 31) | (exponent << 23) | fraction);
                        }
                    }
                }
            }
            v.into_iter()
                .enumerate()
                .map(|(n, a)| {
                    (
                        u64::from(a) | (u64::from(a.rotate_left((n % 31 + 1) as u32)) << 32),
                        0,
                    )
                })
                .collect()
        }
        _ => panic!(),
    }
}
fn input(op: Op, a: u64, b: u64) -> [[u64; 2]; 32] {
    let mut v = std::array::from_fn(|n| {
        [
            0x123456789abcdef0 ^ (n as u64),
            0xfedcba9876543210 ^ (n as u64),
        ]
    });
    match op {
        Op::PolynomialMultiply(p) => {
            let h = usize::from(p.upper);
            v[usize::from(p.lhs)][h] = a;
            v[usize::from(p.rhs)][h] = b;
        }
        Op::WidenSingleVector { src, upper, .. } => v[usize::from(src)][usize::from(upper)] = a,
        _ => panic!(),
    }
    v
}
#[test]
fn polynomial_and_widen_match_independent_oracles() {
    let path = folder("polynomial-widen-known");
    let words = words(true);
    let source = code(&words);
    let mut counts = [0; 2];
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let backend = backend(&path, opt);
        let fp = floating(&backend);
        let mut observed = [0; 2];
        for &w in &words {
            let op = op(w);
            let f = unsafe {
                lib.get::<Entry>(format!("poly_widen_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b) in values(op, true) {
                for fpcr in [
                    0, 0x00400000, 0x00800000, 0x00c00000, 0x02000000, 0x02c00000,
                ] {
                    let mut cpu = Cpu {
                        vregs: input(op, a, b),
                        fpcr,
                        fpsr: 0x08000090,
                        n: 1,
                        c: 1,
                        v: 1,
                        sp: 87,
                        fault_status: 77,
                        fault_address: 91,
                        floating: Some(fp),
                        ..Cpu::default()
                    };
                    cpu.x = std::array::from_fn(|n| 100 + n as u64);
                    let x = cpu.x;
                    let mut want = cpu.vregs;
                    let (d, result, flags) = expected(op, &want, fpcr, cpu.fpsr);
                    want[d] = result;
                    assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
                    assert_eq!(cpu.vregs, want, "{opt} {w:x} {a:x} {b:x} {fpcr:x}");
                    assert_eq!(cpu.fpsr, flags);
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
                        (fpcr, 1, 0, 1, 1, 87, 77, 91)
                    );
                    observed[usize::from(matches!(op, Op::WidenSingleVector { .. }))] += 1;
                }
            }
        }
        if counts == [0; 2] {
            counts = observed;
        } else {
            assert_eq!(counts, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"polynomial_native_cases_per_optimization":counts[0],"widen_native_cases_per_optimization":counts[1],"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn polynomial_and_widen_match_synthetic_qemu() {
    let path = folder("polynomial-widen-reference");
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
        let op = op(w);
        for (a, b) in values(op, false) {
            for fpcr in [
                0, 0x00400000, 0x00800000, 0x00c00000, 0x02000000, 0x02c00000,
            ] {
                cases.push(reference::VectorCase {
                    word: w,
                    vectors: input(op, a, b)[..4].try_into().unwrap(),
                    fpcr,
                    fpsr: 0x08000090,
                    nzcv: 0xb0000000,
                });
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
        let lib = build(&source, &path, opt);
        let back = backend(&path, opt);
        let fp = floating(&back);
        for (i, c) in cases.iter().enumerate() {
            let f = unsafe {
                lib.get::<Entry>(format!("poly_widen_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                n: 1,
                c: 1,
                v: 1,
                floating: Some(fp),
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let mut want = cpu.vregs;
            let (d, value, flags) = expected(op(c.word), &want, c.fpcr, c.fpsr);
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
thread_local! {static CALLS:std::cell::Cell<u32>=const{std::cell::Cell::new(0)};static FAIL:std::cell::Cell<u32>=const{std::cell::Cell::new(0)};}
unsafe extern "C" fn fail_later(
    operation: u32,
    bytes: u32,
    _fpcr: u32,
    _fpsr: u32,
    _a: u64,
    _b: u64,
    _c: u64,
    result: *mut u64,
    flags: *mut u32,
) -> i32 {
    assert_eq!((operation, bytes), (14, 4));
    let call = CALLS.with(|v| {
        let n = v.get() + 1;
        v.set(n);
        n
    });
    unsafe {
        *result = 0xdead;
        *flags = 0xdead;
    }
    if call == FAIL.with(|v| v.get()) {
        2
    } else {
        0
    }
}
#[test]
fn widening_callback_failures_are_atomic_and_profiles_stay_checked() {
    let path = folder("polynomial-widen-failure");
    let words: Vec<_> = words(true)
        .into_iter()
        .filter(|&w| matches!(op(w), Op::WidenSingleVector { .. }))
        .collect();
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let back = backend(&path, opt);
        let fp = floating(&back);
        for &w in &words {
            let f = unsafe {
                lib.get::<Entry>(format!("poly_widen_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for stage in 0..5 {
                CALLS.with(|v| v.set(0));
                FAIL.with(|v| v.set(stage));
                let mut cpu = Cpu {
                    vregs: input(op(w), 0x7f8000013f800000, 0),
                    fpsr: 0x08000090,
                    fpcr: if stage == 4 { 1 << 24 } else { 0 },
                    floating: if stage == 0 {
                        None
                    } else if stage == 4 {
                        Some(fp)
                    } else {
                        Some(fail_later)
                    },
                    ..Cpu::default()
                };
                let before = cpu.vregs;
                let flags = cpu.fpsr;
                let status = unsafe { f(&mut cpu, 10) };
                if stage == 3 {
                    assert_eq!(status, 0);
                    assert_eq!(CALLS.with(|v| v.get()), 2);
                } else {
                    assert_eq!(status, if stage == 0 || stage == 4 { 3 } else { 2 });
                    assert_eq!(cpu.vregs, before);
                    assert_eq!(cpu.fpsr, flags);
                    assert_eq!(
                        CALLS.with(|v| v.get()),
                        if stage == 0 || stage == 4 { 0 } else { stage }
                    );
                }
            }
        }
    }
}

fn floating(library: &libloading::Library) -> mkw_runtime::abi::FloatingCallback {
    // SAFETY: privately built attributed backend with checked callback ABI version.
    unsafe {
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> u32>(b"mkw_fp_abi_version\0")
                .unwrap()(),
            mkw_runtime::abi::CPU_ABI_VERSION
        );
        *library
            .get::<mkw_runtime::abi::FloatingCallback>(b"mkw_fp\0")
            .unwrap()
    }
}
