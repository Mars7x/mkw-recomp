use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod fp_native;
use fp_native::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn word(q: u32, size: u32, kind: u32, part: u32, rd: u32, rn: u32, rm: u32) -> u32 {
    0x0e000800
        | (q << 30)
        | (size << 22)
        | ((kind + 1 + 4 * part) << 12)
        | (rm << 16)
        | (rn << 5)
        | rd
}
fn variants(registers: &[(u32, u32, u32)]) -> Vec<u32> {
    let mut words = Vec::new();
    for q in 0..2 {
        for size in 0..4 {
            if q == 0 && size == 3 {
                continue;
            }
            for kind in 0..3 {
                for part in 0..2 {
                    for &(rd, rn, rm) in registers {
                        words.push(word(q, size, kind, part, rd, rn, rm));
                    }
                }
            }
        }
    }
    words
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|w| generate(&[w, 0xd65f03c0], &format!("permutation_{w:x}")))
        .collect()
}
// Build lane lists and apply sequence operations independently of emitted bank/index formulas.
fn oracle(
    left: [u64; 2],
    right: [u64; 2],
    bytes: usize,
    total: usize,
    kind: u32,
    part: usize,
) -> [u64; 2] {
    let l: Vec<_> = left
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .take(total)
        .collect();
    let r: Vec<_> = right
        .into_iter()
        .flat_map(u64::to_le_bytes)
        .take(total)
        .collect();
    let l: Vec<_> = l.chunks(bytes).collect();
    let r: Vec<_> = r.chunks(bytes).collect();
    let lanes: Vec<&[u8]> = match kind {
        0 => l.into_iter().chain(r).skip(part).step_by(2).collect(),
        1 => l
            .chunks_exact(2)
            .zip(r.chunks_exact(2))
            .flat_map(|(a, b)| [a[part], b[part]])
            .collect(),
        2 => l
            .into_iter()
            .skip(part * total / bytes / 2)
            .take(total / bytes / 2)
            .zip(r.into_iter().skip(part * total / bytes / 2))
            .flat_map(|(a, b)| [a, b])
            .collect(),
        _ => unreachable!(),
    };
    let mut raw = [0u8; 16];
    for (i, b) in lanes.into_iter().flatten().enumerate() {
        raw[i] = *b;
    }
    [
        u64::from_le_bytes(raw[..8].try_into().unwrap()),
        u64::from_le_bytes(raw[8..].try_into().unwrap()),
    ]
}
#[test]
fn permutation_sequences_overlap_upper_clearing_and_state_preservation() {
    let path = folder("permutation-known");
    let words = variants(&[
        (0, 1, 2),
        (1, 1, 2),
        (2, 1, 2),
        (0, 0, 0),
        (31, 31, 31),
        (31, 1, 31),
    ]);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for w in &words {
            let rd = (w & 31) as usize;
            let rn = ((w >> 5) & 31) as usize;
            let rm = ((w >> 16) & 31) as usize;
            let size = (w >> 22) & 3;
            let total = if w & (1 << 30) != 0 { 16 } else { 8 };
            let kind = ((w >> 12) & 3) - 1;
            let part = ((w >> 14) & 1) as usize;
            let native = unsafe {
                library
                    .get::<Entry>(format!("permutation_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut random = 0xdea187c3456b9107u64;
            for pattern in 0..32 {
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
                for (reg, v) in cpu.vregs.iter_mut().enumerate() {
                    for (half, value) in v.iter_mut().enumerate() {
                        random ^= random << 13;
                        random ^= random >> 7;
                        random ^= random << 17;
                        *value = if pattern == 0 {
                            0x0706050403020100u64
                                .wrapping_add((reg as u64).wrapping_mul(0x1010101010101010))
                                .wrapping_add((half as u64) * 0x0808080808080808)
                        } else {
                            random
                        };
                    }
                }
                cpu.x[0] = u64::MAX;
                let before = cpu.vregs;
                let x = cpu.x;
                let expected = oracle(before[rn], before[rm], 1 << size, total, kind, part);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                for (reg, &value) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == rd { expected } else { value },
                        "{opt} {w:x} pattern {pattern} v{reg}"
                    );
                }
                assert_eq!(cpu.x, x);
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x02c00000, 0x0800009f)
                );
            }
        }
    }
}
#[test]
fn permutation_encodings_match_qemu() {
    let path = folder("permutation-reference");
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
    let words = variants(&[(0, 1, 2), (0, 0, 2), (0, 1, 0), (0, 0, 0), (0, 31, 31)]);
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
        let library = build(&code, &path, opt);
        for (i, w) in words.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("permutation_{w:x}\0").as_bytes())
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
