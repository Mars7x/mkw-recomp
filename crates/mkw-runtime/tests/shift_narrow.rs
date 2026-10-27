use mkw_arm64::{
    simd_shift::{Kind, Shift},
    Op,
};
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words() -> Vec<u32> {
    let mut words = BTreeSet::new();
    for q in 0..2 {
        for bits in [8u32, 16, 32] {
            for amount in 1..=bits {
                for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 1)] {
                    words.insert(
                        0x0f008400 | (q << 30) | ((2 * bits - amount) << 16) | (src << 5) | dst,
                    );
                }
            }
        }
    }
    words.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("shift_narrow_{w:x}")))
        .collect()
}
fn raw(v: [u64; 2]) -> [u8; 16] {
    let mut b = [0; 16];
    b[..8].copy_from_slice(&v[0].to_le_bytes());
    b[8..].copy_from_slice(&v[1].to_le_bytes());
    b
}
fn pack(b: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(b[..8].try_into().unwrap()),
        u64::from_le_bytes(b[8..].try_into().unwrap()),
    ]
}
#[test]
fn shift_narrow_byte_oracles_overlap_upper_clearing_and_state() {
    let path = folder("shift-narrow-known");
    let words = words();
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for w in &words {
            let Op::SimdShift(Shift {
                dst,
                src,
                bytes,
                upper,
                amount,
                kind: Kind::Narrow,
                ..
            }) = mkw_arm64::decode(*w, 0).unwrap().op
            else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("shift_narrow_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for pattern in 0..256u16 {
                let initial_fpsr = if pattern % 2 == 0 {
                    0x0800009f
                } else {
                    0x0000009f
                };
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpcr: 0x02c00000,
                    fpsr: initial_fpsr,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                    *v = pack(std::array::from_fn(|byte| {
                        (pattern as u8)
                            .wrapping_add((byte as u8).wrapping_mul(17))
                            .wrapping_add((reg as u8).wrapping_mul(31))
                    }));
                }
                let before = cpu.vregs;
                let general = cpu.x;
                let input = raw(before[usize::from(src)]);
                let mut expected = if upper {
                    raw(before[usize::from(dst)])
                } else {
                    [0; 16]
                };
                let b = usize::from(bytes);
                let start = if upper { 8 } else { 0 };
                for lane in 0..8 / b {
                    let wide = input[lane * b * 2..(lane + 1) * b * 2]
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| u128::from(v) << (i * 8))
                        .sum::<u128>();
                    let shifted = wide / (1u128 << amount);
                    for byte in 0..b {
                        expected[start + lane * b + byte] = (shifted >> (byte * 8)) as u8;
                    }
                }
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                let expected = pack(expected);
                for (reg, &old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == usize::from(dst) {
                            expected
                        } else {
                            old
                        },
                        "{opt} {w:x} pattern{pattern} v{reg}"
                    );
                }
                assert_eq!(cpu.x, general);
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x02c00000, initial_fpsr)
                );
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"cases_per_optimization":words.len()*256,"native_optimizations":["O0","O2"],"fatal_ubsan":true})).unwrap()).unwrap();
}
#[test]
fn shift_narrow_encodings_match_synthetic_qemu() {
    let path = folder("shift-narrow-reference");
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
    let mut words = BTreeSet::new();
    for w in self::words().into_iter().filter(|w| w & 31 == 0) {
        for src in [0, 1, 2, 31] {
            words.insert((w & !0x3e0) | (src << 5));
        }
    }
    let words: Vec<_> = words.into_iter().collect();
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, reference::elf(&words)).unwrap();
    use std::os::unix::fs::PermissionsExt;
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
    assert_eq!(out.stdout.len(), words.len() * 32);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for (i, w) in words.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("shift_narrow_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                sp: 99,
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.x[..3].copy_from_slice(&reference::INITIAL_X);
            cpu.vregs[..3].copy_from_slice(&reference::INITIAL_V[..3]);
            cpu.vregs[31] = reference::INITIAL_V[3];
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
            let actual: Vec<_> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                cpu.x[0],
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(actual, &out.stdout[i * 32..(i + 1) * 32], "{opt} {w:x}");
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":words.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
