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
    let dir = root().join("private/testing/actor-queries").join(label);
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

use mkw_runtime::arbitration::{Actor, Arbitration};
fn actor_run(
    actor: &mut Actor,
    r: &Router<'_>,
    s: &Services,
    c: &mut CpuV10,
    at: u64,
    limit: u64,
) -> (i32, u64) {
    let mut left = limit;
    let mut attachment = actor.attach(c).unwrap();
    let status = Routed::attach_services(&mut attachment, r, s)
        .unwrap()
        .run(GuestAddr(at), &mut left)
        .unwrap();
    (status, left)
}
#[test]
fn native_actor_identity_is_pinned_without_reallocating_its_owner() {
    for opt in ["-O0", "-O2"] {
        let lib = build("identity", opt);
        let r = router(&lib);
        let table = Arc::new(Table::new(1).unwrap());
        let h = ThreadIdentity::register(&table, 44).unwrap();
        let object = table.get::<ThreadIdentity>(h).unwrap();
        let id = object.id();
        let weak = Arc::downgrade(&object);
        drop(object);
        let mem = memory();
        let domain = Arbitration::new(mem.clone(), 1).unwrap();
        let mut actor = domain
            .bind(Bridge::new(mem.clone()).unwrap(), &table, h)
            .unwrap();
        let queries = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table.clone());
        let s = queries.registry_for_actor(&actor).unwrap();
        let mut other = Bridge::new(mem.clone()).unwrap();
        assert!(Bridge::new(mem.clone()).is_err());
        actor.set_base_priority(12).unwrap();
        assert_eq!(actor.priorities().unwrap().base, 12);
        table.close(h).unwrap();
        assert!(queries.registry_for_actor(&actor).is_err());
        for (at, value) in [(0x4000, 44), (0x5000, id)] {
            for steps in 0..=3 {
                let mut c = cpu(0xdeadbeefffff8000);
                let before = c.base.x;
                assert_eq!(
                    actor_run(&mut actor, &r, &s, &mut c, at, steps),
                    if steps < 2 { (2, 0) } else { (0, steps - 2) }
                );
                assert_eq!(
                    [c.base.x[0], c.base.x[1]],
                    if steps == 0 {
                        [before[0], before[1]]
                    } else {
                        [0, value]
                    }
                );
                assert_eq!(&c.base.x[2..], &before[2..]);
            }
            let mut c = cpu(0xffff8000);
            let before = c.base.x;
            assert_eq!(run(&mut other, &r, &s, &mut c, at, 2), (3, 1));
            assert_eq!(c.base.x, before);
        }
        let mut c = cpu(h.raw() as u64);
        assert_eq!(actor_run(&mut actor, &r, &s, &mut c, 0x5000, 2), (0, 0));
        assert_eq!(c.base.x[0], INVALID_HANDLE as u64);
        drop(actor);
        let mut replacement = Bridge::new(mem).unwrap();
        let mut c = cpu(0xffff8000);
        assert_eq!(run(&mut replacement, &r, &s, &mut c, 0x5000, 2), (3, 1));
        assert!(weak.upgrade().is_some());
        drop(s);
        assert!(weak.upgrade().is_none());
    }
}
#[test]
fn construction_rejects_wrong_table_shutdown_and_stale_memory() {
    for mode in 0..3 {
        let table = Arc::new(Table::new(1).unwrap());
        let h = ThreadIdentity::register(&table, 10).unwrap();
        let mem = memory();
        let domain = Arbitration::new(mem.clone(), 1).unwrap();
        let actor = domain
            .bind(Bridge::new(mem.clone()).unwrap(), &table, h)
            .unwrap();
        let selected = if mode == 0 {
            Arc::new(Table::new(1).unwrap())
        } else {
            table
        };
        if mode == 1 {
            domain.shutdown().unwrap();
        }
        if mode == 2 {
            *mem.lock().unwrap() = Arc::try_unwrap(memory()).unwrap().into_inner().unwrap();
        }
        let q = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, selected);
        assert!(q.registry_for_actor(&actor).is_err());
    }
}
#[test]
fn separate_actors_cannot_borrow_each_others_current_identity() {
    for opt in ["-O0", "-O2"] {
        let lib = build("two", opt);
        let r = router(&lib);
        let table = Arc::new(Table::new(2).unwrap());
        let h1 = ThreadIdentity::register(&table, 4).unwrap();
        let h2 = ThreadIdentity::register(&table, 8).unwrap();
        let mem = memory();
        let domain = Arbitration::new(mem.clone(), 2).unwrap();
        let mut a = domain
            .bind(Bridge::new(mem.clone()).unwrap(), &table, h1)
            .unwrap();
        let mut b = domain.bind(Bridge::new(mem).unwrap(), &table, h2).unwrap();
        let q = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table);
        let sa = q.registry_for_actor(&a).unwrap();
        let sb = q.registry_for_actor(&b).unwrap();
        for (actor, own, foreign, want) in [(&mut a, &sa, &sb, 4), (&mut b, &sb, &sa, 8)] {
            let mut c = cpu(0xffff8000);
            assert_eq!(actor_run(actor, &r, own, &mut c, 0x4000, 2), (0, 0));
            assert_eq!(c.base.x[1], want);
            let mut c = cpu(0xffff8000);
            let old = c.base.x;
            assert_eq!(actor_run(actor, &r, foreign, &mut c, 0x4000, 2), (3, 1));
            assert_eq!(c.base.x, old);
        }
    }
}
#[test]
fn actor_query_wrapper_retains_completed_identity_on_store_fault() {
    for opt in ["-O0", "-O2"] {
        let lib = build("wrapper", opt);
        let r = router(&lib);
        for output in [0x2100, 0x3000] {
            for steps in 0..=7 {
                let table = Arc::new(Table::new(1).unwrap());
                let h = ThreadIdentity::register(&table, 9).unwrap();
                let id = table.get::<ThreadIdentity>(h).unwrap().id();
                let mem = memory();
                let domain = Arbitration::new(mem.clone(), 1).unwrap();
                let mut actor = domain
                    .bind(Bridge::new(mem.clone()).unwrap(), &table, h)
                    .unwrap();
                let s = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table)
                    .registry_for_actor(&actor)
                    .unwrap();
                let mut c = cpu(0xffff8000);
                c.base.x[0] = output;
                assert_eq!(
                    actor_run(&mut actor, &r, &s, &mut c, 0x8000, steps).0,
                    if steps < 4 {
                        2
                    } else if output == 0x3000 {
                        5
                    } else if steps < 6 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(c.base.x[1], if steps >= 2 { id } else { 0xffff8000 });
                assert_eq!(
                    mem.lock()
                        .unwrap()
                        .read(GuestAddr(0x2100), mkw_runtime::exclusive::Width::Double)
                        .unwrap(),
                    if output == 0x2100 && steps >= 4 {
                        id as u128
                    } else {
                        0
                    }
                );
            }
        }
    }
}
#[test]
fn guarded_pac_entry_uses_the_same_actor_query_identity() {
    use mkw_arm64::terminal::{contract::Contract, Image};
    use mkw_codegen::guarded::Artifact;
    use mkw_runtime::{
        guarded,
        pauth::{Backend, DiagnosticProfile, Keys},
        thread_runner::{Range, Storage, Worker},
    };
    let words = [0xd503237fu32, 0xd40004a1, 0xd65f0fff];
    let image = Image::new(0, words.iter().flat_map(|w| w.to_le_bytes()).collect()).unwrap();
    let artifact = Artifact::new(
        Contract::plain_services(&image, 0, 12).unwrap(),
        "mkw_gv4_actor_identity",
    )
    .unwrap();
    let dir = root().join("private/testing/actor-queries/guarded");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("synthetic.c");
    std::fs::write(&src, artifact.code()).unwrap();
    for opt in ["-O0", "-O2"] {
        let file = dir.join(format!("synthetic{opt}.so"));
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
            .arg(root().join("native/pac.c"))
            .arg(root().join("native/pac_profile.c"))
            .arg(root().join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let lib = unsafe { libloading::Library::new(file).unwrap() };
        let pac = unsafe {
            Backend::load(
                &lib,
                DiagnosticProfile::Legacy39NoTbiQarma5,
                Keys::new(1, 2, 3, 4),
            )
            .unwrap()
        };
        let target = unsafe {
            guarded::Target::load(
                &lib,
                artifact.name().as_bytes(),
                artifact.contract().clone(),
                artifact.producer(),
                0x400000,
            )
            .unwrap()
        };
        let router = guarded::Router::new(vec![target], 1).unwrap();
        let mut raw = Memory::default();
        for base in [0x2000, 0x3000, 0x4000] {
            raw.map(
                GuestAddr(base),
                vec![0; 4096],
                Permissions::RW,
                "synthetic storage",
            )
            .unwrap();
        }
        let mem = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let table = Arc::new(Table::new(1).unwrap());
        let h = ThreadIdentity::register(&table, 44).unwrap();
        let id = table.get::<ThreadIdentity>(h).unwrap().id();
        let domain = Arbitration::new(mem.clone(), 1).unwrap();
        let mut actor = domain
            .bind(Bridge::new(mem.clone()).unwrap(), &table, h)
            .unwrap();
        let services = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table)
            .registry_for_actor(&actor)
            .unwrap();
        assert!(Bridge::new(mem).is_err());
        for budget in 0..=4 {
            let worker = Worker {
                entry: GuestAddr(0x400000),
                return_pc: GuestAddr(0x500000),
                arguments: [123, 0xffff8000, 0, 0, 0, 0, 0, 0],
                budget,
                storage: Storage {
                    stack: Range {
                        start: GuestAddr(0x2000),
                        bytes: 4096,
                    },
                    tls: Range {
                        start: GuestAddr(0x3000),
                        bytes: 4096,
                    },
                    tlr: Range {
                        start: GuestAddr(0x4000),
                        bytes: 4096,
                    },
                    tpidr_el0: GuestAddr(0x3000),
                    tpidrro_el0: GuestAddr(0x4000),
                },
            };
            let out = actor.run_guarded(&router, &services, &pac, worker).unwrap();
            assert_eq!(out.status, if budget < 3 { 2 } else { 0 });
            assert_eq!(
                [out.registers.x[0], out.registers.x[1]],
                if budget >= 2 {
                    [0, id]
                } else {
                    [123, 0xffff8000]
                }
            );
            assert_eq!(out.registers.sp, 0x3000);
            assert_eq!(out.remaining_budget, budget.saturating_sub(3));
        }
    }
}
