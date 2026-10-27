use mkw_runtime::abi::Cpu;
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
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
fn yield_preserves_state_and_consumes_one_shared_budget() {
    let path = folder("yield-hint-known");
    let code = generate(&[0xd503203f, 0xd65f03c0], "yield_known");
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let native = unsafe { library.get::<BudgetEntry>(b"yield_known_budget\0").unwrap() };
        for initial in 0..4u64 {
            let mut calls = 0u32;
            let mut cpu = Cpu {
                sp: 0x3000,
                module_base: 0x2000,
                n: 1,
                z: 0,
                c: 1,
                v: 0,
                fpcr: 0x02c00000,
                fpsr: 0x0800009f,
                memory: Some(probe),
                memory_context: std::ptr::addr_of_mut!(calls).cast(),
                ..Cpu::default()
            };
            cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
            cpu.x[30] = 0x5000;
            cpu.vregs = std::array::from_fn(|r| [r as u64 + 123, r as u64 + 456]);
            let x = cpu.x;
            let vectors = cpu.vregs;
            let mut remaining = initial;
            assert_eq!(
                unsafe { native(&mut cpu, &mut remaining) },
                if initial < 2 { 2 } else { 0 }
            );
            assert_eq!(remaining, initial.saturating_sub(2));
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
            assert_eq!(
                (
                    cpu.sp,
                    cpu.module_base,
                    cpu.n,
                    cpu.z,
                    cpu.c,
                    cpu.v,
                    cpu.fpcr,
                    cpu.fpsr
                ),
                (0x3000, 0x2000, 1, 0, 1, 0, 0x02c00000, 0x0800009f)
            );
            assert_eq!(calls, 0);
            assert_eq!((cpu.fault_status, cpu.fault_address), (0, 0));
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":4,"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn yield_matches_synthetic_qemu_state() {
    let path = folder("yield-hint-reference");
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
    let cases: Vec<_> = (0..16u64)
        .map(|n| reference::VectorCase {
            word: 0xd503203f,
            vectors: std::array::from_fn(|r| {
                [
                    n.wrapping_mul(0x1122334455667788).wrapping_add(r as u64),
                    !n,
                ]
            }),
            fpcr: 0x02c00000,
            fpsr: 0x9f | if n & 1 != 0 { 1 << 27 } else { 0 },
            nzcv: (n as u32) << 28,
        })
        .collect();
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, reference::vector_elf(&cases)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .current_dir(&path)
        .arg(&image)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout.len(), cases.len() * 32);
    let code = generate(&[0xd503203f, 0xd65f03c0], "yield_reference");
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let native = unsafe { library.get::<Entry>(b"yield_reference\0").unwrap() };
        for (i, c) in cases.iter().enumerate() {
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
            assert_eq!(actual, &out.stdout[i * 32..(i + 1) * 32]);
            assert_eq!(cpu.vregs, before);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":16,"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
