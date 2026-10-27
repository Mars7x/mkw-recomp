use mkw_runtime::{abi::Cpu, floating::FloatingBackend};
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod fp_native;
use fp_native::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;

fn words() -> Vec<u32> {
    let mut words = Vec::new();
    for arrangement in [0, 1 << 30, (1 << 30) | (1 << 22)] {
        for base in [
            0x0e22d420, 0x0ea2d420, 0x2e22dc20, 0x2e22fc20, 0x0e22cc20, 0x0e22f420, 0x0ea2f420,
            0x0e22c420, 0x0ea2c420, 0x2e22d420, 0x0ea2cc20, 0x0ea0f820, 0x2ea0f820, 0x2ea1f820,
            0x0e21d820, 0x2e21d820, 0x0ea1b820, 0x2ea1b820,
        ] {
            words.push(base | arrangement);
        }
    }
    for size in [0, 1 << 22] {
        for base in [0x5e21d820, 0x7e21d820, 0x5ea1b820, 0x7ea1b820] {
            words.push(base | size);
        }
    }
    for scalar in 0..2 {
        for q in 0..2 {
            for size in 0..2 {
                if (scalar == 1 && q == 0) || (scalar == 0 && q == 0 && size == 1) {
                    continue;
                }
                for lane in 0..(if size == 0 { 4 } else { 2 }) {
                    let index = if size == 0 {
                        ((lane & 1) << 21) | ((lane >> 1) << 11)
                    } else {
                        lane << 11
                    };
                    for base in [0x0f829020, 0x0f821020, 0x0f825020] {
                        words.push(base | (scalar << 28) | (q << 30) | (size << 22) | index);
                    }
                }
            }
        }
    }
    words.extend([0x7e30d820, 0x7e70d820]);
    let originals = words.clone();
    for w in originals {
        words.push(w & !(31 << 5)); // destination aliases first source
        let mkw_arm64::Op::VectorFp(v) = mkw_arm64::decode(w, 0).unwrap().op else {
            unreachable!()
        };
        if matches!(
            v.kind,
            mkw_arm64::vector_fp::VectorFpKind::Arithmetic(
                0 | 1 | 2 | 3 | 6 | 11 | 15 | 16 | 17 | 18
            )
        ) || (v.kind == mkw_arm64::vector_fp::VectorFpKind::PairwiseAdd && v.total > v.bytes)
        {
            words.push(w & !(31 << 16)); // destination aliases second source
            words.push(w & !((31 << 16) | (31 << 5)));
        }
    }
    words.sort_unstable();
    words.dedup();
    words
}
fn codes(words: impl IntoIterator<Item = u32>) -> String {
    words
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|w| generate(&[w, 0xd65f03c0], &format!("vector_{w:x}")))
        .collect()
}
// Callback failure after a successful first lane must never commit partial state.
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
        *status = fpsr | 1;
    }
    0
}
#[test]
fn known_lanes_overlap_upper_clearing_and_transactional_refusal() {
    let path = folder("vector-fp-known");
    let overlap = 0x4e22d421; // V1 = V1 + V2
    let high = 0x4e3fd7ff; // V31 = V31 + V31
    let code = codes(words().into_iter().chain([overlap, high]));
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (w, input, rhs, expected, flags) in [
            (
                0x4e21d820,
                [0xffffffff00000001, 0x800000007fffffff],
                [0, 0],
                [0xbf8000003f800000, 0xcf0000004f000000],
                16,
            ),
            (
                0x0e21d820,
                [0xffffffff00000001, u64::MAX],
                [0, 0],
                [0xbf8000003f800000, 0],
                0,
            ),
            (
                0x5e21d820,
                [0xffffffff00000001, u64::MAX],
                [0, 0],
                [0x3f800000, 0],
                0,
            ),
            (
                0x4ea1b820,
                [0xbfc000003fc00000, 0x7fc123457f800000],
                [0, 0],
                [0xffffffff00000001, 0x000000007fffffff],
                17,
            ),
            (
                0x4e22d420,
                [0x3f8000003f800000; 2],
                [0x4000000040000000; 2],
                [0x4040000040400000; 2],
                0,
            ),
            (
                overlap,
                [0x3f8000003f800000; 2],
                [0x4000000040000000; 2],
                [0x4040000040400000; 2],
                0,
            ),
            (
                high,
                [0x3f8000003f800000; 2],
                [0, 0],
                [0x4000000040000000; 2],
                0,
            ),
        ] {
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let dst = (w & 31) as usize;
            let lhs = ((w >> 5) & 31) as usize;
            let mut cpu = Cpu {
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.vregs[dst] = [123, 456];
            cpu.vregs[lhs] = input;
            cpu.vregs[2] = rhs;
            {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            assert_eq!(cpu.vregs[dst], expected, "{w:x}");
            assert_eq!(cpu.fpsr, 0x08000080 | flags, "{w:x}");
        }
        for w in words() {
            let mkw_arm64::Op::VectorFp(v) = mkw_arm64::decode(w, 0).unwrap().op else {
                unreachable!()
            };
            if v.lhs != 1
                || v.rhs != 2
                || v.kind != mkw_arm64::vector_fp::VectorFpKind::Arithmetic(2)
            {
                continue;
            }
            let Some(lane) = v.rhs_lane else { continue };
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu::default();
            cpu.vregs[0] = [123, 456];
            cpu.vregs[1] = if v.bytes == 4 {
                [0x3f8000003f800000; 2]
            } else {
                [0x3ff0000000000000; 2]
            };
            cpu.vregs[2] = if v.bytes == 4 {
                [0x4040000040000000, 0x40a0000040800000]
            } else {
                [0x4000000000000000, 0x4008000000000000]
            };
            let element = if v.bytes == 4 {
                [0x40000000u64, 0x40400000, 0x40800000, 0x40a00000][usize::from(lane)]
            } else {
                cpu.vregs[2][usize::from(lane)]
            };
            let mut expected = [0u64; 2];
            for n in 0..v.total / v.bytes {
                let offset = u32::from(n) * u32::from(v.bytes) * 8;
                expected[(offset / 64) as usize] |= element << (offset % 64);
            }
            {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            assert_eq!(cpu.vregs[0], expected, "indexed {w:x}");
            assert_eq!(cpu.fpsr, 0);
        }
        let fmls = unsafe { library.get::<Entry>(b"vector_4ea2cc20\0").unwrap() };
        let mut cpu = Cpu::default();
        cpu.vregs[0] = [0x4120000041200000; 2];
        cpu.vregs[1] = [0x4000000040000000; 2];
        cpu.vregs[2] = [0x4040000040400000; 2];
        {
            let mut a = backend.attach(&mut cpu);
            assert_eq!(unsafe { fmls(a.cpu_mut(), 10) }, 0);
        }
        assert_eq!(cpu.vregs[0], [0x4080000040800000; 2]);
        assert_eq!(cpu.fpsr, 0);
        for (w, left, right, expected, flags) in [
            (
                0x4e22c420,
                [0x000000007fc12345, 0x800000007f812345],
                [0x800000003f800000, 0x000000003f800000],
                [0x000000003f800000, 0x000000007fc12345],
                1,
            ),
            (
                0x4ea2c420,
                [0x000000007fc12345, 0x800000007f812345],
                [0x800000003f800000, 0x000000003f800000],
                [0x800000003f800000, 0x800000007fc12345],
                1,
            ),
            (
                0x4e22f420,
                [0x000000007fc12345, 0x800000007f812345],
                [0x800000003f800000, 0x000000003f800000],
                [0x000000007fc12345, 0x000000007fc12345],
                1,
            ),
            (
                0x4ea2f420,
                [0x000000007fc12345, 0x800000007f812345],
                [0x800000003f800000, 0x000000003f800000],
                [0x800000007fc12345, 0x800000007fc12345],
                1,
            ),
            (
                0x6e22d420,
                [0x400000003f800000, 0x4080000040400000],
                [0x40c0000040a00000, 0x4100000040e00000],
                [0x40e0000040400000, 0x4170000041300000],
                0,
            ),
            (
                0x2e22d420,
                [0x400000003f800000, 0x4080000040400000],
                [0x40c0000040a00000, 0x4100000040e00000],
                [0x4130000040400000, 0],
                0,
            ),
            (
                0x6e62d420,
                [0x3ff0000000000000, 0x4000000000000000],
                [0x4008000000000000, 0x4010000000000000],
                [0x4008000000000000, 0x401c000000000000],
                0,
            ),
            (
                0x7e30d820,
                [0x400000003f800000, 123],
                [0, 0],
                [0x40400000, 0],
                0,
            ),
            (
                0x7e70d820,
                [0x3ff0000000000000, 0x4000000000000000],
                [0, 0],
                [0x4008000000000000, 0],
                0,
            ),
            (
                0x6e22d420,
                [0x3f8000007fc12345, 0x3f8000007f812345],
                [0, 0],
                [0x7fc123457fc12345, 0],
                1,
            ),
        ] {
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.vregs[0] = [123, 456];
            cpu.vregs[1] = left;
            cpu.vregs[2] = right;
            {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            assert_eq!(cpu.vregs[0], expected, "numeric {w:x}");
            assert_eq!(cpu.fpsr, 0x08000080 | flags);
        }
        let pairwise = unsafe { library.get::<Entry>(b"vector_2e22d420\0").unwrap() };
        let mut cpu = Cpu {
            fpsr: 0x08000080,
            floating: Some(fail_second),
            ..Cpu::default()
        };
        cpu.vregs[0] = [123, 456];
        cpu.vregs[1] = [1, 0];
        cpu.vregs[2] = [2, 0];
        assert_eq!(unsafe { pairwise(&mut cpu, 10) }, 3);
        assert_eq!(cpu.vregs[0], [123, 456]);
        assert_eq!(cpu.fpsr, 0x08000080);
        let native = unsafe { library.get::<Entry>(b"vector_4e22d420\0").unwrap() };
        for mode in 0..3 {
            let mut cpu = Cpu {
                fpsr: 0x08000080,
                fpcr: if mode == 1 { 1 << 24 } else { 0 },
                ..Cpu::default()
            };
            cpu.vregs[0] = [123, 456];
            cpu.vregs[1] = [0x0000000200000001, 0];
            if mode == 0 {
                assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
            }
            if mode == 1 {
                let mut a = backend.attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 3);
            }
            if mode == 2 {
                cpu.floating = Some(fail_second);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
            }
            assert_eq!(cpu.vregs[0], [123, 456]);
            assert_eq!(cpu.fpsr, 0x08000080);
        }
    }
}
#[test]
fn vector_results_and_flags_match_qemu() {
    let path = folder("vector-fp-reference");
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
    for w in words() {
        let doubles = w & (1 << 22) != 0;
        let patterns: &[u64] = if doubles {
            &[
                0,
                0x8000000000000000,
                1,
                0x800fffffffffffff,
                0x3ff0000000000000,
                0xbff8000000000000,
                0x7fefffffffffffff,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8123456789abc,
                0x7ff0123456789abc,
                0xffffffffffffffff,
                0x8000000000000001,
                0x7fffffffffffffff,
            ]
        } else {
            &[
                0,
                0x8000000000000001,
                0x80000001007fffff,
                0x3f800000bf800000,
                0x3fc0000040000000,
                0x7f7fffffff7fffff,
                0x7f800000ff800000,
                0x7fc123457f812345,
                0xffc12345ff812345,
                0xffffffff80000000,
                0x7fffffff01000001,
                0x3f8000013f7fffff,
                0x0080000080800000,
                0x0000000080000000,
            ]
        };
        for i in 0..patterns.len() {
            for rounding in 0..4 {
                for dn in 0..2 {
                    let vectors = std::array::from_fn(|reg| {
                        std::array::from_fn(|half| {
                            patterns[(i + reg * 3 + half * 5) % patterns.len()]
                        })
                    });
                    cases.push(reference::VectorCase {
                        word: w,
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
    let code = codes(words());
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let fp_lib = backend(&path, opt);
        let backend = unsafe { FloatingBackend::load(&fp_lib) }.unwrap();
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_{:x}\0", c.word).as_bytes())
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
                "{opt} fixture {i} word {:08x} fpcr {:x}",
                c.word,
                c.fpcr
            );
            assert_eq!(cpu.vregs[1..4], c.vectors[1..4]);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
