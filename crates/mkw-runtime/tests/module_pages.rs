use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, HeapSpec},
    module_pages::Plan,
    owned_memory_budget::OwnedMemoryBudget,
    GuestAddr, LoadedModule, Memory, Permissions,
};
use std::sync::{Arc, Mutex};
fn fixture(base: u64) -> LoadedModule {
    let mut raw = vec![0u8; 320];
    raw[..4].copy_from_slice(b"NSO0");
    for (i, size) in [9u32, 7, 17].iter().enumerate() {
        for (offset, value) in [
            (16 + i * 16, 256 + i as u32 * 20),
            (20 + i * 16, i as u32 * 0x2000),
            (24 + i * 16, *size),
        ] {
            raw[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    raw[0x3c..0x40].copy_from_slice(&4090u32.to_le_bytes());
    let mut nso = mkw_formats::Nso::parse(&raw).unwrap();
    for (i, segment) in nso.segments.iter_mut().enumerate() {
        segment.bytes.fill(0x21 + i as u8);
    }
    LoadedModule {
        name: "synthetic".into(),
        base: GuestAddr(base),
        nso,
    }
}
fn cfg() -> Config {
    Config {
        granule: 16,
        max_owners: 2,
    }
}
#[test]
fn payload_bss_padding_permissions_and_gaps_are_explicit() {
    let modules = [fixture(0x10000)];
    let plan = Plan::new(&modules).unwrap();
    assert_eq!((plan.mapped_bytes(), plan.region_count()), (16384, 3));
    let mut m = plan.materialize().unwrap();
    assert_eq!(m.read(GuestAddr(0x10000), 9).unwrap(), &[0x21; 9]);
    assert!(m
        .read(GuestAddr(0x10009), 4096 - 9)
        .unwrap()
        .iter()
        .all(|b| *b == 0));
    assert!(m.read(GuestAddr(0x11000), 1).is_err());
    assert_eq!(m.read(GuestAddr(0x12000), 7).unwrap(), &[0x22; 7]);
    assert!(m.write(GuestAddr(0x10009), &[1]).is_err());
    assert!(m.write(GuestAddr(0x12007), &[1]).is_err());
    assert_eq!(m.read(GuestAddr(0x14000), 17).unwrap(), &[0x23; 17]);
    assert!(m
        .read(GuestAddr(0x14011), 8192 - 17)
        .unwrap()
        .iter()
        .all(|b| *b == 0));
    m.write(GuestAddr(0x14ffc), &[0x77; 8]).unwrap(); // Crosses the shared data/BSS allocation's page boundary.
    assert_eq!(m.read(GuestAddr(0x14ffc), 8).unwrap(), &[0x77; 8]);
    assert!(m.read(GuestAddr(0x16000), 1).is_err());
    assert_eq!(
        plan.materialize()
            .unwrap()
            .read(GuestAddr(0x14ffc), 8)
            .unwrap(),
        &[0; 8]
    );
    let exact = mkw_runtime::map_modules(&modules).unwrap();
    assert!(exact.read(GuestAddr(0x10009), 1).is_err());
    assert!(exact.read(GuestAddr(0x14011 + 4090), 1).is_err());
}
#[test]
fn malformed_metadata_collisions_and_overflow_reject_before_materialization() {
    for mode in 0..9 {
        let mut m = fixture(0x10000);
        match mode {
            0 => m.base = GuestAddr(0x10001),
            1 => m.nso.segments[0].memory_offset = 1,
            2 => m.nso.segments[0].bytes.pop().map(|_| ()).unwrap(),
            3 => m.nso.segments[1].permissions = "RWX".into(),
            4 => m.nso.bss_size = usize::MAX,
            5 => m.nso.bss_size = mkw_formats::MAX_IMAGE,
            6 => m.nso.segments[1].memory_offset = 0,
            7 => m.base = GuestAddr(u64::MAX - 4095),
            _ => {
                m.nso.segments.pop();
            }
        }
        assert!(Plan::new(&[m]).is_err(), "mode {mode}");
    }
    assert!(Plan::new(&[fixture(0x10000), fixture(0x14000)]).is_err());
    // Empty images do not claim arbitrary pages.
    assert_eq!(Plan::new(&[]).unwrap().mapped_bytes(), 0);
}
#[test]
fn segment_and_bss_page_ownership_collisions_are_rejected() {
    let mut m = fixture(0x10000);
    m.nso.segments[0].bytes.resize(4097, 0);
    m.nso.segments[0].size = 4097;
    m.nso.segments[1].memory_offset = 4096;
    assert!(Plan::new(&[m]).is_err());
    // BSS is real ownership, including its final partial page.
    let mut m = fixture(0x10000);
    m.nso.segments[0].memory_offset = 0x5000;
    assert!(Plan::new(&[m]).is_err());
}
#[test]
fn execute_only_text_and_sealing_do_not_gain_read_or_write_access() {
    let mut m = fixture(0x10000);
    m.nso.segments[0].permissions = "--X".into();
    let modules = [m];
    let mut memory = Plan::new(&modules).unwrap().materialize().unwrap();
    assert!(memory.read(GuestAddr(0x10000), 1).is_err());
    assert!(memory.write(GuestAddr(0x10000), &[1]).is_err());
    memory.seal();
    assert!(memory
        .map(GuestAddr(0x30000), vec![0; 4096], Permissions::RW, "late")
        .is_err());
}
#[test]
fn page_accounting_and_heap_use_one_nonduplicated_data_bss_charge() {
    let modules = [fixture(0x1_0000_0000)];
    let plan = Plan::new(&modules).unwrap();
    let (memory, heap) = ExclusiveMemory::with_heap(
        plan.materialize().unwrap(),
        cfg(),
        HeapSpec {
            base: GuestAddr(0x2_0000_0000),
            capacity: 8192,
        },
    )
    .unwrap();
    let memory = Arc::new(Mutex::new(memory));
    let budget = OwnedMemoryBudget::new(memory.clone(), 24576).unwrap();
    assert_eq!(budget.snapshot().unwrap().used, 16384);
    memory.lock().unwrap().resize_heap(&heap, 4096).unwrap();
    assert_eq!(budget.snapshot().unwrap().used, 20480);
    assert_eq!(budget.snapshot().unwrap().committed_capacity, 24576);
    memory.lock().unwrap().resize_heap(&heap, 0).unwrap();
    assert_eq!(budget.snapshot().unwrap().used, 16384);
    let raw: Memory = mkw_runtime::map_modules(&modules).unwrap();
    assert!(OwnedMemoryBudget::new(
        Arc::new(Mutex::new(ExclusiveMemory::new(raw, cfg()).unwrap())),
        24576
    )
    .is_err());
}

#[test]
fn synthetic_native_accesses_respect_page_permissions_padding_and_budgets() {
    use mkw_runtime::{
        exclusive_calls::{Routed, Router, Target},
        exclusive_native::{Bridge, CpuV10},
        services::Services,
    };
    use std::{path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let raw: Vec<_> = [0xf9400002u32, 0xf9000001, 0xd65f03c0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let source =
        mkw_codegen::emit_services_v3(&mkw_arm64::lift_services_v3(&raw, 0, 0).unwrap(), 0)
            .unwrap();
    let dir = root.join("private/testing/module-pages");
    std::fs::create_dir_all(&dir).unwrap();
    for opt in ["-O0", "-O2"] {
        let src = dir.join(format!("synthetic{opt}.c"));
        let lib = src.with_extension("so");
        std::fs::write(&src, &source).unwrap();
        let out = Command::new("cc")
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
            .arg(root.join("native"))
            .arg(src)
            .arg("-o")
            .arg(&lib)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lib = unsafe { libloading::Library::new(lib).unwrap() };
        let target = unsafe {
            Target::load_services(&lib, b"mkw_guest_v10_budget\0", GuestAddr(0x80000), 0x80000)
                .unwrap()
        };
        let router = Router::new(vec![target], 1).unwrap();
        for at in [0x1400d, 0x14ffc, 0x11000, 0x12000] {
            for steps in 0..=4 {
                let modules = [fixture(0x10000)];
                let memory = Arc::new(Mutex::new(
                    ExclusiveMemory::new(
                        Plan::new(&modules).unwrap().materialize().unwrap(),
                        cfg(),
                    )
                    .unwrap(),
                ));
                let budget = OwnedMemoryBudget::new(memory.clone(), 16384).unwrap();
                let mut bridge = Bridge::new(memory.clone()).unwrap();
                let old = memory
                    .lock()
                    .unwrap()
                    .read(GuestAddr(at), mkw_runtime::exclusive::Width::Double)
                    .ok();
                let mut cpu = CpuV10::default();
                cpu.base.x[0] = at;
                cpu.base.x[1] = 0xabcdef1234567890;
                cpu.base.x[2] = 99;
                cpu.base.x[30] = 0x90000;
                let mut left = steps;
                let services = Services::new(vec![]).unwrap();
                let status = {
                    let mut a = bridge.attach(&mut cpu).unwrap();
                    let mut routed = Routed::attach_services(&mut a, &router, &services).unwrap();
                    routed.run(GuestAddr(0x80000), &mut left).unwrap()
                };
                let fault = (at == 0x11000 && steps >= 1) || (at == 0x12000 && steps >= 2);
                assert_eq!(
                    status,
                    if fault {
                        5
                    } else if steps < 3 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(
                    cpu.base.x[2],
                    if steps >= 1 {
                        old.map(|v| v as u64).unwrap_or(99)
                    } else {
                        99
                    }
                );
                if let Some(before) = old {
                    let after = memory
                        .lock()
                        .unwrap()
                        .read(GuestAddr(at), mkw_runtime::exclusive::Width::Double)
                        .unwrap();
                    assert_eq!(
                        after,
                        if steps >= 2 && at != 0x12000 {
                            0xabcdef1234567890
                        } else {
                            before
                        }
                    );
                }
                assert_eq!(budget.snapshot().unwrap().used, 16384);
            }
        }
    }
}
