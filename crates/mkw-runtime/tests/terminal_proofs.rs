//! Original synthetic loop only: a proof allows diagnostic faults/budget exits,
//! and does not promise success, termination or original target authorization.
use mkw_arm64::terminal::Image;
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
#[test]
fn certified_loop_never_returns_normally_and_preserves_native_budget_memory_and_fault_semantics() {
    let words = [0xf9400020u32, 0x91000400, 0xf9000020, 0x17fffffd];
    let bytes = words
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let image = Image::new(0, bytes.clone()).unwrap();
    let analysis = image.analyze(0, bytes.len(), &[]).unwrap();
    let certificate = analysis.no_return_fact().unwrap();
    assert_eq!(certificate.source().bytes, 16);
    let ir = mkw_arm64::lift_function(&bytes, 0, 0).unwrap();
    assert_eq!(ir.len(), 4);
    let source = mkw_codegen::emit_services_v3(&ir, 0).unwrap();
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/terminal-proof-native");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("loop.c");
    std::fs::write(&src, source).unwrap();
    for opt in ["-O0", "-O2"] {
        let library = dir.join(format!("loop{opt}.so"));
        let build = Command::new("cc")
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
            .arg(&library)
            .output()
            .unwrap();
        std::fs::write(dir.join(format!("compiler{opt}.log")), &build.stderr).unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let code = unsafe { libloading::Library::new(library).unwrap() };
        let target = unsafe {
            Target::load_services(&code, b"mkw_guest_v10_budget\0", GuestAddr(0x4000), 0x4000)
                .unwrap()
        };
        let router = Router::new(vec![target], 8).unwrap();
        let services = Services::new(vec![]).unwrap();
        for initial in [0, u64::MAX - 1] {
            for budget in 0u64..=64 {
                let mut memory = Memory::default();
                memory
                    .map(
                        GuestAddr(0x1000),
                        initial.to_le_bytes().to_vec(),
                        Permissions::RW,
                        "original synthetic counter",
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
                let mut cpu = CpuV10::default();
                cpu.base.x[0] = 0x5555;
                cpu.base.x[1] = 0x1000;
                cpu.base.x[30] = 0x9000;
                let mut left = budget;
                {
                    let mut a = bridge.attach(&mut cpu).unwrap();
                    let mut r = Routed::attach_services(&mut a, &router, &services).unwrap();
                    assert_eq!(r.run(GuestAddr(0x4000), &mut left).unwrap(), 2);
                }
                let stored = initial.wrapping_add((budget + 1) / 4);
                assert_eq!(left, 0);
                assert_eq!(cpu.base.pc, 0x4000 + (budget % 4) * 4);
                assert_eq!(
                    owned
                        .lock()
                        .unwrap()
                        .read(GuestAddr(0x1000), Width::Double)
                        .unwrap(),
                    stored as u128
                );
                assert_eq!(
                    cpu.base.x[0],
                    if budget == 0 {
                        0x5555
                    } else {
                        stored.wrapping_add(u64::from(budget % 4 == 2))
                    }
                );
                assert_eq!(cpu.base.x[1], 0x1000);
            }
        }
        let mut memory = Memory::default();
        memory
            .map(
                GuestAddr(0x1000),
                vec![0; 8],
                Permissions::RW,
                "fault counter",
            )
            .unwrap();
        let mut bridge = Bridge::new(Arc::new(Mutex::new(
            ExclusiveMemory::new(
                memory,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        )))
        .unwrap();
        let mut cpu = CpuV10::default();
        cpu.base.x[0] = 77;
        cpu.base.x[1] = 0xdead0000;
        let mut budget = 10;
        {
            let mut a = bridge.attach(&mut cpu).unwrap();
            let mut r = Routed::attach_services(&mut a, &router, &services).unwrap();
            assert_eq!(r.run(GuestAddr(0x4000), &mut budget).unwrap(), 5);
        }
        assert_eq!((budget, cpu.base.pc, cpu.base.x[0]), (9, 0x4000, 77));
    }
}
