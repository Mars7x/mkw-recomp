use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    kernel_handles::{Error, Table},
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
#[test]
fn logical_thread_identity_survives_handle_transfer_without_inventing_lifecycle() {
    let a = Table::new(2).unwrap();
    let b = Table::new(1).unwrap();
    assert_eq!(ThreadIdentity::register(&a, 64), Err(Error::Configuration));
    assert_eq!(ThreadIdentity::register(&a, 255), Err(Error::Configuration));
    assert_eq!(
        ThreadIdentity::register(&Table::new(0).unwrap(), 0),
        Err(Error::Quota)
    );
    let h = ThreadIdentity::register(&a, 44).unwrap();
    let object = a.get::<ThreadIdentity>(h).unwrap();
    let id = object.id();
    assert_ne!(id, 0);
    assert_eq!(object.initial_priority(), 44);
    let weak = Arc::downgrade(&object);
    assert_eq!(b.get::<ThreadIdentity>(h).err(), Some(Error::InvalidHandle));
    let exported = a.export(h).unwrap();
    let copy = b.import(exported.clone()).unwrap();
    assert_ne!(copy, h);
    assert_eq!(b.get::<ThreadIdentity>(copy).unwrap().id(), id);
    a.close(h).unwrap();
    assert_eq!(a.get::<ThreadIdentity>(h).err(), Some(Error::InvalidHandle));
    assert_eq!(b.get::<ThreadIdentity>(copy).unwrap().id(), id);
    drop(object);
    drop(exported);
    assert!(weak.upgrade().is_some());
    b.close(copy).unwrap();
    assert!(weak.upgrade().is_none());
    let fresh = ThreadIdentity::register(&a, 63).unwrap();
    assert_ne!(fresh, h);
    assert!(a.get::<ThreadIdentity>(fresh).unwrap().id() > id);
    a.shutdown().unwrap();
    assert_eq!(ThreadIdentity::register(&a, 1), Err(Error::Closed));
}
#[test]
fn concurrent_registration_and_close_keep_ids_unique() {
    let table = Arc::new(Table::new(8).unwrap());
    let ids = std::thread::scope(|scope| {
        let tasks = (0..4)
            .map(|_| {
                let table = table.clone();
                scope.spawn(move || {
                    let mut ids = Vec::new();
                    for i in 0..64 {
                        let h = ThreadIdentity::register(&table, i).unwrap();
                        let o = table.get::<ThreadIdentity>(h).unwrap();
                        ids.push(o.id());
                        table.close(h).unwrap();
                        assert_eq!(o.initial_priority(), i);
                    }
                    ids
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(ids.len(), 256);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), 256);
    assert!(ids.iter().all(|id| *id != 0));
}
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
    let dir = root().join("private/testing/thread-queries").join(label);
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
                max_owners: 1,
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
fn native_queries_return_owned_initial_state_with_exact_widths_budgets_and_refusals() {
    assert_eq!((GET_PRIORITY, GET_ID, INVALID_HANDLE), (0x0c, 0x25, 0xe401));
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let table = Arc::new(Table::new(70).unwrap());
        let svc = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table.clone()).registry();
        let mut bridge = Bridge::new(memory()).unwrap();
        for priority in 0..64 {
            let h = ThreadIdentity::register(&table, priority).unwrap();
            let id = table.get::<ThreadIdentity>(h).unwrap().id();
            for (at, value) in [(0x4000, priority as u64), (0x5000, id)] {
                for limit in 0..=3 {
                    let mut c = cpu(0xaabbccdd00000000 | u64::from(h.raw()));
                    let old = c.base.x;
                    assert_eq!(
                        run(&mut bridge, &r, &svc, &mut c, at, limit),
                        if limit < 2 { (2, 0) } else { (0, limit - 2) }
                    );
                    assert_eq!(&c.base.x[2..], &old[2..]);
                    assert_eq!(c.base.x[0], if limit == 0 { old[0] } else { 0 });
                    assert_eq!(c.base.x[1], if limit == 0 { old[1] } else { value });
                }
            }
            table.close(h).unwrap();
        }
        let wrong = table.insert(Arc::new(123u64)).unwrap();
        let stale = ThreadIdentity::register(&table, 44).unwrap();
        table.close(stale).unwrap();
        let other = Table::new(1).unwrap();
        let foreign = ThreadIdentity::register(&other, 44).unwrap();
        for handle in [
            0,
            wrong.raw(),
            stale.raw(),
            foreign.raw(),
            0x3fffffff,
            u32::MAX,
        ] {
            for at in [0x4000, 0x5000] {
                let mut c = cpu(u64::from(handle));
                assert_eq!(run(&mut bridge, &r, &svc, &mut c, at, 3), (0, 1));
                assert_eq!(&c.base.x[..2], &[u64::from(INVALID_HANDLE), 0]);
            }
        }
        let real = ThreadIdentity::register(&table, 44).unwrap();
        let denied = svc.restricted_to(&BTreeSet::new());
        assert!(svc.combined(&svc).is_err());
        for (registry, handle) in [
            (&svc, 0xffff8000u64),
            (&svc, 0xffff8001),
            (&denied, u64::from(real.raw())),
        ] {
            for at in [0x4000, 0x5000] {
                let mut c = cpu(handle);
                let old = c.base.x;
                assert_eq!(run(&mut bridge, &r, registry, &mut c, at, 3), (3, 2));
                assert_eq!(c.base.x, old);
            }
        }
    }
}
#[test]
fn copied_queries_wrapper_stores_and_monitor_completion_use_same_authority() {
    for opt in ["-O0", "-O2"] {
        let lib = build("copy", opt);
        let r = router(&lib);
        let a = Arc::new(Table::new(2).unwrap());
        let b = Arc::new(Table::new(2).unwrap());
        let h = ThreadIdentity::register(&a, 44).unwrap();
        let id = a.get::<ThreadIdentity>(h).unwrap().id();
        let copy = b.import(a.export(h).unwrap()).unwrap();
        a.close(h).unwrap();
        let svc = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, b.clone()).registry();
        let mem = memory();
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut c = cpu(u64::from(copy.raw()));
        c.base.x[0] = 0x2100;
        assert_eq!(run(&mut bridge, &r, &svc, &mut c, 0x8000, 10), (0, 4));
        assert_eq!(c.base.x[0], 0);
        assert_eq!(
            mem.lock()
                .unwrap()
                .read(GuestAddr(0x2100), Width::Double)
                .unwrap(),
            u128::from(id)
        );
        assert_eq!(c.base.sp, 0x2800);
        for handle in [u64::from(copy.raw()), 0, 0xffff8000] {
            let mut c = cpu(handle);
            c.base.x[5] = 0x2000;
            c.base.x[6] = 99;
            assert_eq!(run(&mut bridge, &r, &svc, &mut c, 0x6000, 10), (0, 8));
            let result = run(&mut bridge, &r, &svc, &mut c, 0x5000, 10);
            assert_eq!(result, if handle == 0xffff8000 { (3, 9) } else { (0, 8) });
            assert_eq!(run(&mut bridge, &r, &svc, &mut c, 0x7000, 10), (0, 8));
            assert_eq!(c.base.x[7], u64::from(handle != 0xffff8000));
        }
        b.close(copy).unwrap();
        let mut c = cpu(u64::from(copy.raw()));
        assert_eq!(run(&mut bridge, &r, &svc, &mut c, 0x5000, 3), (0, 1));
        assert_eq!(c.base.x[0], u64::from(INVALID_HANDLE));
    }
}

#[test]
fn guarded_initial_queries_use_registered_identity_and_explicit_permissions() {
    use mkw_arm64::terminal::{contract::Contract, Image};
    use mkw_codegen::guarded::Artifact;
    use mkw_runtime::{
        guarded, guarded_entry, pauth,
        thread_runner::{Range, Storage, Worker},
    };
    let words = [
        0xd503237fu32,
        0xaa1e03f6,
        0xaa0103f3,
        0xd4000181,
        0x35000100,
        0x2a0103f4,
        0xaa1303e1,
        0xd40004a1,
        0x35000080,
        0xaa0103f5,
        0xaa1603fe,
        0xd65f0fff,
        0x14000000,
    ];
    let image = Image::new(0, words.iter().flat_map(|w| w.to_le_bytes()).collect()).unwrap();
    let a = Artifact::new(
        Contract::plain_services(&image, 0, 52).unwrap(),
        "mkw_gv4_thread_queries",
    )
    .unwrap();
    let dir = root().join("private/testing/thread-queries/guarded");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("synthetic.c");
    std::fs::write(&src, a.code()).unwrap();
    for opt in ["-O0", "-O2"] {
        let file = dir.join(format!("synthetic{opt}.so"));
        let c = Command::new("cc")
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
            .arg(root().join("native/pac.c"))
            .arg(root().join("native/pac_profile.c"))
            .arg(root().join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(&file)
            .output()
            .unwrap();
        assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
        let lib = unsafe { libloading::Library::new(file).unwrap() };
        let pac = unsafe {
            pauth::Backend::load(
                &lib,
                pauth::DiagnosticProfile::Legacy39NoTbiQarma5,
                pauth::Keys::new(1, 2, 3, 4),
            )
            .unwrap()
        };
        let target = unsafe {
            guarded::Target::load(
                &lib,
                a.name().as_bytes(),
                a.contract().clone(),
                a.producer(),
                0x400000,
            )
            .unwrap()
        };
        let router = guarded::Router::new(vec![target], 4).unwrap();
        let table = Arc::new(Table::new(2).unwrap());
        let h = ThreadIdentity::register(&table, 44).unwrap();
        let id = table.get::<ThreadIdentity>(h).unwrap().id();
        let services =
            ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table.clone()).registry();
        let denied = services.restricted_to(&BTreeSet::from([GET_PRIORITY]));
        let mut memory = Memory::default();
        for base in [0x8000, 0x9000, 0xa000] {
            memory
                .map(
                    GuestAddr(base),
                    vec![0; 256],
                    Permissions::RW,
                    "synthetic thread storage",
                )
                .unwrap();
        }
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
        for mode in 0..3 {
            for budget in 0..=16 {
                let w = Worker {
                    entry: GuestAddr(0x400000),
                    return_pc: GuestAddr(0x500000),
                    arguments: [
                        0,
                        if mode == 1 { 0 } else { u64::from(h.raw()) },
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    budget,
                    storage: Storage {
                        stack: Range {
                            start: GuestAddr(0x8000),
                            bytes: 256,
                        },
                        tls: Range {
                            start: GuestAddr(0x9000),
                            bytes: 256,
                        },
                        tlr: Range {
                            start: GuestAddr(0xa000),
                            bytes: 256,
                        },
                        tpidr_el0: GuestAddr(0x9000),
                        tpidrro_el0: GuestAddr(0xa000),
                    },
                };
                let out = guarded_entry::run(
                    owned.clone(),
                    &router,
                    if mode == 2 { &denied } else { &services },
                    &pac,
                    w,
                )
                .unwrap();
                let expected = if mode == 2 && budget >= 8 {
                    3
                } else if mode == 1 || budget < 12 {
                    2
                } else {
                    0
                };
                assert_eq!(out.status, expected);
                assert_eq!(
                    out.remaining_budget,
                    if expected == 0 {
                        budget - 12
                    } else if expected == 3 {
                        budget - 8
                    } else {
                        0
                    }
                );
                if out.status == 0 {
                    assert_eq!((out.registers.x[20], out.registers.x[21]), (44, id));
                    assert_eq!(out.registers.pc, 0x500000);
                }
            }
        }
    }
}
