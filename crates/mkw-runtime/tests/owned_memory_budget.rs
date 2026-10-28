use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Heap, HeapSpec, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    heap_services::{Abi, HeapServices},
    owned_memory_budget::{Error, OwnedMemoryBudget},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
const PAGE: u64 = 4096;
const MIB: usize = 1024 * 1024;
const HEAP: u64 = 0x1_0000_0000;
fn cfg() -> Config {
    Config {
        granule: 16,
        max_owners: 2,
    }
}
fn fixture() -> (Arc<Mutex<ExclusiveMemory>>, Heap) {
    let mut raw = Memory::default();
    for (address, permissions) in [
        (0x2000, Permissions::RW),
        (0x4000, Permissions::R),
        (0x6000, Permissions::RX),
    ] {
        raw.map(
            GuestAddr(address),
            vec![0; PAGE as usize],
            permissions,
            "synthetic private mapping",
        )
        .unwrap();
    }
    let (memory, heap) = ExclusiveMemory::with_heap(
        raw,
        cfg(),
        HeapSpec {
            base: GuestAddr(HEAP),
            capacity: 4 * MIB,
        },
    )
    .unwrap();
    (Arc::new(Mutex::new(memory)), heap)
}
#[test]
fn exact_pages_full_capacity_and_explicit_allowance_are_required() {
    let (m, _) = fixture();
    for allowance in [0, 1, PAGE, 4 * MIB as u64, u64::MAX] {
        assert!(matches!(
            OwnedMemoryBudget::new(m.clone(), allowance),
            Err(Error::Configuration)
        ));
    }
    for (base, len) in [(0x2001, 4096), (0x2000, 4095), (0x2000, 4097)] {
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(base),
            vec![0; len],
            Permissions::RW,
            "non-page mapping",
        )
        .unwrap();
        let m = Arc::new(Mutex::new(ExclusiveMemory::new(raw, cfg()).unwrap()));
        assert!(matches!(
            OwnedMemoryBudget::new(m, 16384),
            Err(Error::Configuration)
        ));
    }
    // This is a configured allowance, never an allocation or discovered RAM.
    let allowance = u64::MAX - 4095;
    let s = OwnedMemoryBudget::new(m, allowance)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(s.used, 3 * PAGE);
    assert_eq!(s.committed_capacity, 3 * PAGE + 4 * MIB as u64);
    assert_eq!(s.available, allowance - 3 * PAGE);
}
#[test]
fn live_usage_excludes_hidden_capacity_and_snapshots_preserve_reservations() {
    let (m, h) = fixture();
    let budget = OwnedMemoryBudget::new(m.clone(), 8 * MIB as u64).unwrap();
    let owner = m.lock().unwrap().new_owner().unwrap();
    for pages in [0, 1, 3, 1024, 2, 0, 1024] {
        m.lock()
            .unwrap()
            .resize_heap(&h, pages * PAGE as usize)
            .unwrap();
        m.lock()
            .unwrap()
            .load(&owner, GuestAddr(0x2000), Width::Double)
            .unwrap();
        let s = budget.snapshot().unwrap();
        assert_eq!(s.used, (3 + pages as u64) * PAGE);
        assert_eq!(s.available + s.used, s.allowance);
        assert_eq!(s.committed_capacity, 3 * PAGE + 4 * MIB as u64);
        assert!(m
            .lock()
            .unwrap()
            .store_conditional(&owner, GuestAddr(0x2000), Width::Double, 7)
            .unwrap());
    }
    let before = budget.snapshot().unwrap();
    assert!(m.lock().unwrap().resize_heap(&h, 4 * MIB + 4096).is_err());
    assert_eq!(budget.snapshot().unwrap(), before);
}
#[test]
fn clones_reject_replacement_and_poison_instead_of_reusing_stale_accounting() {
    let (m, _) = fixture();
    let budget = OwnedMemoryBudget::new(m.clone(), 8 * MIB as u64).unwrap();
    let copy = budget.clone();
    *m.lock().unwrap() = ExclusiveMemory::new(Memory::default(), cfg()).unwrap();
    assert_eq!(budget.snapshot(), Err(Error::StaleGeneration));
    assert_eq!(copy.snapshot(), Err(Error::StaleGeneration));
    assert_eq!(
        OwnedMemoryBudget::new(m.clone(), PAGE)
            .unwrap()
            .snapshot()
            .unwrap()
            .used,
        0
    );
    let poisoned = m.clone();
    assert!(std::thread::spawn(move || {
        let _guard = poisoned.lock().unwrap();
        panic!("synthetic poison");
    })
    .join()
    .is_err());
    assert_eq!(copy.snapshot(), Err(Error::Poisoned));
    assert!(matches!(
        OwnedMemoryBudget::new(m, PAGE),
        Err(Error::Poisoned)
    ));
}
#[test]
fn concurrent_snapshots_track_only_committed_whole_page_states() {
    let (m, h) = fixture();
    let budget = OwnedMemoryBudget::new(m.clone(), 8 * MIB as u64).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    std::thread::scope(|scope| {
        let start = barrier.clone();
        scope.spawn(move || {
            start.wait();
            for i in 0..1024 {
                m.lock().unwrap().resize_heap(&h, (i % 17) * 4096).unwrap();
            }
        });
        for _ in 0..2 {
            let start = barrier.clone();
            let b = budget.clone();
            scope.spawn(move || {
                start.wait();
                for _ in 0..1024 {
                    let s = b.snapshot().unwrap();
                    assert_eq!(s.used % PAGE, 0);
                    assert!((3 * PAGE..=19 * PAGE).contains(&s.used));
                    assert_eq!(s.available + s.used, s.allowance);
                    assert_eq!(s.committed_capacity, 3 * PAGE + 4 * MIB as u64);
                }
            });
        }
    });
    assert_eq!(budget.snapshot().unwrap().used, (3 + 1023 % 17) * PAGE);
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(opt: &str) -> libloading::Library {
    let raw: Vec<_> = [0xd4000021u32, 0xd65f03c0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    let source =
        mkw_codegen::emit_services_v3(&mkw_arm64::lift_services_v3(&raw, 0, 0).unwrap(), 0)
            .unwrap();
    let dir = root().join("private/testing/owned-memory-budget");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("synthetic{opt}.c"));
    let lib = src.with_extension("so");
    std::fs::write(&src, source).unwrap();
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
        .arg(root().join("native"))
        .arg(&src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
#[test]
fn native_heap_service_effects_and_budget_stops_feed_live_accounting() {
    for opt in ["-O0", "-O2"] {
        let lib = build(opt);
        let target = unsafe {
            Target::load_services(&lib, b"mkw_guest_v10_budget\0", GuestAddr(0x10000), 0x10000)
                .unwrap()
        };
        let router = Router::new(vec![target], 1).unwrap();
        let (m, h) = fixture();
        let budget = OwnedMemoryBudget::new(m.clone(), 8 * MIB as u64).unwrap();
        let services = HeapServices::new(Abi::Nx64ZeroedBoundedHeap, m.clone(), h.clone())
            .unwrap()
            .registry();
        let mut bridge = Bridge::new(m.clone()).unwrap();
        for size in [
            0,
            2 * MIB as u64,
            4 * MIB as u64,
            6 * MIB as u64,
            PAGE,
            u64::MAX,
        ] {
            for steps in 0..=3 {
                m.lock().unwrap().resize_heap(&h, 2 * MIB).unwrap();
                let mut cpu = CpuV10::default();
                cpu.base.x = [77; 31];
                cpu.base.x[1] = size;
                cpu.base.x[30] = 0x20000;
                let mut left = steps;
                let status = {
                    let mut attachment = bridge.attach(&mut cpu).unwrap();
                    let mut routed =
                        Routed::attach_services(&mut attachment, &router, &services).unwrap();
                    routed.run(GuestAddr(0x10000), &mut left).unwrap()
                };
                assert_eq!(
                    (status, left),
                    if steps < 2 { (2, 0) } else { (0, steps - 2) }
                );
                let accepted = steps > 0 && size <= 4 * MIB as u64 && size % (2 * MIB as u64) == 0;
                let expected = if accepted { size } else { 2 * MIB as u64 };
                assert_eq!(budget.snapshot().unwrap().used, 3 * PAGE + expected);
                assert_eq!(m.lock().unwrap().heap_size(&h).unwrap() as u64, expected);
                assert_eq!(&cpu.base.x[2..30], &[77; 28]);
            }
        }
    }
}
