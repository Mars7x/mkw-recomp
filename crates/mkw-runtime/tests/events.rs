//! Author-created event protocol fixtures; these are not Horizon SVC mappings.
use mkw_runtime::{
    events::{Error, Events, Handle},
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
struct Adapter {
    events: Arc<Events>,
    op: u32,
}
impl Service for Adapter {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        let h = Handle::from_raw(a[0] as u32);
        let mut o = Outputs::default();
        let result = match self.op {
            0 => self.events.create().map(|e| {
                o = Outputs::default()
                    .write_w(1, e.write.raw())
                    .unwrap()
                    .write_w(2, e.read.raw())
                    .unwrap();
            }),
            1 => self.events.signal(h),
            2 => self.events.clear(h),
            3 => self.events.reset(h),
            4 => self.events.close(h),
            _ => self.events.poll(&[h]).map(|index| {
                o = Outputs::default()
                    .write_w(1, index.map(|i| i as u32).unwrap_or(u32::MAX))
                    .unwrap();
            }),
        };
        // Fixture-specific result codes, deliberately not Horizon constants.
        let code = match result {
            Ok(()) => 0,
            Err(Error::InvalidHandle) => 1,
            Err(Error::AlreadyClear) => 2,
            Err(Error::Quota) => 3,
            Err(_) => return Err(HostFault::unsupported()),
        };
        Ok(Reply::Return(o.write_w(0, code)?))
    }
}
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/events")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for n in 0..6 {
        let words = [0xd4000001u32 | ((0x700 + n) << 5), 0xd65f03c0];
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        source += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", &format!("event_{n}"));
    }
    let src = path.join("synthetic.c");
    let lib = path.join(format!("synthetic{opt}.so"));
    std::fs::write(&src, source).unwrap();
    let built = Command::new("cc")
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
    std::fs::write(path.join(format!("compiler{opt}.log")), &built.stderr).unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        (0..6)
            .map(|n| {
                let name = format!("event_{n}\0");
                let at = 0x4000 + n * 0x1000;
                unsafe { Target::load_services(lib, name.as_bytes(), GuestAddr(at), at).unwrap() }
            })
            .collect(),
        8,
    )
    .unwrap()
}
fn service(events: &Arc<Events>) -> Services {
    Services::new(
        (0..6)
            .map(|op| {
                (
                    0x700 + op as u16,
                    Arc::new(Adapter {
                        events: events.clone(),
                        op,
                    }) as Arc<dyn Service>,
                )
            })
            .collect(),
    )
    .unwrap()
}
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
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
                max_owners: 4,
            },
        )
        .unwrap(),
    ))
}
fn call(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    op: u64,
    handle: u64,
) -> [u64; 3] {
    let mut cpu = CpuV10::default();
    cpu.base.x[0] = handle;
    cpu.base.x[1] = 0x1111111111111111;
    cpu.base.x[2] = 0x2222222222222222;
    cpu.base.x[30] = 0x9000;
    let mut budget = 10;
    {
        let mut a = bridge.attach(&mut cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        assert_eq!(
            r.run(GuestAddr(0x4000 + op * 0x1000), &mut budget).unwrap(),
            0
        );
    }
    assert_eq!(budget, 8);
    [cpu.base.x[0], cpu.base.x[1], cpu.base.x[2]]
}
#[test]
fn native_event_create_signal_poll_reset_close_obey_endpoint_rights_and_lifetimes() {
    let path = folder("lifetime");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let events = Arc::new(Events::new(1).unwrap());
        let svc = service(&events);
        let mut bridge = Bridge::new(memory()).unwrap();
        let pair = call(&mut bridge, &router, &svc, 0, 0);
        assert_eq!(pair[0], 0);
        let (write, read) = (pair[1], pair[2]);
        assert_eq!(call(&mut bridge, &router, &svc, 0, 0)[0], 3);
        assert_eq!(
            call(&mut bridge, &router, &svc, 5, read)[1],
            u32::MAX as u64
        );
        assert_eq!(call(&mut bridge, &router, &svc, 1, read)[0], 1);
        assert_eq!(call(&mut bridge, &router, &svc, 1, write)[0], 0);
        assert_eq!(call(&mut bridge, &router, &svc, 5, read)[1], 0);
        assert_eq!(call(&mut bridge, &router, &svc, 5, read)[1], 0);
        assert_eq!(call(&mut bridge, &router, &svc, 3, read)[0], 0);
        assert_eq!(call(&mut bridge, &router, &svc, 3, read)[0], 2);
        call(&mut bridge, &router, &svc, 1, write);
        call(&mut bridge, &router, &svc, 4, write);
        assert_eq!(call(&mut bridge, &router, &svc, 5, read)[1], 0);
        assert_eq!(call(&mut bridge, &router, &svc, 0, 0)[0], 3);
        call(&mut bridge, &router, &svc, 4, read);
        let next = call(&mut bridge, &router, &svc, 0, 0);
        assert_eq!(next[0], 0);
        assert_ne!(next[1], write);
        assert_eq!(call(&mut bridge, &router, &svc, 4, read)[0], 1);
    }
}
#[test]
fn concurrent_native_event_cycles_keep_contexts_and_object_identities_separate() {
    let path = folder("parallel");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let events = Arc::new(Events::new(4).unwrap());
        let svc = service(&events);
        let memory = memory();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..4 {
                let svc = &svc;
                let router = &router;
                let memory = memory.clone();
                handles.push(scope.spawn(move || {
                    let mut bridge = Bridge::new(memory).unwrap();
                    let mut ids = Vec::new();
                    for _ in 0..100 {
                        let pair = call(&mut bridge, router, svc, 0, 0);
                        assert_eq!(pair[0], 0);
                        let (write, read) = (pair[1], pair[2]);
                        ids.extend([write, read]);
                        assert_eq!(call(&mut bridge, router, svc, 1, write)[0], 0);
                        assert_eq!(call(&mut bridge, router, svc, 5, read)[1], 0);
                        assert_eq!(call(&mut bridge, router, svc, 2, read)[0], 0);
                        assert_eq!(call(&mut bridge, router, svc, 5, read)[1], u32::MAX as u64);
                        assert_eq!(call(&mut bridge, router, svc, 4, read)[0], 0);
                        assert_eq!(call(&mut bridge, router, svc, 4, write)[0], 0);
                    }
                    ids
                }));
            }
            let ids: std::collections::BTreeSet<_> = handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect();
            assert_eq!(ids.len(), 800);
        });
    }
}
