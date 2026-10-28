//! Theorems retain premises; an ordinary legacy router DOES NOT discharge them.
//! Original synthetic code only. Existing QEMU thunk/transport suites remain required.
use mkw_arm64::terminal::{conditional::Effect, Image};
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
fn bytes(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|w| w.to_le_bytes()).collect()
}
#[test]
fn conditional_source_premises_match_native_loops_but_do_not_guard_a_mutated_target() {
    let caller = [0xaa1e03f3, 0x9400000f, 0xaa1303fe, 0xd65f03c0];
    let stub = [0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220];
    let closed = [0x14000000];
    let returning = [0xd28000e0, 0xd65f03c0];
    let mut text = vec![0xd503201fu32; 40];
    text[..4].copy_from_slice(&caller);
    text[16..20].copy_from_slice(&stub);
    text[32] = closed[0];
    text[36..38].copy_from_slice(&returning);
    let image = Image::new(0, bytes(&text)).unwrap();
    let local = image
        .analyze(0x80, 4, &[])
        .unwrap()
        .no_return_fact()
        .unwrap();
    let alias = image
        .conditional_import(
            &image.import_thunk(0x40, 0).unwrap(),
            &Effect::from_local(&local),
        )
        .unwrap();
    let conditional = image.analyze_conditionally(0, 16, &[alias]).unwrap();
    let theorem = conditional.no_return_effect().unwrap().unwrap();
    assert_eq!(theorem.requirements().len(), 1);
    assert!(theorem.local_source_fact().is_none());
    assert!(image
        .analyze(0, 16, &[])
        .unwrap()
        .no_return_fact()
        .is_none());
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/conditional-proof-native");
    std::fs::create_dir_all(&dir).unwrap();
    let mut source = String::new();
    for (name, words, at) in [
        ("caller", caller.as_slice(), 0),
        ("stub", stub.as_slice(), 0x40),
        ("closed", closed.as_slice(), 0x80),
        ("returning", returning.as_slice(), 0x90),
    ] {
        let ir = mkw_arm64::lift_function_exclusive_v10(&bytes(words), at, at).unwrap();
        source += &mkw_codegen::emit_exclusive_calls_v10(&ir, at)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let c = dir.join("original-synthetic.c");
    std::fs::write(&c, source).unwrap();
    for opt in ["-O0", "-O2"] {
        let file = dir.join(format!("original-synthetic{opt}.so"));
        let result = Command::new("cc")
            .args([
                "-std=c11",
                opt,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-shared",
                "-fPIC",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
                "-I",
            ])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
            .arg(&c)
            .arg("-o")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let code = unsafe { libloading::Library::new(file).unwrap() };
        let router = Router::new(
            [
                (b"caller\0".as_slice(), 0),
                (b"stub\0".as_slice(), 0x40),
                (b"closed\0".as_slice(), 0x80),
                (b"returning\0".as_slice(), 0x90),
            ]
            .into_iter()
            .map(|(name, offset)| unsafe {
                Target::load(&code, name, GuestAddr(0x400000 + offset), 0x400000).unwrap()
            })
            .collect(),
            8,
        )
        .unwrap();
        let mut memory = Memory::default();
        memory
            .map(
                GuestAddr(0x402000),
                vec![0; 64],
                Permissions::RW,
                "synthetic mutable import",
            )
            .unwrap();
        let owned = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                memory,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let mut bridge = Bridge::new(owned.clone()).unwrap();
        owned
            .lock()
            .unwrap()
            .write(GuestAddr(0x402018), &0x400080u64.to_le_bytes())
            .unwrap();
        for initial in 0..=12u64 {
            let mut cpu = CpuV10::default();
            cpu.base.x[30] = 0x500000;
            cpu.base.x[16] = 66;
            cpu.base.x[17] = 77;
            cpu.base.x[19] = 99;
            let mut budget = initial;
            let status = {
                let mut a = bridge.attach(&mut cpu).unwrap();
                let mut r = Routed::attach(&mut a, &router).unwrap();
                r.run(GuestAddr(0x400000), &mut budget).unwrap()
            };
            assert_eq!(status, 2);
            assert_eq!(budget, 0);
            let pc = match initial {
                0 => 0x400000,
                1 => 0x400004,
                2 => 0x400040,
                3 => 0x400044,
                4 => 0x400048,
                5 => 0x40004c,
                _ => 0x400080,
            };
            assert_eq!(cpu.base.pc, pc);
            assert_eq!(
                cpu.base.x[30],
                if initial < 2 { 0x500000 } else { 0x400008 }
            );
            assert_eq!(cpu.base.x[19], if initial < 1 { 99 } else { 0x500000 });
            assert_eq!(
                cpu.base.x[16],
                if initial < 3 {
                    66
                } else if initial < 5 {
                    0x402000
                } else {
                    0x402018
                }
            );
            assert_eq!(cpu.base.x[17], if initial < 4 { 77 } else { 0x400080 });
        }
        // Violating an undischarged premise changes real behavior. No guard is installed.
        owned
            .lock()
            .unwrap()
            .write(GuestAddr(0x402018), &0x400090u64.to_le_bytes())
            .unwrap();
        let mut cpu = CpuV10::default();
        cpu.base.x[30] = 0x500000;
        let mut budget = 30;
        let status = {
            let mut a = bridge.attach(&mut cpu).unwrap();
            let mut r = Routed::attach(&mut a, &router).unwrap();
            r.run(GuestAddr(0x400000), &mut budget).unwrap()
        };
        assert_eq!(
            (status, budget, cpu.base.x[0], cpu.base.pc),
            (0, 20, 7, 0x500000)
        );
        assert_eq!(theorem.requirements()[0].provider().entry, 0x80);
    }
}
