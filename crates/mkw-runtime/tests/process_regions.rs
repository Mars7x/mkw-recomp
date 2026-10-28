use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Heap, HeapSpec, Width},
    process_regions::{Error, Layout, ProcessRegions, Span},
    GuestAddr, Memory, Permissions,
};
use std::sync::{Arc, Mutex};
fn span(base: u64, size: u64) -> Span {
    Span {
        base: GuestAddr(base),
        size,
    }
}
fn layout() -> Layout {
    Layout {
        address_space: span(0, 0x10_0000_0000),
        code: span(0x10000, 0x10000),
        heap: span(0x1_0000_0000, 8192),
        stack: span(0x2_0000_0000, 0x10000),
        alias: span(0x3_0000_0000, 0x100000),
        remote: span(0x4_0000_0000, 0x100000),
    }
}
fn cfg() -> Config {
    Config {
        granule: 16,
        max_owners: 1,
    }
}
fn fixture(extra: Option<(u64, usize, Permissions)>) -> (Arc<Mutex<ExclusiveMemory>>, Heap) {
    let mut m = Memory::default();
    m.map(GuestAddr(0x10000), vec![0; 4096], Permissions::RX, "code")
        .unwrap();
    m.map(
        GuestAddr(0x2_0000_1000),
        vec![0; 4096],
        Permissions::RW,
        "initial stack",
    )
    .unwrap();
    if let Some((base, size, p)) = extra {
        m.map(GuestAddr(base), vec![0; size], p, "extra").unwrap();
    }
    let (m, h) = ExclusiveMemory::with_heap(
        m,
        cfg(),
        HeapSpec {
            base: GuestAddr(0x1_0000_0000),
            capacity: 8192,
        },
    )
    .unwrap();
    (Arc::new(Mutex::new(m)), h)
}
#[test]
fn bounds_are_distinct_from_commitment_and_preserve_owner_monitors() {
    let (m, h) = fixture(None);
    let regions = ProcessRegions::new(m.clone(), &h, layout()).unwrap();
    let owner = m.lock().unwrap().new_owner().unwrap();
    for size in [0, 4096, 8192, 4096, 0] {
        m.lock().unwrap().resize_heap(&h, size).unwrap();
        m.lock()
            .unwrap()
            .load(&owner, GuestAddr(0x2_0000_1000), Width::Double)
            .unwrap();
        assert_eq!(regions.snapshot().unwrap(), layout());
        assert!(m
            .lock()
            .unwrap()
            .store_conditional(&owner, GuestAddr(0x2_0000_1000), Width::Double, 7)
            .unwrap());
        assert!(m
            .lock()
            .unwrap()
            .read(GuestAddr(0x3_0000_0000), Width::Byte)
            .is_err());
        assert!(m
            .lock()
            .unwrap()
            .read(GuestAddr(0x4_0000_0000), Width::Byte)
            .is_err());
    }
    assert!(m.lock().unwrap().new_owner().is_err());
}
#[test]
fn invalid_and_overlapping_reservations_are_rejected() {
    let (m, h) = fixture(None);
    for mode in 0..12 {
        let mut l = layout();
        match mode {
            0 => l.code.size = 0,
            1 => l.code.base = GuestAddr(1),
            2 => l.stack.size = 4097,
            3 => l.remote.base = GuestAddr(u64::MAX - 4095),
            4 => l.remote = l.alias,
            5 => l.address_space.size = 0x4_0000_0000,
            6 => l.heap.size = 4096,
            7 => l.heap.base = GuestAddr(0x1_0000_1000),
            8 => l.stack = l.code,
            9 => l.address_space.base = GuestAddr(0x20000),
            10 => l.address_space.size = u64::MAX,
            _ => l.code.size = 0x2_0000_0000,
        }
        assert!(
            matches!(
                ProcessRegions::new(m.clone(), &h, l),
                Err(Error::Configuration)
            ),
            "mode {mode}"
        );
    }
}
#[test]
fn every_fixed_page_must_fit_permitted_containers() {
    for (base, size, p) in [
        (0x3_0000_0000, 4096, Permissions::RW),
        (0x4_0000_0000, 4096, Permissions::RW),
        (0x50000, 4096, Permissions::R),
        (0x1f000, 8192, Permissions::RW),
        (0x2_0000_2000, 4096, Permissions::RX),
        (0x18000, 4095, Permissions::RW),
    ] {
        let (m, h) = fixture(Some((base, size, p)));
        assert!(matches!(
            ProcessRegions::new(m, &h, layout()),
            Err(Error::Configuration)
        ));
    }
    let (m, h) = fixture(Some((0x18000, 4096, Permissions::RW)));
    assert!(ProcessRegions::new(m, &h, layout()).is_ok());
}
#[test]
fn foreign_heap_generations_replacement_and_poison_do_not_rebind() {
    let (m, h) = fixture(None);
    let (_, foreign) = fixture(None);
    assert!(matches!(
        ProcessRegions::new(m.clone(), &foreign, layout()),
        Err(Error::Configuration)
    ));
    let regions = ProcessRegions::new(m.clone(), &h, layout()).unwrap();
    let clone = regions.clone();
    *m.lock().unwrap() = ExclusiveMemory::new(Memory::default(), cfg()).unwrap();
    assert_eq!(regions.snapshot(), Err(Error::StaleGeneration));
    assert_eq!(clone.snapshot(), Err(Error::StaleGeneration));
    let p = m.clone();
    assert!(std::thread::spawn(move || {
        let _guard = p.lock().unwrap();
        panic!("synthetic poisoned memory");
    })
    .join()
    .is_err());
    assert_eq!(regions.snapshot(), Err(Error::Poisoned));
}
#[test]
fn concurrent_resize_cannot_move_virtual_reservations() {
    let (m, h) = fixture(None);
    let regions = ProcessRegions::new(m.clone(), &h, layout()).unwrap();
    let barrier = std::sync::Barrier::new(3);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            barrier.wait();
            for i in 0..1024 {
                m.lock().unwrap().resize_heap(&h, (i % 3) * 4096).unwrap();
            }
        });
        for _ in 0..2 {
            scope.spawn(|| {
                barrier.wait();
                for _ in 0..1024 {
                    assert_eq!(regions.snapshot().unwrap(), layout());
                }
            });
        }
    });
}

