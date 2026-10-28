//! Original synthetic fixtures. Real owned endpoints, no IPC packet/game code.
use mkw_runtime::{
    event_services::{Abi, INVALID_HANDLE, INVALID_STATE, OUT_OF_RANGE, TIMED_OUT},
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    kernel_handles::{Handle, Table},
    owned_events::{Factory, WriteEnd},
    services::Services,
    table_event_services::TableEventServices,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/table-event-services")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut text = String::new();
    for (n, words) in [0x45u32, 0x11, 0x12, 0x17, 0x16, 0x18]
        .into_iter()
        .map(|v| vec![0xd4000001 | (v << 5), 0xd65f03c0])
        .chain([vec![0xc85ffca4, 0xd65f03c0], vec![0xc807fca6, 0xd65f03c0]])
        .chain([vec![0xd4000521, 0xd65f03c0]])
        .enumerate()
    {
        let bytes = words
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        text += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", &format!("own_{n}"));
    }
    let src = path.join("synthetic.c");
    let lib = path.join(format!("synthetic{opt}.so"));
    std::fs::write(&src, text).unwrap();
    let output = Command::new("cc")
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
        .arg(src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    std::fs::write(path.join(format!("compiler{opt}.log")), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        (0..9)
            .map(|n| {
                let at = 0x4000 + n * 0x1000;
                unsafe {
                    Target::load_services(lib, format!("own_{n}\0").as_bytes(), GuestAddr(at), at)
                        .unwrap()
                }
            })
            .collect(),
        8,
    )
    .unwrap()
}
fn memory(array: &[u32]) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 0x2000],
        Permissions::RW,
        "author-created counter",
    )
    .unwrap();
    m.map(
        GuestAddr(0xd000),
        array.iter().flat_map(|n| n.to_le_bytes()).collect(),
        Permissions::R,
        "read-only handle array",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 1,
            },
        )
        .unwrap(),
    ))
}
fn cpu(a: [u64; 4]) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x = [0x123456789abcdef0; 31];
    c.base.x[..4].copy_from_slice(&a);
    c.base.x[5] = 0x2000;
    c.base.x[6] = 99;
    c.base.x[30] = 0xe000;
    c.base.sp = 0x2800;
    c
}
fn run(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    c: &mut CpuV10,
    op: u64,
    budget: u64,
) -> (i32, u64) {
    let mut b = budget;
    let status = {
        let mut a = bridge.attach(c).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        r.run(GuestAddr(0x4000 + op * 0x1000), &mut b).unwrap()
    };
    (status, b)
}
fn call(router: &Router<'_>, svc: &Services, op: u64, args: [u64; 4], array: &[u32]) -> CpuV10 {
    let mut bridge = Bridge::new(memory(array)).unwrap();
    let mut c = cpu(args);
    assert_eq!(run(&mut bridge, router, svc, &mut c, op, 10), (0, 8));
    c
}
fn import(to: &Table, from: &Table, h: Handle) -> Handle {
    to.import(from.export(h).unwrap()).unwrap()
}
#[test]
fn reference_event_handlers_use_real_transferred_endpoints_and_persistent_shared_state() {
    let path = folder("state");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let f = Factory::new(2).unwrap();
        let source = Table::new(4).unwrap();
        let guest = Arc::new(Table::new(8).unwrap());
        let (aw, ar) = f.create(&source).unwrap();
        let (bw, br) = f.clone().create(&source).unwrap();
        let a = import(&guest, &source, ar);
        let b = import(&guest, &source, br);
        let duplicate = import(&guest, &source, br);
        let w = import(&guest, &source, aw);
        source.close(ar).unwrap();
        source.close(br).unwrap();
        let svc = TableEventServices::new(Abi::Horizon64EventReference, guest.clone()).registry();
        source.get::<WriteEnd>(bw).unwrap().signal().unwrap();
        let mut wide = vec![a.raw(); 63];
        wide.push(b.raw());
        assert_eq!(
            &call(&router, &svc, 5, [9, 0xd000, 64, 0], &wide).base.x[..2],
            &[0, 63]
        );
        assert_eq!(
            &call(&router, &svc, 5, [9, 0xd000, 64, 0], &vec![b.raw(); 64])
                .base
                .x[..2],
            &[0, 0]
        );

        let c = call(
            &router,
            &svc,
            5,
            [9, 0xd000, 3, 0],
            &[a.raw(), b.raw(), duplicate.raw()],
        );
        assert_eq!(&c.base.x[..2], &[0, 1]);
        let c = call(&router, &svc, 5, [9, 0xd000, 2, 0], &[b.raw(), b.raw()]);
        assert_eq!(&c.base.x[..2], &[0, 0]);
        for values in [[b.raw(), 0], [b.raw(), w.raw()], [b.raw(), bw.raw()]] {
            assert_eq!(
                call(&router, &svc, 5, [9, 0xd000, 2, 0], &values).base.x[0],
                u64::from(INVALID_HANDLE)
            );
        }
        assert_eq!(
            call(&router, &svc, 1, [u64::from(a.raw()), 0, 0, 0], &[])
                .base
                .x[0],
            u64::from(INVALID_HANDLE)
        );
        assert_eq!(
            call(
                &router,
                &svc,
                1,
                [0xdeadbeef00000000 | u64::from(w.raw()), 0, 0, 0],
                &[]
            )
            .base
            .x[0],
            0
        );
        assert_eq!(
            &call(&router, &svc, 5, [9, 0xd000, 2, 0], &[a.raw(), b.raw()])
                .base
                .x[..2],
            &[0, 0]
        );
        assert_eq!(
            call(&router, &svc, 3, [u64::from(a.raw()), 0, 0, 0], &[])
                .base
                .x[0],
            0
        );
        assert_eq!(
            call(&router, &svc, 3, [u64::from(a.raw()), 0, 0, 0], &[])
                .base
                .x[0],
            u64::from(INVALID_STATE)
        );
        assert_eq!(
            call(&router, &svc, 2, [u64::from(w.raw()), 0, 0, 0], &[])
                .base
                .x[0],
            0
        );
        call(&router, &svc, 4, [u64::from(b.raw()), 0, 0, 0], &[]);
        source.close(bw).unwrap();
        assert_eq!(
            &call(&router, &svc, 5, [9, 0xd000, 1, 0], &[duplicate.raw()])
                .base
                .x[..2],
            &[0, 0]
        );
        assert!(f.create(&source).is_err());
        assert_eq!(
            call(&router, &svc, 5, [9, 0xd000, 1, 0], &[b.raw()]).base.x[0],
            u64::from(INVALID_HANDLE)
        );
        call(&router, &svc, 4, [u64::from(duplicate.raw()), 0, 0, 0], &[]);
        let (x, y) = f.create(&source).unwrap();
        source.close(x).unwrap();
        source.close(y).unwrap();
        call(&router, &svc, 4, [u64::from(a.raw()), 0, 0, 0], &[]);
        call(&router, &svc, 4, [u64::from(w.raw()), 0, 0, 0], &[]);
        source.close(aw).unwrap();
        let (x, y) = f.create(&source).unwrap();
        source.close(x).unwrap();
        source.close(y).unwrap();
        guest.shutdown().unwrap();
        source.shutdown().unwrap();
    }
}
#[test]
fn table_event_poll_refusals_budgets_and_pinned_authority_monitor_contract_remain_explicit() {
    let path = folder("boundaries");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let f = Factory::new(1).unwrap();
        let source = Table::new(2).unwrap();
        let guest = Arc::new(Table::new(4).unwrap());
        let (w, r) = f.create(&source).unwrap();
        let read = import(&guest, &source, r);
        let write = import(&guest, &source, w);
        let foreign_factory = Factory::new(1).unwrap();
        let foreign = Table::new(2).unwrap();
        let (_, other) = foreign_factory.create(&foreign).unwrap();
        let other = import(&guest, &foreign, other);
        let svc = TableEventServices::new(Abi::Horizon64EventReference, guest.clone()).registry();
        for kind in 0..4 {
            source.get::<WriteEnd>(w).unwrap().clear().unwrap();
            if kind >= 2 {
                source.get::<WriteEnd>(w).unwrap().signal().unwrap();
            }
            let array = match kind {
                2 => vec![read.raw(), other.raw()],
                3 => vec![read.raw(), write.raw()],
                _ => vec![read.raw()],
            };
            let mem = memory(&array);
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut c = cpu([9, 0xd000, array.len() as u64, u64::from(kind == 1)]);
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 6, 10), (0, 8));
            let before = c.base.x;
            let handled = kind == 0 || kind == 3;
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 5, 10),
                if handled { (0, 8) } else { (3, 9) }
            );
            if !handled {
                assert_eq!(c.base.x, before);
            } else {
                assert_eq!(
                    c.base.x[0],
                    u64::from(if kind == 0 { TIMED_OUT } else { INVALID_HANDLE })
                );
            }
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 7, 10), (0, 8));
            assert_eq!(c.base.x[7], u64::from(handled));
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x2000), Width::Double)
                    .unwrap(),
                if handled { 0 } else { 99 }
            );
        }
        source.get::<WriteEnd>(w).unwrap().clear().unwrap();
        for (pointer, count, timeout, status, result) in [
            (u64::MAX, 0, 0, 0, TIMED_OUT),
            (u64::MAX, 65, 0, 0, OUT_OF_RANGE),
            (u64::MAX, u32::MAX as u64, 0, 0, OUT_OF_RANGE),
            (u64::MAX, 1, 0, 5, 0),
            (0xd000, 0xdeadbeef00000001, 0, 0, TIMED_OUT),
            (u64::MAX, 1, u64::MAX, 3, 0),
        ] {
            let mut bridge = Bridge::new(memory(&[read.raw()])).unwrap();
            let mut c = cpu([9, pointer, count, timeout]);
            let before = c.base.x;
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 5, 10),
                (status, if status == 0 { 8 } else { 9 })
            );
            if status == 0 {
                assert_eq!(c.base.x[0], u64::from(result));
                assert_eq!(c.base.x[1], pointer);
            } else {
                assert_eq!(c.base.x, before);
            }
        }
        for budget in 0..3 {
            let mut bridge = Bridge::new(memory(&[read.raw()])).unwrap();
            let mut c = cpu([9, 0xd000, 1, 0]);
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 5, budget),
                if budget < 2 { (2, 0) } else { (0, 0) }
            );
            assert_eq!(
                c.base.x[0],
                if budget == 0 { 9 } else { u64::from(TIMED_OUT) }
            );
        }
        for (op, args) in [(0, [9, 0, 0, 0]), (3, [0xffff8001, 0, 0, 0])] {
            let mut bridge = Bridge::new(memory(&[])).unwrap();
            let mut c = cpu(args);
            let before = c.base.x;
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, op, 10), (3, 9));
            assert_eq!(c.base.x, before);
        }
        let mut bridge = Bridge::new(memory(&[read.raw()])).unwrap();
        let empty = Services::new(vec![]).unwrap();
        let mut c = cpu([9, 0xd000, 1, 0]);
        assert_eq!(run(&mut bridge, &router, &empty, &mut c, 5, 10), (3, 9));
        guest.shutdown().unwrap();
        source.shutdown().unwrap();
        foreign.shutdown().unwrap();
    }
}

