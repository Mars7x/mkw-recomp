use mkw_arm64::{
    vector_compare::{Compare, Relation},
    Op,
};
use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words() -> Vec<u32> {
    let mut out = BTreeSet::new();
    for (scalar, q, size) in [(0, 0, 0), (0, 1, 0), (0, 1, 1), (1, 1, 0), (1, 1, 1)] {
        let fields = (scalar << 28) | (q << 30) | (size << 22);
        for base in [0x0e20e400, 0x2e20e400, 0x2ea0e400, 0x2e20ec00, 0x2ea0ec00] {
            for (rd, rn, rm) in [(0, 1, 2), (0, 0, 2), (0, 1, 0), (0, 0, 0), (31, 31, 31)] {
                out.insert(base | fields | (rm << 16) | (rn << 5) | rd);
            }
        }
        for base in [0x0ea0d800, 0x2ea0c800, 0x0ea0c800, 0x2ea0d800, 0x0ea0e800] {
            for (rd, rn) in [(0, 1), (0, 0), (31, 31)] {
                out.insert(base | fields | (rn << 5) | rd);
            }
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("compare_{w:x}")))
        .collect()
}
fn decoded(w: u32) -> Compare {
    let Op::VectorCompare(v) = mkw_arm64::decode(w, 0).unwrap().op else {
        panic!()
    };
    v
}
fn oracle(op: Compare, input: &[[u64; 2]; 32]) -> ([u64; 2], u32) {
    let left = input[usize::from(op.lhs)];
    let right = op.rhs.map(|r| input[usize::from(r)]).unwrap_or([0, 0]);
    let mut out = [0; 2];
    let mut flags = 0;
    for lane in 0..op.total / op.bytes {
        let bits = op.bytes * 8;
        let offset = lane * bits;
        let mask = if op.bytes == 4 { 0xffffffff } else { u64::MAX };
        let mut a = (left[usize::from(offset / 64)] >> (offset % 64)) & mask;
        let mut b = (right[usize::from(offset / 64)] >> (offset % 64)) & mask;
        let (exponent, fraction, quiet) = if op.bytes == 4 {
            (0x7f800000, 0x007fffff, 0x00400000)
        } else {
            (0x7ff0000000000000, 0x000fffffffffffff, 0x0008000000000000)
        };
        let na = a & exponent == exponent && a & fraction != 0;
        let nb = b & exponent == exponent && b & fraction != 0;
        if na || nb {
            if op.relation != Relation::Equal || (na && a & quiet == 0) || (nb && b & quiet == 0) {
                flags |= 1;
            }
            continue;
        }
        if op.absolute {
            a &= mask >> 1;
            b &= mask >> 1;
        }
        // Host comparisons are an independent test oracle for ordered finite/infinite inputs.
        let ordering = if op.bytes == 4 {
            f32::from_bits(a as u32).partial_cmp(&f32::from_bits(b as u32))
        } else {
            f64::from_bits(a).partial_cmp(&f64::from_bits(b))
        }
        .unwrap();
        use std::cmp::Ordering::*;
        let pass = match op.relation {
            Relation::Equal => ordering == Equal,
            Relation::GreaterEqual => ordering != Less,
            Relation::Greater => ordering == Greater,
            Relation::LessEqual => ordering != Greater,
            Relation::Less => ordering == Less,
        };
        if pass {
            out[usize::from(offset / 64)] |= mask << (offset % 64);
        }
    }
    (out, flags)
}
unsafe extern "C" fn fail_second(
    _: u32,
    _: u32,
    _: u32,
    fpsr: u32,
    a: u64,
    _: u64,
    _: u64,
    result: *mut u64,
    status: *mut u32,
) -> i32 {
    if a == 2 {
        return 3;
    }
    unsafe {
        *result = 6;
        *status = fpsr | 1;
    }
    0
}
#[test]
fn comparison_masks_flags_aliasing_and_transactional_refusals() {
    let path = folder("comparison-known");
    let words = words();
    let code = codes(&words);
    let patterns = [
        0,
        0x8000000000000000,
        0x3ff000003f800000,
        0xbff00000bf800000,
        0x7ff812347fc12345,
        0x7ff012347f812345,
        0x7f800000ff800000,
        0x3ff0000000000000,
        0xbff0000000000000,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x8000000100000001,
        0x7ff8123456789abc,
        0x7ff0123456789abc,
    ];
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for w in &words {
            let op = decoded(*w);
            let native = unsafe {
                lib.get::<Entry>(format!("compare_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for i in 0..patterns.len() {
                for dn in 0..2 {
                    let mut cpu = Cpu {
                        fpcr: dn << 25,
                        fpsr: 0x08000080,
                        n: 1,
                        z: 0,
                        c: 1,
                        v: 1,
                        sp: 99,
                        ..Cpu::default()
                    };
                    for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                        *v = [
                            patterns[(i + reg) % patterns.len()],
                            patterns[(i + reg + 5) % patterns.len()],
                        ];
                    }
                    cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                    let general = cpu.x;
                    let before = cpu.vregs;
                    let (expected, flags) = oracle(op, &before);
                    {
                        let mut a = backend.attach(&mut cpu);
                        assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
                    }
                    for (reg, v) in before.iter().enumerate() {
                        assert_eq!(
                            cpu.vregs[reg],
                            if reg == usize::from(op.dst) {
                                expected
                            } else {
                                *v
                            },
                            "{opt} {w:x} pattern {i} DN{dn} v{reg}"
                        );
                    }
                    assert_eq!(cpu.x, general);
                    assert_eq!(cpu.fpsr, 0x08000080 | flags);
                    assert_eq!(
                        (cpu.n, cpu.z, cpu.c, cpu.v, cpu.sp, cpu.fpcr),
                        (1, 0, 1, 1, 99, dn << 25)
                    );
                }
            }
            for control in [0, 1 << 24, 1 << 8, 1 << 26] {
                let mut cpu = Cpu {
                    fpcr: control,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.vregs[usize::from(op.dst)] = [123, 456];
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
        let native = unsafe { lib.get::<Entry>(b"compare_6e22e420\0").unwrap() };
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
}
#[test]
fn comparison_results_and_flags_match_qemu() {
    let path = folder("comparison-reference");
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
    let words: Vec<_> = words().into_iter().filter(|w| w & 31 == 0).collect();
    let mut cases = Vec::new();
    for w in &words {
        let v = decoded(*w);
        let patterns: &[u64] = if v.bytes == 4 {
            &[
                0,
                0x8000000080000000,
                0x0000000080000000,
                0x8000000000000000,
                0x3f800000bf800000,
                0x400000003f800000,
                0x7f800000ff800000,
                0x7fc123457f812345,
                0xffc12345ff812345,
                0x8000000100000001,
                0x7f7fffffff7fffff,
                0x007fffff807fffff,
                0x0080000080800000,
                0x3f8000013f7fffff,
            ]
        } else {
            &[
                0,
                0x8000000000000000,
                1,
                0x800fffffffffffff,
                0x3ff0000000000000,
                0xbff0000000000000,
                0x4000000000000000,
                0x7fefffffffffffff,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8123456789abc,
                0x7ff0123456789abc,
                0xfff8123456789abc,
                0xfff0123456789abc,
            ]
        };
        for i in 0..patterns.len() {
            for rounding in 0..4 {
                for dn in 0..2 {
                    let vectors = std::array::from_fn(|r| {
                        std::array::from_fn(|h| patterns[(i + r * 3 + h * 5) % patterns.len()])
                    });
                    cases.push(reference::VectorCase {
                        word: *w,
                        vectors,
                        fpcr: (rounding << 22) | (dn << 25),
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
        let output = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .current_dir(&path)
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
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("compare_{:x}\0", c.word).as_bytes())
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
