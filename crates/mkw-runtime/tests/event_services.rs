use mkw_runtime::{
    event_services::{
        Abi, EventServices, CLEAR, CLOSE, CREATE, INVALID_HANDLE, INVALID_STATE, LIMIT_REACHED,
        RESET, SIGNAL,
    },
    events::Handle,
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/event-services");
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (n, number) in [CREATE, SIGNAL, CLEAR, RESET, CLOSE, 0x18]
        .into_iter()
        .enumerate()
    {
        let words = [0xd4000001u32 | (u32::from(number) << 5), 0xd65f03c0];
        let bytes = words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        source += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", &format!("kernel_event_{n}"));
    }
    let src = path.join("synthetic.c");
    std::fs::write(&src, source).unwrap();
    let lib = path.join(format!("synthetic{opt}.so"));
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
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    std::fs::write(path.join(format!("compiler{opt}.log")), &out.stderr).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        (0..6)
            .map(|n| {
                let name = format!("kernel_event_{n}\0");
                let at = 0x4000 + n * 0x1000;
                unsafe { Target::load_services(lib, name.as_bytes(), GuestAddr(at), at).unwrap() }
            })
            .collect(),
        8,
    )
    .unwrap()
}
fn memory(owners: usize) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 4096],
        Permissions::RW,
        "author-created buffer",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: owners,
            },
        )
        .unwrap(),
    ))
}
fn call_with_budget(
    bridge: &mut Bridge,
    router: &Router<'_>,
    services: &Services,
    operation: u64,
    value: u64,
    budget: u64,
) -> (i32, [u64; 3], u64) {
    let mut cpu = CpuV10::default();
    cpu.base.x[0] = value;
    cpu.base.x[1] = 0x1111222233334444;
    cpu.base.x[2] = 0x5555666677778888;
    cpu.base.x[30] = 0x9000;
    let mut b = budget;
    let status = {
        let mut a = bridge.attach(&mut cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, services).unwrap();
        r.run(GuestAddr(0x4000 + operation * 0x1000), &mut b)
            .unwrap()
    };
    (status, [cpu.base.x[0], cpu.base.x[1], cpu.base.x[2]], b)
}
fn call(
    bridge: &mut Bridge,
    router: &Router<'_>,
    services: &Services,
    operation: u64,
    value: u64,
) -> (i32, [u64; 3]) {
    let (status, registers, remaining) =
        call_with_budget(bridge, router, services, operation, value, 10);
    assert_eq!(remaining, if status == 0 { 8 } else { 9 });
    (status, registers)
}
#[test]
fn explicit_event_svc_reference_profile_transports_real_state_and_refuses_unmodeled_calls() {
    assert_eq!(
        [CREATE, SIGNAL, CLEAR, RESET, CLOSE],
        [0x45, 0x11, 0x12, 0x17, 0x16]
    );
    let path = folder();
    for opt in ["-O0", "-O2"] {
        let library = build(&path, opt);
        let router = router(&library);
        let backend = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let services = backend.registry();
        let mut m = Memory::default();
        m.map(
            GuestAddr(0x1000),
            vec![0; 4096],
            Permissions::RW,
            "synthetic memory",
        )
        .unwrap();
        let memory = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                m,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let mut bridge = Bridge::new(memory).unwrap();
        let (status, pair) = call(&mut bridge, &router, &services, 0, 0);
        assert_eq!(status, 0);
        assert_eq!(pair[0], 0);
        let (write, read) = (pair[1], pair[2]);
        assert_ne!(write, read);
        let quota = call(&mut bridge, &router, &services, 0, 0);
        assert_eq!(
            quota,
            (
                0,
                [
                    u64::from(LIMIT_REACHED),
                    0x1111222233334444u64,
                    0x5555666677778888
                ]
            )
        );
        assert_eq!(LIMIT_REACHED, 0x10801);
        assert_eq!(INVALID_HANDLE, 0xe401);
        assert_eq!(INVALID_STATE, 0xfa01);
        assert_eq!(
            call(&mut bridge, &router, &services, 1, read).1[0],
            u64::from(INVALID_HANDLE)
        );
        assert_eq!(
            call(&mut bridge, &router, &services, 3, read).1[0],
            u64::from(INVALID_STATE)
        );
        assert_eq!(call(&mut bridge, &router, &services, 1, write).1[0], 0);
        assert_eq!(
            backend.events().poll(&[Handle::from_raw(read as u32)]),
            Ok(Some(0))
        );
        assert_eq!(call(&mut bridge, &router, &services, 2, read).1[0], 0);
        assert_eq!(
            call(&mut bridge, &router, &services, 3, read).1[0],
            u64::from(INVALID_STATE)
        );
        assert_eq!(
            call(&mut bridge, &router, &services, 1, write | (u64::MAX << 32)).1[0],
            0
        );
        assert_eq!(call(&mut bridge, &router, &services, 3, read).1[0], 0);
        assert_eq!(call(&mut bridge, &router, &services, 4, write).1[0], 0);
        assert_eq!(
            call(&mut bridge, &router, &services, 0, 0).1[0],
            u64::from(LIMIT_REACHED)
        );
        assert_eq!(call(&mut bridge, &router, &services, 4, read).1[0], 0);
        assert_eq!(
            call(&mut bridge, &router, &services, 4, read).1[0],
            u64::from(INVALID_HANDLE)
        );
        let next = call(&mut bridge, &router, &services, 0, 0);
        assert_eq!(next.1[0], 0);
        assert_ne!(next.1[1], write);
        assert_eq!(call(&mut bridge, &router, &services, 5, 0).0, 3);
        assert_eq!(call(&mut bridge, &router, &services, 3, 0xffff8001).0, 3);
    }
}
#[test]
fn event_svc_registration_namespace_and_budget_boundaries_preserve_real_effects() {
    let path = folder().join("boundaries");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let a = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let b = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let registry = a.registry();
        let foreign = b.registry();
        let mut bridge = Bridge::new(memory(1)).unwrap();
        assert_eq!(
            call_with_budget(&mut bridge, &router, &registry, 0, 777, 0),
            (2, [777, 0x1111222233334444, 0x5555666677778888], 0)
        );
        let created = call_with_budget(&mut bridge, &router, &registry, 0, 0, 1);
        assert_eq!(created.0, 2);
        assert_eq!(created.1[0], 0);
        assert_eq!(created.2, 0);
        let (write, read) = (created.1[1], created.1[2]);
        assert_eq!(
            call(&mut bridge, &router, &registry, 0, 0).1[0],
            u64::from(LIMIT_REACHED)
        );
        assert_eq!(
            call(&mut bridge, &router, &foreign, 1, write).1[0],
            u64::from(INVALID_HANDLE)
        );
        assert_eq!(
            call(&mut bridge, &router, &foreign, 4, read).1[0],
            u64::from(INVALID_HANDLE)
        );
        let clone = a.clone().registry();
        assert_eq!(call(&mut bridge, &router, &clone, 1, write).1[0], 0);
        assert_eq!(call(&mut bridge, &router, &registry, 3, read).1[0], 0);
        for h in [write, read] {
            assert_eq!(call(&mut bridge, &router, &registry, 4, h).1[0], 0);
        }
        let empty = Services::new(vec![]).unwrap();
        assert_eq!(
            call(&mut bridge, &router, &empty, 0, 777),
            (3, [777, 0x1111222233334444, 0x5555666677778888])
        );
        assert_eq!(call(&mut bridge, &router, &registry, 0, 0).1[0], 0);
        assert!(EventServices::new(Abi::Horizon64EventReference, 0).is_err());
        assert!(EventServices::new(Abi::Horizon64EventReference, 1025).is_err());
    }
}
#[test]
fn concurrent_event_svc_clients_share_one_process_state_with_distinct_frames() {
    let path = folder().join("concurrent");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let backend = EventServices::new(Abi::Horizon64EventReference, 4).unwrap();
        let registry = backend.registry();
        let memory = memory(4);
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..4 {
                let router = &router;
                let registry = &registry;
                let memory = memory.clone();
                handles.push(scope.spawn(move || {
                    let mut bridge = Bridge::new(memory).unwrap();
                    let mut ids = Vec::new();
                    for _ in 0..32 {
                        let (status, created) = call(&mut bridge, router, registry, 0, 0);
                        assert_eq!((status, created[0]), (0, 0));
                        let (write, read) = (created[1], created[2]);
                        ids.extend([write, read]);
                        assert_eq!(call(&mut bridge, router, registry, 1, write).1[0], 0);
                        assert_eq!(call(&mut bridge, router, registry, 3, read).1[0], 0);
                        assert_eq!(
                            call(&mut bridge, router, registry, 3, read).1[0],
                            u64::from(INVALID_STATE)
                        );
                        for h in [read, write] {
                            assert_eq!(call(&mut bridge, router, registry, 4, h).1[0], 0);
                        }
                    }
                    ids
                }));
            }
            let ids: std::collections::BTreeSet<_> = handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect();
            assert_eq!(ids.len(), 256);
        });
        let mut guards = Vec::new();
        for _ in 0..4 {
            guards.push(Bridge::new(memory.clone()).unwrap());
        }
    }
}
