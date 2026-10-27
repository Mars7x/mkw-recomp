use mkw_arm64::{simd::Simd, Op};
use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words() -> Vec<u32> {
    let mut out = BTreeSet::new();
    for q in 0..2 {
        for count in 1..5 {
            for extension in 0..2 {
                for (dst, base, index) in [
                    (0, 1, 2),
                    (0, 0, 0),
                    (0, 31, 0),
                    (31, 31, 31),
                    (1, 31, 1),
                    (2, 30, 31),
                    (30, 31, 0),
                    (0, 2, 31),
                ] {
                    out.insert(
                        0x0e000000
                            | (q << 30)
                            | ((count - 1) << 13)
                            | (extension << 12)
                            | (index << 16)
                            | (base << 5)
                            | dst,
                    );
                }
            }
        }
        for (dst, src) in [(0, 1), (0, 0), (31, 31), (31, 0)] {
            out.insert(0x0e205800 | (q << 30) | (src << 5) | dst);
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|w| generate(&[w, 0xd65f03c0], &format!("table_count_{w:x}")))
        .collect()
}
fn raw(v: [u64; 2]) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&v[0].to_le_bytes());
    out[8..].copy_from_slice(&v[1].to_le_bytes());
    out
}
fn pack(v: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(v[..8].try_into().unwrap()),
        u64::from_le_bytes(v[8..].try_into().unwrap()),
    ]
}
fn expected(op: Simd, before: &[[u64; 2]; 32]) -> (usize, [u64; 2]) {
    let mut output = [0; 16];
    let dst = match op {
        Simd::Table {
            dst,
            base,
            count,
            index,
            bytes,
            extension,
        } => {
            let table: Vec<_> = (0..count)
                .flat_map(|slot| raw(before[usize::from((base + slot) % 32)]))
                .collect();
            let indices = raw(before[usize::from(index)]);
            let old = raw(before[usize::from(dst)]);
            for i in 0..usize::from(bytes) {
                output[i] = table
                    .get(usize::from(indices[i]))
                    .copied()
                    .unwrap_or(if extension { old[i] } else { 0 });
            }
            dst
        }
        Simd::ByteCount { dst, src, bytes } => {
            let source = raw(before[usize::from(src)]);
            for i in 0..usize::from(bytes) {
                output[i] = source[i].count_ones() as u8;
            }
            dst
        }
        _ => unreachable!(),
    };
    (usize::from(dst), pack(output))
}
#[test]
fn table_lookup_and_popcount_match_independent_byte_sequences() {
    let path = folder("table-count-known");
    let words = words();
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for w in &words {
            let Op::Simd(op) = mkw_arm64::decode(*w, 0).unwrap().op else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("table_count_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for pattern in 0..256u16 {
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpcr: 0x02c00000,
                    fpsr: 0x0800009f,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                    *v = pack(std::array::from_fn(|byte| {
                        (reg as u8)
                            .wrapping_mul(17)
                            .wrapping_add(byte as u8)
                            .wrapping_add(pattern as u8)
                    }));
                }
                let input = pack(std::array::from_fn(|byte| {
                    (pattern as u8).wrapping_add((byte as u8).wrapping_mul(17))
                }));
                match op {
                    Simd::Table { index, .. } => cpu.vregs[usize::from(index)] = input,
                    Simd::ByteCount { src, .. } => cpu.vregs[usize::from(src)] = input,
                    _ => unreachable!(),
                }
                let before = cpu.vregs;
                let general = cpu.x;
                let (dst, expected) = expected(op, &before);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                for (reg, &old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == dst { expected } else { old },
                        "{opt} {w:x} pattern {pattern} v{reg}"
                    );
                }
                assert_eq!(cpu.x, general);
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x02c00000, 0x0800009f)
                );
            }
        }
    }
}
#[test]
fn table_and_count_encodings_match_synthetic_qemu() {
    let path = folder("table-count-reference");
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
    for q in 0..2 {
        for count in 1..5 {
            for extension in 0..2 {
                for base in [0, 31] {
                    if base == 0 && count == 4 {
                        continue;
                    }
                    for index in [0, 1, 2, 31] {
                        words.insert(
                            0x0e000000
                                | (q << 30)
                                | ((count - 1) << 13)
                                | (extension << 12)
                                | (index << 16)
                                | (base << 5),
                        );
                    }
                }
            }
        }
        for src in [0, 1, 2, 31] {
            words.insert(0x0e205800 | (q << 30) | (src << 5));
        }
    }
    let words: Vec<_> = words.into_iter().collect();
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, reference::elf(&words)).unwrap();
    use std::os::unix::fs::PermissionsExt;
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
    assert_eq!(output.stdout.len(), words.len() * 32);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for (i, w) in words.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("table_count_{w:x}\0").as_bytes())
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
            let actual: Vec<_> = [cpu.vregs[0][0], cpu.vregs[0][1], cpu.x[0], 0xb0000000]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect();
            assert_eq!(actual, &output.stdout[i * 32..(i + 1) * 32], "{opt} {w:x}");
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":words.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