#[test]
fn explicit_profile_composition_rejects_conflicts_and_preserves_final_process_restrictions() {
    use mkw_runtime::host_call::HostFault;
    use mkw_runtime::process_entropy::{Abi as EntropyAbi, ProcessEntropy};
    use mkw_runtime::services::{Call, Outputs, Reply, Service};
    use std::{
        collections::BTreeSet,
        sync::atomic::{AtomicUsize, Ordering},
    };
    struct Counter(Arc<AtomicUsize>);
    impl Service for Counter {
        fn invoke(&self, _: &Call<'_>) -> Result<Reply, HostFault> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Reply::Return(Outputs::default()))
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let handler = Arc::new(Counter(count.clone())) as Arc<dyn Service>;
    let left = Services::new((0..128).map(|n| (n, handler.clone())).collect()).unwrap();
    let right = Services::new((128..256).map(|n| (n, handler.clone())).collect()).unwrap();
    let full = left.combined(&right).unwrap();
    let extra = Services::new(vec![(0xffff, handler)]).unwrap();
    assert!(full.combined(&extra).is_err());
    assert!(left.combined(&left).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let path = folder("composition");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let factory = Factory::new(1).unwrap();
        let producer = Table::new(2).unwrap();
        let guest = Arc::new(Table::new(2).unwrap());
        let (w, r) = factory.create(&producer).unwrap();
        let write = import(&guest, &producer, w);
        let read = import(&guest, &producer, r);
        let events =
            TableEventServices::new(Abi::Horizon64EventReference, guest.clone()).registry();
        let entropy = ProcessEntropy::from_synthetic_words(
            EntropyAbi::Horizon64EntropyReference,
            [0, 1, 0x1122334455667788, 3],
        )
        .registry();
        let combined = events.combined(&entropy).unwrap();
        assert!(combined.combined(&entropy).is_err());
        assert!(events.combined(&events).is_err());
        let restricted = combined.restricted_to(&BTreeSet::from([0x18, 0x29]));
        assert_eq!(
            &call(&router, &restricted, 8, [9, 11, 0, 2], &[]).base.x[..2],
            &[0, 0x1122334455667788]
        );
        let mut bridge = Bridge::new(memory(&[])).unwrap();
        let mut c = cpu([u64::from(write.raw()), 0, 0, 0]);
        let before = c.base.x;
        assert_eq!(
            run(&mut bridge, &router, &restricted, &mut c, 1, 10),
            (3, 9)
        );
        assert_eq!(c.base.x, before);
        assert_eq!(
            call(&router, &restricted, 5, [9, 0xd000, 1, 0], &[read.raw()])
                .base
                .x[0],
            u64::from(TIMED_OUT)
        );
        assert_eq!(
            call(
                &router,
                &combined,
                1,
                [u64::from(write.raw()), 0, 0, 0],
                &[]
            )
            .base
            .x[0],
            0
        );
        assert_eq!(
            &call(&router, &restricted, 5, [9, 0xd000, 1, 0], &[read.raw()])
                .base
                .x[..2],
            &[0, 0]
        );
        assert_eq!(
            &call(&router, &combined, 8, [9, 11, 0, 2], &[]).base.x[..2],
            &[0, 0x1122334455667788]
        );
        guest.shutdown().unwrap();
        producer.shutdown().unwrap();
    }
}
