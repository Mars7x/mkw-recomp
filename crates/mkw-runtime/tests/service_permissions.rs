use mkw_runtime::{
    event_services::{Abi, EventServices, CLEAR, CLOSE, CREATE, RESET, SIGNAL},
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
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/service-permissions");
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
#[test]
fn explicit_service_restriction_cannot_add_handlers_or_allocate_through_denied_calls() {
    use std::collections::BTreeSet;
    let path = folder();
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let backend = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let all = backend.registry();
        let none = all.restricted_to(&BTreeSet::new());
        let create = all.restricted_to(&BTreeSet::from([CREATE, 0xffff]));
        let narrowed = create.restricted_to(&BTreeSet::from([SIGNAL]));
        let unknown = Services::new(vec![])
            .unwrap()
            .restricted_to(&BTreeSet::from([CREATE]));
        let mut bridge = Bridge::new(memory(1)).unwrap();
        for services in [&none, &narrowed, &unknown] {
            for budget in [0, 1, 2] {
                let mut cpu = CpuV10::default();
                cpu.base.x = [0x123456789abcdef0; 31];
                cpu.base.x[30] = 0x9000;
                let before = cpu.base.x;
                let mut left = budget;
                let status = {
                    let mut a = bridge.attach(&mut cpu).unwrap();
                    let mut r = Routed::attach_services(&mut a, &router, services).unwrap();
                    r.run(GuestAddr(0x4000), &mut left).unwrap()
                };
                assert_eq!(status, if budget == 0 { 2 } else { 3 });
                assert_eq!(left, budget.saturating_sub(1));
                assert_eq!(cpu.base.x, before);
                // Repeated denied create calls must not consume the sole object quota.
                let ends = backend.events().create().unwrap();
                backend.events().close(ends.read).unwrap();
                backend.events().close(ends.write).unwrap();
            }
        }
        let mut cpu = CpuV10::default();
        cpu.base.x[30] = 0x9000;
        let mut budget = 2;
        {
            let mut a = bridge.attach(&mut cpu).unwrap();
            let mut r = Routed::attach_services(&mut a, &router, &create).unwrap();
            assert_eq!(r.run(GuestAddr(0x4000), &mut budget).unwrap(), 0);
        }
        assert_eq!((budget, cpu.base.x[0]), (0, 0));
        // Views share real state but cannot expand the original registry.
        assert_eq!(
            backend.events().create().err(),
            Some(mkw_runtime::events::Error::Quota)
        );
        backend
            .events()
            .close(Handle::from_raw(cpu.base.x[1] as u32))
            .unwrap();
        backend
            .events()
            .close(Handle::from_raw(cpu.base.x[2] as u32))
            .unwrap();
    }
}
