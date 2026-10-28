//! Original synthetic inputs only. In particular, changing a writable GOT slot
//! changes the real dynamic target: a source witness is not a terminal fact.
use mkw_arm64::terminal::Image;
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    GuestAddr, Memory, Permissions,
};
use std::os::unix::fs::PermissionsExt;
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
const ENTRY: u64 = 0x402000;
const PAGE: u64 = 0x404000;
const LR: u64 = 0x401100;
fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn thunk(veneers: usize, offset: u32) -> Vec<u32> {
    let mut w = vec![0x14000001; veneers];
    w.extend([
        0xd0000010,
        0xf9400211 | ((offset / 8) << 10),
        0x91000210 | (offset << 10),
        0xd61f0220,
    ]);
    w
}
fn immediate(words: &mut Vec<u32>, r: u32, v: u64) {
    words.push(0xd2800000 | (((v as u32) & 65535) << 5) | r);
    for i in 1..4 {
        words.push(0xf2800000 | ((i as u32) << 21) | ((((v >> (i * 16)) as u32) & 65535) << 5) | r);
    }
}
fn put64(b: &mut [u8], at: usize, v: u64) {
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn elf(veneers: usize, offset: u32, target: u64) -> Vec<u8> {
    let mut b = vec![0u8; 0x8010 + 32];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    b[16..18].copy_from_slice(&2u16.to_le_bytes());
    b[18..20].copy_from_slice(&183u16.to_le_bytes());
    b[20..24].copy_from_slice(&1u32.to_le_bytes());
    put64(&mut b, 24, 0x401000);
    put64(&mut b, 32, 64);
    b[52..54].copy_from_slice(&64u16.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&1u16.to_le_bytes());
    b[64..68].copy_from_slice(&1u32.to_le_bytes());
    b[68..72].copy_from_slice(&7u32.to_le_bytes());
    put64(&mut b, 80, 0x400000);
    put64(&mut b, 88, 0x400000);
    let len = b.len() as u64;
    put64(&mut b, 96, len);
    put64(&mut b, 104, len);
    put64(&mut b, 112, 4096);
    let mut main = Vec::new();
    for (r, v) in [(0, 55), (16, 66), (17, 77), (30, LR)] {
        immediate(&mut main, r, v);
    }
    let pc = 0x401000 + main.len() as u64 * 4;
    main.push(0x14000000 | (((ENTRY - pc) / 4) as u32));
    let mut capture = Vec::new();
    immediate(&mut capture, 20, 0x408010);
    capture.extend([0xf9000280, 0xf9000690, 0xf9000a91, 0xf9000e9e]);
    for (r, v) in [(0, 1), (1, 0x408010), (2, 32), (8, 64)] {
        immediate(&mut capture, r, v);
    }
    capture.push(0xd4000001);
    immediate(&mut capture, 0, 0);
    immediate(&mut capture, 8, 93);
    capture.push(0xd4000001);
    for (at, w) in [
        (0x1000, main),
        (0x1100, capture),
        (0x2000, thunk(veneers, offset)),
        (0x6000, vec![0xd2800160, 0xd65f03c0]),
        (0x7000, vec![0xd28002c0, 0xd65f03c0]),
    ] {
        let raw = bytes(&w);
        b[at..at + raw.len()].copy_from_slice(&raw);
    }
    put64(&mut b, 0x4000 + offset as usize, target);
    b
}
#[test]
fn exact_thunks_match_qemu_and_dynamic_slot_changes_do_not_prove_terminal_targets() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/import-thunks");
    std::fs::create_dir_all(&folder).unwrap();
    let mut cases = 0;
    for veneers in [0, 1, 8] {
        for offset in [0, 24, 4088] {
            let words = thunk(veneers, offset);
            let raw = bytes(&words);
            let image = Image::new(ENTRY, raw.clone()).unwrap();
            let witness = image.import_thunk(ENTRY, 8).unwrap();
            assert_eq!(witness.slot(), PAGE + u64::from(offset));
            assert!(image
                .analyze(ENTRY, raw.len(), &[])
                .unwrap()
                .no_return_fact()
                .is_none());
            let mut source = mkw_codegen::emit_exclusive_calls_v10(
                &mkw_arm64::lift_function_exclusive_v10(&raw, 0, 0).unwrap(),
                0,
            )
            .unwrap()
            .replace("mkw_guest_v10_budget", "stub");
            for (name, marker) in [("first", 11u32), ("second", 22)] {
                source += &mkw_codegen::emit_exclusive_calls_v10(
                    &mkw_arm64::lift_function_exclusive_v10(
                        &bytes(&[0xd2800000 | (marker << 5), 0xd65f03c0]),
                        0,
                        0,
                    )
                    .unwrap(),
                    0,
                )
                .unwrap()
                .replace("mkw_guest_v10_budget", name);
            }
            let src = folder.join(format!("case-{veneers}-{offset}.c"));
            std::fs::write(&src, source).unwrap();
            let mut references = Vec::new();
            for target in [0x406000, 0x407000] {
                let file = folder.join(format!("reference-{veneers}-{offset}-{target}.elf"));
                std::fs::write(&file, elf(veneers, offset, target)).unwrap();
                std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
                let result = Command::new("qemu-aarch64")
                    .arg(&file)
                    .output()
                    .expect("QEMU AArch64 is required for this semantic check");
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                assert_eq!(result.stdout.len(), 32);
                let values = result
                    .stdout
                    .chunks_exact(8)
                    .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
                    .collect::<Vec<_>>();
                assert_eq!(
                    values,
                    vec![
                        if target == 0x406000 { 11 } else { 22 },
                        witness.slot(),
                        target,
                        LR
                    ]
                );
                references.push(values);
                cases += 1;
            }
            for opt in ["-O0", "-O2"] {
                let output = folder.join(format!("case-{veneers}-{offset}{opt}.so"));
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
                    .arg(&src)
                    .arg("-o")
                    .arg(&output)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let code = unsafe { libloading::Library::new(output).unwrap() };
                let router = Router::new(
                    vec![
                        unsafe { Target::load(&code, b"stub\0", GuestAddr(ENTRY), ENTRY).unwrap() },
                        unsafe {
                            Target::load(&code, b"first\0", GuestAddr(0x406000), 0x406000).unwrap()
                        },
                        unsafe {
                            Target::load(&code, b"second\0", GuestAddr(0x407000), 0x407000).unwrap()
                        },
                    ],
                    8,
                )
                .unwrap();
                let mut memory = Memory::default();
                memory
                    .map(
                        GuestAddr(PAGE),
                        vec![0; 4096],
                        Permissions::RW,
                        "synthetic mutable slot",
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
                // Reuse the same memory/bridge/witness; mutate the slot between runs.
                for (index, target) in [0x406000u64, 0x407000, 0x406000].into_iter().enumerate() {
                    owned
                        .lock()
                        .unwrap()
                        .write(GuestAddr(witness.slot()), &target.to_le_bytes())
                        .unwrap();
                    for initial_budget in 0..=veneers as u64 + 8 {
                        let mut cpu = CpuV10::default();
                        for r in 0..31 {
                            cpu.base.x[r] = 100 + r as u64;
                        }
                        cpu.base.x[0] = 55;
                        cpu.base.x[16] = 66;
                        cpu.base.x[17] = 77;
                        cpu.base.x[30] = LR;
                        cpu.base.module_base = 0xdead0000;
                        let mut budget = initial_budget;
                        let status = {
                            let mut a = bridge.attach(&mut cpu).unwrap();
                            let mut r = Routed::attach(&mut a, &router).unwrap();
                            r.run(GuestAddr(ENTRY), &mut budget).unwrap()
                        };
                        let total = veneers as u64 + 6;
                        assert_eq!(status, if initial_budget < total { 2 } else { 0 });
                        assert_eq!(budget, initial_budget.saturating_sub(total));
                        assert_eq!(cpu.base.module_base, 0xdead0000);
                        for r in 1..31 {
                            if ![16, 17, 30].contains(&r) {
                                assert_eq!(cpu.base.x[r], 100 + r as u64);
                            }
                        }
                        let executed = initial_budget.min(total);
                        let n = veneers as u64;
                        assert_eq!(
                            cpu.base.x[16],
                            if executed <= n {
                                66
                            } else if executed < n + 3 {
                                PAGE
                            } else {
                                witness.slot()
                            }
                        );
                        assert_eq!(cpu.base.x[17], if executed < n + 2 { 77 } else { target });
                        assert_eq!(
                            cpu.base.x[0],
                            if executed < n + 5 {
                                55
                            } else if target == 0x406000 {
                                11
                            } else {
                                22
                            }
                        );
                        assert_eq!(cpu.base.x[30], LR);
                        if initial_budget >= total {
                            assert_eq!(
                                vec![
                                    cpu.base.x[0],
                                    cpu.base.x[16],
                                    cpu.base.x[17],
                                    cpu.base.x[30]
                                ],
                                references[index % 2]
                            );
                            assert_eq!(cpu.base.pc, LR);
                        } else {
                            let expected = if executed < n + 4 {
                                ENTRY + executed * 4
                            } else {
                                target + (executed - n - 4) * 4
                            };
                            assert_eq!(cpu.base.pc, expected);
                        }
                    }
                }
                for target in [0u64, 0x409000] {
                    owned
                        .lock()
                        .unwrap()
                        .write(GuestAddr(witness.slot()), &target.to_le_bytes())
                        .unwrap();
                    let mut cpu = CpuV10::default();
                    cpu.base.x[0] = 55;
                    cpu.base.x[30] = LR;
                    let mut budget = 64;
                    let status = {
                        let mut a = bridge.attach(&mut cpu).unwrap();
                        let mut r = Routed::attach(&mut a, &router).unwrap();
                        r.run(GuestAddr(ENTRY), &mut budget).unwrap()
                    };
                    assert_eq!(status, 3);
                    assert_eq!(budget, 64 - veneers as u64 - 4);
                    assert_eq!(
                        (
                            cpu.base.pc,
                            cpu.base.x[0],
                            cpu.base.x[16],
                            cpu.base.x[17],
                            cpu.base.x[30]
                        ),
                        (target, 55, witness.slot(), target, LR)
                    );
                }
            }
        }
    }
    std::fs::write(folder.join("receipt.json"),format!("{{\"qemu_cases\":{cases},\"fatal_ubsan\":true,\"native_optimizations\":[\"O0\",\"O2\"],\"mutable_slot_retargets\":true,\"game_code_executed\":false}}\n")).unwrap();
    assert_eq!(cases, 18);
}
