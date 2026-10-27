use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod fp_native;
use fp_native::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn word(size: u32, c: bool, rd: u32, rn: u32, rm: u32) -> u32 {
    0x1ac04000
        | (u32::from(size == 3) << 31)
        | (size << 10)
        | (u32::from(c) << 12)
        | (rm << 16)
        | (rn << 5)
        | rd
}
// Polynomial long division with bit reversal; independent of the emitted reflected recurrence.
fn oracle(seed: u32, data: u64, bytes: u32, castagnoli: bool) -> u32 {
    let bits = bytes * 8;
    let reversed = data.reverse_bits() >> (64 - bits);
    let mut dividend = (u128::from(seed.reverse_bits()) << bits) ^ (u128::from(reversed) << 32);
    let polynomial = (1u128 << 32) | if castagnoli { 0x1edc6f41 } else { 0x04c11db7 };
    for bit in (32..bits + 32).rev() {
        if dividend & (1u128 << bit) != 0 {
            dividend ^= polynomial << (bit - 32);
        }
    }
    (dividend as u32).reverse_bits()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|w| generate(&[w, 0xd65f03c0], &format!("crc_{w:x}")))
        .collect()
}
#[test]
fn crc_matches_polynomial_division_and_known_strings_preserving_state() {
    let path = folder("crc-known");
    let mut words = Vec::new();
    let registers = [
        (0, 1, 2),
        (1, 1, 2),
        (2, 1, 2),
        (0, 0, 0),
        (0, 31, 31),
        (31, 1, 2),
    ];
    for size in 0..4 {
        for c in [false, true] {
            for (rd, rn, rm) in registers {
                words.push(word(size, c, rd, rn, rm));
            }
        }
    }
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for &w in &words {
            let size = (w >> 10) & 3;
            let c = w & (1 << 12) != 0;
            let (rd, rn, rm) = (
                (w & 31) as usize,
                ((w >> 5) & 31) as usize,
                ((w >> 16) & 31) as usize,
            );
            let native = unsafe {
                library
                    .get::<Entry>(format!("crc_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut random = 0x47be0123bafed001u64;
            for i in 0..160 {
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.vregs[0] = [123, 456];
                for r in 0..31 {
                    random ^= random << 13;
                    random ^= random >> 7;
                    random ^= random << 17;
                    cpu.x[r] = if i < 8 {
                        [
                            0,
                            u64::MAX,
                            1,
                            1 << 31,
                            1 << 63,
                            0xdeadbeef00000000,
                            0x0102030405060708,
                            0xfedcba9876543210,
                        ][(i + r) % 8]
                    } else {
                        random
                    };
                }
                let left = if rn == 31 { 0 } else { cpu.x[rn] };
                let right = if rm == 31 { 0 } else { cpu.x[rm] };
                let expected = u64::from(oracle(left as u32, right, 1 << size, c));
                let before = cpu.x;
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                for (reg, &old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.x[reg],
                        if reg == rd { expected } else { old },
                        "{w:x} fixture {i} register {reg}"
                    );
                }
                assert_eq!(
                    (cpu.sp, cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpsr),
                    (99, 1, 0, 1, 1, 0x08000080)
                );
                assert_eq!(cpu.vregs[0], [123, 456]);
            }
        }
        for (c, expected) in [(false, 0xcbf43926u32), (true, 0xe3069283)] {
            let w = word(0, c, 0, 1, 2);
            let native = unsafe {
                library
                    .get::<Entry>(format!("crc_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let mut state = u32::MAX;
            for byte in b"123456789" {
                let mut cpu = Cpu::default();
                cpu.x[1] = u64::from(state);
                cpu.x[2] = u64::from(*byte);
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                state = cpu.x[0] as u32;
            }
            assert_eq!(!state, expected);
        }
    }
}
#[test]
fn crc_encoding_variants_match_qemu() {
    let path = folder("crc-reference");
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
    let mut words = Vec::new();
    for size in 0..4 {
        for c in [false, true] {
            for (rd, rn, rm) in [(0, 1, 2), (0, 0, 0), (0, 31, 2), (0, 1, 31), (31, 1, 2)] {
                words.push(word(size, c, rd, rn, rm));
            }
        }
    }
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
                    .get::<Entry>(format!("crc_{w:x}\0").as_bytes())
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
            cpu.vregs[0] = reference::INITIAL_V[0];
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
