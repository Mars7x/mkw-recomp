use mkw_runtime::abi::Cpu;
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
fn words() -> Vec<u32> {
    let mut out = BTreeSet::new();
    for hint in 0..32 {
        for rn in [1] {
            for imm in [0, 1, 4095] {
                out.insert(0xf9800000 | (imm << 10) | (rn << 5) | hint);
            }
            for imm in [0, 1, 255, 256, 511] {
                out.insert(0xf8800000 | (imm << 12) | (rn << 5) | hint);
            }
            for option in [2, 3, 6, 7] {
                for shift in 0..2 {
                    for rm in [2, 31] {
                        out.insert(
                            0xf8a00800
                                | (rm << 16)
                                | (option << 13)
                                | (shift << 12)
                                | (rn << 5)
                                | hint,
                        );
                    }
                }
            }
        }
        for imm in [0, 1, 0x3ffff, 0x40000, 0x7ffff] {
            out.insert(0xd8000000 | (imm << 5) | hint);
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("prefetch_{w:x}")))
        .collect()
}
unsafe extern "C" fn probe(
    context: *mut std::ffi::c_void,
    _write: i32,
    _address: u64,
    _value: *mut u64,
    _bytes: u32,
) -> i32 {
    unsafe {
        *context.cast::<u32>() += 1;
    }
    99
}
#[test]
fn prefetch_no_cache_effect_profile_preserves_state_budget_and_faults() {
    let path = folder("prefetch-known");
    let words = words();
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for &word in &words {
            let native = unsafe {
                library
                    .get::<BudgetEntry>(format!("prefetch_{word:x}_budget\0").as_bytes())
                    .unwrap()
            };
            for address in [0, 0x1001, u64::MAX] {
                for initial in 0..3u64 {
                    let mut calls = 0u32;
                    let mut cpu = Cpu {
                        sp: address,
                        module_base: 0x2000,
                        n: 1,
                        z: 0,
                        c: 1,
                        v: 1,
                        fpcr: 0x02c00000,
                        fpsr: 0x0800009f,
                        fault_status: 77,
                        fault_address: 0x55443322,
                        memory: Some(probe),
                        memory_context: std::ptr::addr_of_mut!(calls).cast(),
                        ..Cpu::default()
                    };
                    cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                    cpu.x[1] = address;
                    cpu.x[2] = 0xffffffff00000001;
                    cpu.x[30] = 0x5000;
                    cpu.vregs = std::array::from_fn(|r| [r as u64 + 123, r as u64 + 456]);
                    let x = cpu.x;
                    let vectors = cpu.vregs;
                    let mut remaining = initial;
                    assert_eq!(
                        unsafe { native(&mut cpu, &mut remaining) },
                        if initial < 2 { 2 } else { 0 },
                        "{opt} {word:x} {address:x}"
                    );
                    assert_eq!(remaining, 0);
                    assert_eq!(
                        cpu.pc,
                        match initial {
                            0 => 0x2000,
                            1 => 0x2004,
                            _ => 0x5000,
                        }
                    );
                    assert_eq!(cpu.x, x);
                    assert_eq!(cpu.vregs, vectors);
                    assert_eq!(calls, 0);
                    assert_eq!(
                        (
                            cpu.sp,
                            cpu.module_base,
                            cpu.n,
                            cpu.z,
                            cpu.c,
                            cpu.v,
                            cpu.fpcr,
                            cpu.fpsr,
                            cpu.fault_status,
                            cpu.fault_address
                        ),
                        (address, 0x2000, 1, 0, 1, 1, 0x02c00000, 0x0800009f, 77, 0x55443322)
                    );
                }
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"native_cases_per_optimization":words.len()*9,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"cache_profile":"no effect; SP/access trap policy remains unmodeled","game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn prefetch_matches_synthetic_qemu_observable_state() {
    let path = folder("prefetch-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            serde_json::to_vec(&serde_json::json!({"qemu_reference":"unavailable"})).unwrap(),
        )
        .unwrap();
        return;
    }
    let words = words();
    let cases: Vec<_> = words
        .iter()
        .enumerate()
        .map(|(i, &word)| reference::VectorCase {
            word,
            vectors: std::array::from_fn(|r| {
                [
                    0x0123456789abcdefu64.wrapping_add(i as u64 + r as u64),
                    0xfedcba9876543210,
                ]
            }),
            fpcr: 0x02c00000,
            fpsr: 0x9f | (((i as u32) & 1) << 27),
            nzcv: ((i as u32) & 15) << 28,
        })
        .collect();
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (i, chunk) in cases.chunks(1500).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{i}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
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
    assert_eq!(oracle.len(), cases.len() * 32);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("prefetch_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: (c.nzcv >> 31) & 1,
                z: (c.nzcv >> 30) & 1,
                c: (c.nzcv >> 29) & 1,
                v: (c.nzcv >> 28) & 1,
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 2) }, 0);
            let actual: Vec<_> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                u64::from(cpu.fpsr),
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(actual, &oracle[i * 32..(i + 1) * 32], "{opt} {:x}", c.word);
            assert_eq!(cpu.vregs, before);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_cache_or_trap_equivalence":false,"game_code_executed":false})).unwrap()).unwrap();
}
