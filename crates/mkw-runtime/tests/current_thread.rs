use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    kernel_handles::Table,
    services::Services,
    thread_queries::{Abi, ThreadIdentity, ThreadQueries, GET_ID, GET_PRIORITY, INVALID_HANDLE},
    GuestAddr, Memory, Permissions,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        (
            "priority",
            vec![0xd4000001 | ((GET_PRIORITY as u32) << 5), 0xd65f03c0],
        ),
        (
            "identity",
            vec![0xd4000001 | ((GET_ID as u32) << 5), 0xd65f03c0],
        ),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("store", vec![0xc807fca6, 0xd65f03c0]),
        (
            "wrapper",
            vec![
                0xf81f0fe0,
                0xd4000001 | ((GET_ID as u32) << 5),
                0xf94003e2,
                0xf9000041,
                0x910043ff,
                0xd65f03c0,
            ],
        ),
    ] {
        let raw = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&raw, 0, 0).unwrap();
        source += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let dir = root().join("private/testing/current-thread").join(label);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("synthetic{opt}.c"));
    let lib = src.with_extension("so");
    std::fs::write(&src, source).unwrap();
    let cc = Command::new("cc")
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
        .arg(src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        cc.status.success(),
        "{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x4000, b"priority\0".as_slice()),
            (0x5000, b"identity\0".as_slice()),
            (0x6000, b"reserve\0".as_slice()),
            (0x7000, b"store\0".as_slice()),
            (0x8000, b"wrapper\0".as_slice()),
        ]
        .into_iter()
        .map(|(at, n)| unsafe { Target::load_services(lib, n, GuestAddr(at), at).unwrap() })
        .collect(),
        8,
    )
    .unwrap()
}
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x2000),
        vec![0; 4096],
        Permissions::RW,
        "synthetic stack/output",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 2,
            },
        )
        .unwrap(),
    ))
}
fn cpu(handle: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0xabcdef0123456789;
    c.base.x[1] = handle;
    c.base.x[2] = 0x8877665544332211;
    c.base.x[30] = 0x9000;
    c.base.sp = 0x2800;
    c
}
fn run(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    cpu: &mut CpuV10,
    at: u64,
    limit: u64,
) -> (i32, u64) {
    let mut budget = limit;
    let status = {
        let mut a = bridge.attach(cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        r.run(GuestAddr(at), &mut budget).unwrap()
    };
    (status, budget)
}
#[test]
fn current_identity_is_pinned_to_one_live_bridge_and_survives_handle_close() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        let table = Arc::new(Table::new(2).unwrap());
        let h = ThreadIdentity::register(&table, 44).unwrap();
        let object = table.get::<ThreadIdentity>(h).unwrap();
        let id = object.id();
        let weak = Arc::downgrade(&object);
        drop(object);
        let queries = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table.clone());
        let mem = memory();
        let mut a = Bridge::new(mem.clone()).unwrap();
        let mut b = Bridge::new(mem.clone()).unwrap();
        let services = queries.registry_for_current_thread(&a, h).unwrap();
        table.close(h).unwrap();
        assert!(weak.upgrade().is_some());
        assert!(queries.registry_for_current_thread(&a, h).is_err());
        for (at, value) in [(0x4000, 44), (0x5000, id)] {
            for budget in 0..=3 {
                let mut c = cpu(0xaabbccddffff8000);
                let old = c.base.x;
                assert_eq!(
                    run(&mut a, &r, &services, &mut c, at, budget),
                    if budget < 2 { (2, 0) } else { (0, budget - 2) }
                );
                assert_eq!(
                    [c.base.x[0], c.base.x[1]],
                    if budget == 0 {
                        [old[0], old[1]]
                    } else {
                        [0, value]
                    }
                );
                assert_eq!(&c.base.x[2..], &old[2..]);
            }
            let mut c = cpu(0xffff8000);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &services, &mut c, at, 2), (3, 1));
            assert_eq!(c.base.x, old);
        }
        let mut foreign = Bridge::new(memory()).unwrap();
        let mut c = cpu(0xffff8000);
        let old = c.base.x;
        assert_eq!(run(&mut foreign, &r, &services, &mut c, 0x5000, 2), (3, 1));
        assert_eq!(c.base.x, old);
        let mut c = cpu(u64::from(h.raw()));
        assert_eq!(run(&mut a, &r, &services, &mut c, 0x5000, 2), (0, 0));
        assert_eq!(c.base.x[0], u64::from(INVALID_HANDLE));
        drop(a);
        let mut replacement = Bridge::new(mem).unwrap();
        let mut c = cpu(0xffff8000);
        let old = c.base.x;
        assert_eq!(
            run(&mut replacement, &r, &services, &mut c, 0x5000, 2),
            (3, 1)
        );
        assert_eq!(c.base.x, old);
        drop(services);
        assert!(weak.upgrade().is_none());
    }
}
#[test]
fn binding_rejects_invalid_handles_and_replaced_memory_generations() {
    let table = Arc::new(Table::new(2).unwrap());
    let h = ThreadIdentity::register(&table, 12).unwrap();
    let wrong = table.insert(Arc::new(123u64)).unwrap();
    let other = Table::new(1).unwrap();
    let foreign = ThreadIdentity::register(&other, 13).unwrap();
    let queries = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table);
    let m = memory();
    let bridge = Bridge::new(m.clone()).unwrap();
    for bad in [
        wrong,
        foreign,
        mkw_runtime::kernel_handles::Handle::from_raw(0xffff8000),
    ] {
        assert!(queries.registry_for_current_thread(&bridge, bad).is_err());
    }
    let services = queries.registry_for_current_thread(&bridge, h).unwrap();
    *m.lock().unwrap() = Arc::try_unwrap(memory()).unwrap().into_inner().unwrap();
    assert!(queries.registry_for_current_thread(&bridge, h).is_err());
    for opt in ["-O0", "-O2"] {
        let lib = build("generation", opt);
        let r = router(&lib);
        let mut next = Bridge::new(m.clone()).unwrap();
        let mut c = cpu(0xffff8000);
        let old = c.base.x;
        assert_eq!(run(&mut next, &r, &services, &mut c, 0x5000, 2), (3, 1));
        assert_eq!(c.base.x, old);
    }
}
#[test]
fn current_query_completion_and_refusal_preserve_existing_monitor_and_policy_rules() {
    for opt in ["-O0", "-O2"] {
        let lib = build("monitor", opt);
        let r = router(&lib);
        let table = Arc::new(Table::new(1).unwrap());
        let h = ThreadIdentity::register(&table, 7).unwrap();
        let queries = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table);
        let m = memory();
        let mut bridge = Bridge::new(m).unwrap();
        let services = queries.registry_for_current_thread(&bridge, h).unwrap();
        let denied = services.restricted_to(&BTreeSet::new());
        assert!(services.combined(&queries.registry()).is_err());
        for mode in 0..3 {
            let mut c = cpu(if mode == 1 { 0xffff8001 } else { 0xffff8000 });
            c.base.x[5] = 0x2000;
            c.base.x[6] = 77;
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x6000, 2), (0, 0));
            assert_eq!(
                run(
                    &mut bridge,
                    &r,
                    if mode == 2 { &denied } else { &services },
                    &mut c,
                    0x4000,
                    2
                ),
                if mode == 0 { (0, 0) } else { (3, 1) }
            );
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x7000, 2), (0, 0));
            assert_eq!(c.base.x[7], u64::from(mode == 0));
        }
    }
}
