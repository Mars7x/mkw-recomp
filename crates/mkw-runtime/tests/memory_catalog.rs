use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Heap, HeapSpec, Width},
    memory_catalog::{Declaration, Error, MemoryCatalog, State},
    process_regions::{Layout, Span},
    GuestAddr, Memory, Permissions,
};
use std::sync::{Arc, Mutex};
fn span(base: u64, size: u64) -> Span {
    Span {
        base: GuestAddr(base),
        size,
    }
}
fn cfg() -> Config {
    Config {
        granule: 16,
        max_owners: 1,
    }
}
fn layout() -> Layout {
    Layout {
        address_space: span(0, 0x1_0000_0000),
        code: span(0x1000, 0x1000),
        heap: span(0x10000000, 8192),
        stack: span(0x3000, 4096),
        alias: span(0x20000000, 4096),
        remote: span(0x30000000, 4096),
    }
}
fn fixture() -> (Arc<Mutex<ExclusiveMemory>>, Heap, Vec<Declaration>) {
    let mut m = Memory::default();
    let mut ds = Vec::new();
    for (base, p, state) in [
        (0x1000, Permissions::RX, State::Code),
        (0x2000, Permissions::RW, State::CodeData),
        (0x3000, Permissions::RW, State::Stack),
        (0x5000, Permissions::RW, State::ThreadLocal),
        (0x6000, Permissions::RW, State::ThreadLocal),
    ] {
        m.map(GuestAddr(base), vec![0; 4096], p, "uninformative label")
            .unwrap();
        ds.push(Declaration {
            span: span(base, 4096),
            state,
        });
    }
    let (m, h) = ExclusiveMemory::with_heap(
        m,
        cfg(),
        HeapSpec {
            base: GuestAddr(0x8000),
            capacity: 8192,
        },
    )
    .unwrap();
    (Arc::new(Mutex::new(m)), h, ds)
}
#[test]
fn complete_partition_coalesces_states_and_tracks_heap_commitment() {
    let (m, h, d) = fixture();
    let c = MemoryCatalog::new(m.clone(), &h, span(0, 0x10000), d).unwrap();
    let owner = m.lock().unwrap().new_owner().unwrap();
    for size in [0, 4096, 8192, 4096, 0] {
        m.lock().unwrap().resize_heap(&h, size).unwrap();
        m.lock()
            .unwrap()
            .load(&owner, GuestAddr(0x3000), Width::Double)
            .unwrap();
        let entries = c.snapshot().unwrap();
        let mut end = 0;
        for e in &entries {
            assert_eq!(e.span.base.0, end);
            assert!(e.span.size > 0);
            end += e.span.size;
        }
        assert_eq!(end, 0x10000);
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.state == State::ThreadLocal)
                .map(|e| e.span.size)
                .sum::<u64>(),
            8192
        );
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.state == State::Normal)
                .map(|e| e.span.size)
                .sum::<u64>(),
            size as u64
        );
        assert_eq!(
            entries
                .iter()
                .filter(|e| e.state == State::Free)
                .map(|e| e.span.size)
                .sum::<u64>(),
            0x10000 - 5 * 4096 - size as u64
        );
        assert!(m
            .lock()
            .unwrap()
            .store_conditional(&owner, GuestAddr(0x3000), Width::Double, 7)
            .unwrap());
    }
    assert!(m.lock().unwrap().new_owner().is_err());
}
#[test]
fn incomplete_ambiguous_or_incompatible_declarations_reject() {
    let (m, h, d) = fixture();
    for mode in 0..9 {
        let mut ds = d.clone();
        let mut space = span(0, 0x10000);
        match mode {
            0 => {
                ds.pop();
            }
            1 => ds.push(ds[0]),
            2 => ds[0].span.size = 8192,
            3 => ds[0].state = State::Stack,
            4 => ds[1].state = State::Code,
            5 => ds[1].state = State::Free,
            6 => space.size = 0x9000,
            7 => space.base = GuestAddr(1),
            _ => space = span(u64::MAX - 4095, 8192),
        }
        assert!(
            matches!(
                MemoryCatalog::new(m.clone(), &h, space, ds),
                Err(Error::Configuration)
            ),
            "{mode}"
        );
    }
    let foreign = fixture().1;
    assert!(matches!(
        MemoryCatalog::new(m.clone(), &foreign, span(0, 0x10000), d.clone()),
        Err(Error::Configuration)
    ));
    let mut reversed = d;
    reversed.reverse();
    assert!(MemoryCatalog::new(m, &h, span(0, 0x10000), reversed).is_ok());
}
#[test]
fn replacement_and_poison_never_rebind_state_declarations() {
    let (m, h, d) = fixture();
    let c = MemoryCatalog::new(m.clone(), &h, span(0, 0x10000), d).unwrap();
    *m.lock().unwrap() = ExclusiveMemory::new(Memory::default(), cfg()).unwrap();
    assert_eq!(c.snapshot(), Err(Error::StaleGeneration));
    let other = m.clone();
    assert!(std::thread::spawn(move || {
        let _g = other.lock().unwrap();
        panic!("synthetic poison");
    })
    .join()
    .is_err());
    assert_eq!(c.snapshot(), Err(Error::Poisoned));
}
#[test]
fn concurrent_snapshots_are_whole_partitions() {
    let (m, h, d) = fixture();
    let c = MemoryCatalog::new(m.clone(), &h, span(0, 0x10000), d).unwrap();
    let barrier = std::sync::Barrier::new(3);
    std::thread::scope(|s| {
        s.spawn(|| {
            barrier.wait();
            for i in 0..1024 {
                m.lock().unwrap().resize_heap(&h, (i % 3) * 4096).unwrap();
            }
        });
        for _ in 0..2 {
            s.spawn(|| {
                barrier.wait();
                for _ in 0..1024 {
                    let es = c.snapshot().unwrap();
                    let mut end = 0;
                    for e in es {
                        assert_eq!(e.span.base.0, end);
                        end += e.span.size;
                    }
                    assert_eq!(end, 0x10000);
                }
            });
        }
    });
}
#[test]
fn native_heap_completion_updates_catalog_normal_pages() {
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
    let dir = root.join("private/testing/memory-catalog");
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
        let regions = MemoryCatalog::new(m.clone(), &h, l.address_space, vec![]).unwrap();
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
                let entries = regions.snapshot().unwrap();
                assert_eq!(
                    entries
                        .iter()
                        .filter(|e| e.state == State::Normal)
                        .map(|e| e.span.size)
                        .sum::<u64>(),
                    if steps > 0 && size <= l.heap.size {
                        size
                    } else {
                        0
                    }
                );
                assert_eq!(
                    entries.iter().map(|e| e.span.size).sum::<u64>(),
                    l.address_space.size
                );
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
