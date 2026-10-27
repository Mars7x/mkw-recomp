use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::{cell::Cell, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn step(_: u32) -> bool {
    true
}
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
            if full || d == 0 {
                out.push(0x0e20fc00 | (q << 30) | (m << 16) | (n << 5) | d);
            }
        }
    }
    out
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("reciprocal_step_{w:x}")))
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
// An independent fixed-point exact oracle: nine limbs at scale 2^-298.
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
        return (0x40000000, 0);
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
    let constant = Wide::shifted(2, 298);
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
    let mut exponent = high as i32 - 298;
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
        let (v, f) = refinement_expected(a, b, fpcr);
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
fn reciprocal_step_exact_oracle_known_answers_and_adversary() {
    for (a, b, value) in [
        (0x3f800000, 0x40000000, 0),
        (0x3f800000, 0x3f800000, 0x3f800000),
        (0, 0x7f800000, 0x40000000),
        (0x7f800000, 0, 0x40000000),
        (0x3fc00000, 0x40000000, 0xbf800000),
    ] {
        assert_eq!(refinement_expected(a, b, 0), (value, 0));
    }
    assert_eq!(
        refinement_expected(0x3f800000, 0x40000000, 2 << 22),
        (0x80000000, 0)
    );
    let (a, b) = (0x4e800001, 0x4ec00000);
    let naive = (2.0 - f64::from(f32::from_bits(a)) * f64::from(f32::from_bits(b))) as f32;
    assert_ne!(naive.to_bits(), refinement_expected(a, b, 0).0);
}
#[test]
fn reciprocal_step_native_matches_exact_576_bit_oracle() {
    let path = folder("reciprocal-step-known");
    let words = words(true);
    let source = code(&words);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let back = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&back) }.unwrap();
        let mut observed = 0;
        for &w in &words {
            let f = unsafe {
                lib.get::<Entry>(format!("reciprocal_step_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (a, b) in pairs() {
                for mode in 0..8 {
                    for initial in [0, 0x08000080] {
                        let fpcr = ((mode & 3) << 22) | ((mode >> 2) << 25);
                        let before = frame(w, a, b);
                        let mut want = before;
                        let (value, flags) = expected(w, &before, fpcr, initial);
                        want[(w & 31) as usize] = value;
                        let mut cpu = Cpu {
                            vregs: before,
                            fpcr,
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
                        {
                            let mut attached = backend.attach(&mut cpu);
                            assert_eq!(unsafe { f(attached.cpu_mut(), 10) }, 0);
                        }
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
                            (fpcr, 1, 0, 1, 1, 123, 77, 91)
                        );
                        observed += 1;
                    }
                }
            }
        }
        if count == 0 {
            count = observed;
        } else {
            assert_eq!(count, observed);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":count,"native_optimizations":["O0","O2"],"independent_576_bit_oracle":true,"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
thread_local! {static CALLS:Cell<u32>=const{Cell::new(0)};static FAIL:Cell<u32>=const{Cell::new(0)};}
unsafe extern "C" fn staged_failure(
    operation: u32,
    bytes: u32,
    _fpcr: u32,
    _fpsr: u32,
    _a: u64,
    _b: u64,
    accumulate: u64,
    result: *mut u64,
    flags: *mut u32,
) -> i32 {
    assert_eq!((operation, bytes, accumulate), (11, 4, 0x40000000));
    let n = CALLS.with(|v| {
        let n = v.get() + 1;
        v.set(n);
        n
    });
    unsafe {
        *result = 0xdead;
        *flags = 31;
    }
    if FAIL.with(|v| v.get()) == n {
        7
    } else {
        0
    }
}
#[test]
fn reciprocal_step_profiles_and_each_callback_fault_are_atomic() {
    let path = folder("reciprocal-step-failure");
    let words = words(true);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&source, &path, opt);
        let back = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&back) }.unwrap();
        for &w in &words {
            let f = unsafe {
                lib.get::<Entry>(format!("reciprocal_step_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let lanes = if w & (1 << 30) != 0 { 4 } else { 2 };
            for stage in 1..=lanes {
                CALLS.with(|v| v.set(0));
                FAIL.with(|v| v.set(stage));
                let mut cpu = Cpu {
                    vregs: frame(w, 0x3f800000, 0x40000000),
                    fpsr: 0x08000080,
                    floating: Some(staged_failure),
                    ..Cpu::default()
                };
                let before = cpu.vregs;
                assert_eq!(unsafe { f(&mut cpu, 10) }, 7);
                assert_eq!(CALLS.with(|v| v.get()), stage);
                assert_eq!(cpu.vregs, before);
                assert_eq!(cpu.fpsr, 0x08000080);
            }
            for missing in [true, false] {
                let mut cpu = Cpu {
                    vregs: frame(w, 0, 0x7f800000),
                    fpsr: 0x08000080,
                    fpcr: if missing { 0 } else { 1 << 24 },
                    ..Cpu::default()
                };
                let before = cpu.vregs;
                let result = if missing {
                    unsafe { f(&mut cpu, 10) }
                } else {
                    let mut attached = backend.attach(&mut cpu);
                    unsafe { f(attached.cpu_mut(), 10) }
                };
                assert_eq!(result, 3);
                assert_eq!(cpu.vregs, before);
                assert_eq!(cpu.fpsr, 0x08000080);
            }
        }
    }
}
#[test]
fn reciprocal_step_matches_synthetic_qemu() {
    let path = folder("reciprocal-step-reference");
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
        for (a, b) in pairs() {
            for mode in 0..8 {
                for fpsr in [0, 0x08000080] {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame(w, a, b)[..4].try_into().unwrap(),
                        fpcr: ((mode & 3) << 22) | ((mode >> 2) << 25),
                        fpsr,
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
        let back = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&back) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let f = unsafe {
                lib.get::<Entry>(format!("reciprocal_step_{:x}\0", c.word).as_bytes())
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
            let (value, flags) = expected(c.word, &want, c.fpcr, c.fpsr);
            want[0] = value;
            {
                let mut attached = backend.attach(&mut cpu);
                assert_eq!(unsafe { f(attached.cpu_mut(), 10) }, 0);
            }
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
