use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::{cell::Cell, collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn step(w: u32) -> bool {
    w & 0xbfe0fc00 == 0x0ea0fc00
}
fn words(full: bool) -> Vec<u32> {
    let mut out = BTreeSet::new();
    for q in 0..2 {
        for is_step in [false, true] {
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
                let base = if is_step {
                    0x0ea0fc00 | (m << 16)
                } else {
                    0x2ea1d800
                };
                out.insert(base | (q << 30) | (n << 5) | d);
            }
        }
    }
    out.into_iter().collect()
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("rsqrt_{w:x}")))
        .collect()
}
fn frame(w: u32, a: u32, b: u32) -> [[u64; 2]; 32] {
    let mut f = std::array::from_fn(|n| [n as u64 + 17, 0xfedcba9876543210]);
    let vector = |a: u32| {
        [
            u64::from(a) | (u64::from(a ^ 0x80000000) << 32),
            u64::from(a.rotate_left(11)) | (u64::from(a.rotate_right(7)) << 32),
        ]
    };
    if step(w) {
        f[((w >> 16) & 31) as usize] = vector(b);
    }
    f[((w >> 5) & 31) as usize] = vector(a);
    f
}
fn estimate_expected(a: u32, fpcr: u32) -> (u32, u32) {
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
    if abs == 0 {
        return ((a & 0x80000000) | 0x7f800000, 2);
    }
    if a & 0x80000000 != 0 {
        return (0x7fc00000, 1);
    }
    if abs == 0x7f800000 {
        return (0, 0);
    }
    let mut scaled = f64::from(f32::from_bits(a));
    let mut power = 0;
    while scaled < 0.25 {
        scaled *= 4.0;
        power += 1;
    }
    while scaled >= 1.0 {
        scaled *= 0.25;
        power -= 1;
    }
    let grid = if scaled < 0.5 { 512.0 } else { 256.0 };
    let midpoint = ((scaled * grid).floor() + 0.5) / grid;
    let approximation = (256.0 / midpoint.sqrt() + 0.5).floor() / 256.0;
    (((approximation * 2f64.powi(power)) as f32).to_bits(), 0)
}
// An independent fixed-point exact oracle: nine limbs at scale 2^-299.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Wide([u64; 9]);
impl Wide {
    fn shifted(value: u64, shift: usize) -> Self {
        let mut out = Self::default();
        let n = shift / 64;
        let bits = shift % 64;
        out.0[n] = value << bits;
        if bits != 0 {
            out.0[n + 1] = value >> (64 - bits);
        }
        out
    }
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.iter().rev().cmp(other.0.iter().rev())
    }
    fn add(self, other: Self) -> Self {
        let mut out = Self::default();
        let mut carry = false;
        for n in 0..9 {
            let (a, c1) = self.0[n].overflowing_add(other.0[n]);
            let (b, c2) = a.overflowing_add(u64::from(carry));
            out.0[n] = b;
            carry = c1 || c2;
        }
        assert!(!carry);
        out
    }
    fn sub(self, other: Self) -> Self {
        let mut out = Self::default();
        let mut borrow = false;
        for n in 0..9 {
            let (a, c1) = self.0[n].overflowing_sub(other.0[n]);
            let (b, c2) = a.overflowing_sub(u64::from(borrow));
            out.0[n] = b;
            borrow = c1 || c2;
        }
        assert!(!borrow);
        out
    }
    fn bit(self, n: usize) -> bool {
        self.0[n / 64] & (1 << (n % 64)) != 0
    }
    fn below(self, n: usize) -> bool {
        let full = n / 64;
        self.0[..full].iter().any(|&x| x != 0)
            || (n % 64 != 0 && self.0[full] & ((1 << (n % 64)) - 1) != 0)
    }
    fn high(self) -> Option<usize> {
        self.0
            .iter()
            .rposition(|&x| x != 0)
            .map(|n| n * 64 + 63 - self.0[n].leading_zeros() as usize)
    }
}
fn refinement_expected(a: u32, b: u32, fpcr: u32) -> (u32, u32) {
    let negated = a ^ 0x80000000;
    let aa = a & 0x7fffffff;
    let bb = b & 0x7fffffff;
    let na = aa > 0x7f800000;
    let nb = bb > 0x7f800000;
    let sa = na && a & 0x400000 == 0;
    let sb = nb && b & 0x400000 == 0;
    if na || nb {
        let selected = if sa {
            negated
        } else if sb {
            b
        } else if na {
            negated
        } else {
            b
        };
        return (
            if fpcr & (1 << 25) != 0 {
                0x7fc00000
            } else {
                selected | 0x400000
            },
            u32::from(sa || sb),
        );
    }
    if (aa == 0x7f800000 && bb == 0) || (bb == 0x7f800000 && aa == 0) {
        return (0x3fc00000, 0);
    }
    if aa == 0x7f800000 || bb == 0x7f800000 {
        return (((a ^ b ^ 0x80000000) & 0x80000000) | 0x7f800000, 0);
    }
    let decompose = |x: u32| {
        let e = (x >> 23) & 255;
        let m = u64::from((x & 0x7fffff) | if e != 0 { 1 << 23 } else { 0 });
        (m, e.max(1))
    };
    let (ma, ea) = decompose(a);
    let (mb, eb) = decompose(b);
    let product = Wide::shifted(ma * mb, (ea + eb - 2) as usize);
    let constant = Wide::shifted(3, 298);
    let (magnitude, negative) = if (a ^ b) & 0x80000000 != 0 {
        (constant.add(product), false)
    } else if constant.cmp(&product) != std::cmp::Ordering::Less {
        (constant.sub(product), false)
    } else {
        (product.sub(constant), true)
    };
    let mode = (fpcr >> 22) & 3;
    let Some(high) = magnitude.high() else {
        return (if mode == 2 { 0x80000000 } else { 0 }, 0);
    };
    assert!(high >= 252);
    let shift = high - 23;
    let mut retained = 0u32;
    for bit in 0..24 {
        if magnitude.bit(shift + bit) {
            retained |= 1 << bit;
        }
    }
    let remainder = magnitude.below(shift);
    let up = match mode {
        0 => magnitude.bit(shift - 1) && (magnitude.below(shift - 1) || retained & 1 != 0),
        1 => !negative && remainder,
        2 => negative && remainder,
        _ => false,
    };
    retained += u32::from(up);
    let mut exponent = high as i32 - 299;
    if retained == 1 << 24 {
        retained >>= 1;
        exponent += 1;
    }
    let sign = if negative { 0x80000000 } else { 0 };
    if exponent > 127 {
        let infinity = mode == 0 || (mode == 1 && !negative) || (mode == 2 && negative);
        return (sign | if infinity { 0x7f800000 } else { 0x7f7fffff }, 20);
    }
    assert!(exponent >= -126);
    (
        sign | (((exponent + 127) as u32) << 23) | (retained & 0x7fffff),
        if remainder { 16 } else { 0 },
    )
}
fn expected(w: u32, before: &[[u64; 2]; 32], fpcr: u32, fpsr: u32) -> ([u64; 2], u32) {
    let n = ((w >> 5) & 31) as usize;
    let m = ((w >> 16) & 31) as usize;
    let count = if w & (1 << 30) != 0 { 4 } else { 2 };
    let mut out = [0u64; 2];
    let mut flags = fpsr;
    for lane in 0..count {
        let a = (before[n][lane / 2] >> ((lane % 2) * 32)) as u32;
        let b = (before[m][lane / 2] >> ((lane % 2) * 32)) as u32;
        let (v, f) = if step(w) {
            refinement_expected(a, b, fpcr)
        } else {
            estimate_expected(a, fpcr)
        };
        out[lane / 2] |= u64::from(v) << ((lane % 2) * 32);
        flags |= f;
    }
    (out, flags)
}
fn pairs() -> Vec<(u32, u32)> {
    let values = [
        0, 1, 0x7fffff, 0x800000, 0x20000000, 0x3f000000, 0x3f800000, 0x3f800001, 0x3fc00000,
        0x40000000, 0x40400000, 0x7e800000, 0x7f000000, 0x7f7fffff, 0x7f800000, 0x7fc12345,
        0x7f812345, 0xffc54321, 0xff854321, 0x80000000,
    ];
    let mut pairs = Vec::new();
    for a in values {
        for b in values {
            pairs.push((a, b));
            pairs.push((a ^ 0x80000000, b));
        }
    }
    for e in [0, 10, 30, 40, 60, 64, 80, 100] {
        pairs.push((((127 + e) << 23) | 1, ((127 + e) << 23) | 0x400000));
    }
    let mut state = 0x12345678u32;
    for _ in 0..256 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = state;
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        pairs.push((a, state));
    }
    pairs
}
#[test]
fn exact_refinement_oracle_known_answers_and_double_rounding_adversary() {
    for (a, b, expected) in [
        (0x3fc00000, 0x40000000, 0),
        (0x3f800000, 0x3f800000, 0x3f800000),
        (0, 0x7f800000, 0x3fc00000),
        (0x7f800000, 0, 0x3fc00000),
    ] {
        assert_eq!(refinement_expected(a, b, 0), (expected, 0));
    }
    assert_eq!(
        refinement_expected(0x3fc00000, 0x40000000, 2 << 22),
        (0x80000000, 0)
    );
    let a = 0x4e800001;
    let b = 0x4ec00000;
    let naive = ((3.0 - f64::from(f32::from_bits(a)) * f64::from(f32::from_bits(b))) * 0.5) as f32;
    assert_ne!(naive.to_bits(), refinement_expected(a, b, 0).0);
}
#[test]
fn rsqrt_native_matches_exact_and_midpoint_oracles() {
    let path = folder("rsqrt-known");
    let words = words(true);
    let source = code(&words);
    let mut estimate_values = BTreeSet::new();
    for e in 1..255u32 {
        for bucket in 0..256 {
            estimate_values.insert((e << 23) | (bucket << 15));
            estimate_values.insert((e << 23) | (bucket << 15) | 32767);
        }
    }
    for bit in 0..23 {
        estimate_values.insert(1 << bit);
        estimate_values.insert((1 << bit) - 1);
    }
    estimate_values.extend([0, 0x7fffff, 0x7f800000, 0x7fc12345, 0x7f812345, 0xff812345]);
    let pairs = pairs();
    let mut counts = (0, 0);
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        let mut observed = (0, 0);
        for &w in &words {
            let native = unsafe {
                library
                    .get::<Entry>(format!("rsqrt_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let inputs: Vec<_> = if step(w) {
                pairs
                    .iter()
                    .flat_map(|&(a, b)| {
                        (0..4)
                            .flat_map(move |r| (0..2).map(move |dn| (a, b, (r << 22) | (dn << 25))))
                    })
                    .collect()
            } else {
                estimate_values.iter().map(|&a| (a, 0, 0)).collect()
            };
            for (a, b, fpcr) in inputs {
                let before = frame(w, a, b);
                let initial = 0x80 | ((a & 1) << 27);
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    c: 1,
                    v: 1,
                    fpcr,
                    fpsr: initial,
                    fault_status: 77,
                    fault_address: 0x1234,
                    vregs: before,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|n| n as u64 + 123);
                let x = cpu.x;
                let (value, flags) = expected(w, &before, fpcr, initial);
                let mut after = before;
                after[(w & 31) as usize] = value;
                {
                    let mut attached = backend.attach(&mut cpu);
                    assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
                }
                assert_eq!(cpu.vregs, after, "{opt} {w:x} {a:x}/{b:x}/{fpcr:x}");
                assert_eq!(cpu.fpsr, flags, "{opt} flags {w:x} {a:x}/{b:x}/{fpcr:x}");
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
                    (99, 1, 0, 1, 1, fpcr, 77, 0x1234)
                );
                if step(w) {
                    observed.1 += 1;
                } else {
                    observed.0 += 1;
                }
            }
        }
        if counts == (0, 0) {
            counts = observed;
        } else {
            assert_eq!(counts, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"estimate_native_cases_per_optimization":counts.0,"refinement_native_cases_per_optimization":counts.1,"refinement_oracle_bits":576,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
thread_local! {static CALLS:Cell<u32>=const{Cell::new(0)};static FAIL_AT:Cell<u32>=const{Cell::new(0)};}
unsafe extern "C" fn staged_failure(
    operation: u32,
    bytes: u32,
    _: u32,
    _: u32,
    _: u64,
    _: u64,
    _: u64,
    result: *mut u64,
    status: *mut u32,
) -> i32 {
    let call = CALLS.with(|v| {
        v.set(v.get() + 1);
        v.get()
    });
    unsafe {
        *result = if operation == 11 {
            0x4008000000000000
        } else if bytes == 4 {
            0x3ff0000000000000
        } else {
            0x3f800000
        };
        *status = 31;
    }
    if FAIL_AT.with(|v| v.get() == call) {
        7
    } else {
        0
    }
}
#[test]
fn rsqrt_profile_refusal_and_each_callback_failure_are_transactional() {
    let path = folder("rsqrt-fault-known");
    let words = words(true);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for &w in &words {
            let native = unsafe {
                library
                    .get::<Entry>(format!("rsqrt_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (fpcr, missing) in [
                (1, false),
                (2, false),
                (1 << 8, false),
                (1 << 24, false),
                (1 << 26, false),
                (0, true),
            ] {
                let before = frame(w, 0x3f800000, 0x3f800000);
                let mut c = Cpu {
                    fpcr,
                    fpsr: 0x80,
                    vregs: before,
                    ..Cpu::default()
                };
                let rc = if missing {
                    unsafe { native(&mut c, 10) }
                } else {
                    let mut attached = backend.attach(&mut c);
                    unsafe { native(attached.cpu_mut(), 10) }
                };
                assert_eq!(rc, 3);
                assert_eq!(c.vregs, before);
                assert_eq!(c.fpsr, 0x80);
            }
            if step(w) {
                let lanes = if w & (1 << 30) != 0 { 4 } else { 2 };
                for fail in 1..=lanes * 4 {
                    CALLS.with(|v| v.set(0));
                    FAIL_AT.with(|v| v.set(fail));
                    let before = [[0x3f8000003f800000; 2]; 32];
                    let mut c = Cpu {
                        fpsr: 0x80,
                        vregs: before,
                        floating: Some(staged_failure),
                        ..Cpu::default()
                    };
                    assert_eq!(
                        unsafe { native(&mut c, 10) },
                        7,
                        "{opt} {w:x} callback {fail}"
                    );
                    assert_eq!(CALLS.with(|v| v.get()), fail);
                    assert_eq!(c.vregs, before);
                    assert_eq!(c.fpsr, 0x80);
                    assert_eq!(c.fault_status, 7);
                }
            }
        }
    }
}
#[test]
fn rsqrt_matches_synthetic_qemu() {
    let path = folder("rsqrt-reference");
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
    let pairs = pairs();
    let mut cases = Vec::new();
    for &w in &words {
        for &(a, b) in &pairs {
            for r in 0..4 {
                for dn in 0..2 {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame(w, a, b)[..4].try_into().unwrap(),
                        fpcr: (r << 22) | (dn << 25),
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
                    .get::<Entry>(format!("rsqrt_{:x}\0", c.word).as_bytes())
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
            let independent = expected(c.word, &before, c.fpcr, c.fpsr);
            {
                let mut attached = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(attached.cpu_mut(), 10) }, 0);
            }
            assert_eq!(
                (cpu.vregs[0], cpu.fpsr),
                independent,
                "independent {opt} {i}"
            );
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