#[test]
fn native_heap_completion_changes_commitment_without_changing_catalog() {
    use mkw_runtime::{
        exclusive_calls::{Routed, Router, Target},
        exclusive_native::{Bridge, CpuV10},
        heap_services::{Abi, HeapServices},
    };
    use std::{path::PathBuf, process::Command};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let raw: Vec<_> = [0xd4000021u32, 0xd65f03c0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let source =
        mkw_codegen::emit_services_v3(&mkw_arm64::lift_services_v3(&raw, 0, 0).unwrap(), 0)
            .unwrap();
    let dir = root.join("private/testing/process-regions");
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
            Target::load_services(&lib, b"mkw_guest_v10_budget\0", GuestAddr(0x10000), 0x10000)
                .unwrap()
        };
        let router = Router::new(vec![target], 1).unwrap();
        let mut l = layout();
        l.heap.size = 2 * 1024 * 1024;
        let (m, h) = ExclusiveMemory::with_heap(
            Memory::default(),
            cfg(),
            HeapSpec {
                base: l.heap.base,
                capacity: l.heap.size as usize,
            },
        )
        .unwrap();
        let m = Arc::new(Mutex::new(m));
        let regions = ProcessRegions::new(m.clone(), &h, l).unwrap();
        let svc = HeapServices::new(Abi::Nx64ZeroedBoundedHeap, m.clone(), h.clone())
            .unwrap()
            .registry();
        let mut bridge = Bridge::new(m.clone()).unwrap();
        for size in [0, l.heap.size, 2 * l.heap.size] {
            for steps in 0..=2 {
                m.lock().unwrap().resize_heap(&h, 0).unwrap();
                let mut c = CpuV10::default();
                c.base.x[1] = size;
                c.base.x[30] = 0x20000;
                let mut left = steps;
                let status = {
                    let mut a = bridge.attach(&mut c).unwrap();
                    let mut r = Routed::attach_services(&mut a, &router, &svc).unwrap();
                    r.run(GuestAddr(0x10000), &mut left).unwrap()
                };
                assert_eq!(status, if steps < 2 { 2 } else { 0 });
                assert_eq!(regions.snapshot().unwrap(), l);
                assert_eq!(
                    m.lock().unwrap().heap_size(&h).unwrap() as u64,
                    if steps > 0 && size <= l.heap.size {
                        size
                    } else {
                        0
                    }
                );
                assert!(m.lock().unwrap().read(l.alias.base, Width::Byte).is_err());
                assert!(m.lock().unwrap().read(l.remote.base, Width::Byte).is_err());
            }
        }
    }
}
