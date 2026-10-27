use mkw_runtime::{
    event_services::{Abi, EventServices, INVALID_HANDLE, OUT_OF_RANGE, TIMED_OUT, WAIT},
    exclusive::{Config, ExclusiveMemory, Width},
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
fn folder(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/event-poll")
        .join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut text = String::new();
    for (name, words) in [
        (
            "poll",
            vec![0xd4000001u32 | (u32::from(WAIT) << 5), 0xd65f03c0],
        ),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("store", vec![0xc807fca6, 0xd65f03c0]),
    ] {
        let bytes = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        text += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let src = path.join("synthetic.c");
    std::fs::write(&src, text).unwrap();
    let lib = path.join(format!("synthetic{opt}.so"));
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
        [
            (0x4000, b"poll\0".as_slice()),
            (0x5000, b"reserve\0".as_slice()),
            (0x6000, b"store\0".as_slice()),
        ]
        .into_iter()
        .map(|(a, n)| unsafe { Target::load_services(lib, n, GuestAddr(a), a).unwrap() })
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
        "author-created counter/array",
    )
    .unwrap();
    let bytes = array
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    m.map(
        GuestAddr(0x5000),
        bytes,
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
fn run(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    cpu: &mut CpuV10,
    at: u64,
    budget: u64,
) -> (i32, u64) {
    let mut b = budget;
    let status = {
        let mut a = bridge.attach(cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        r.run(GuestAddr(at), &mut b).unwrap()
    };
    (status, b)
}
fn cpu(pointer: u64, count: u64, timeout: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0x1111222233334444;
    c.base.x[1] = pointer;
    c.base.x[2] = count;
    c.base.x[3] = timeout;
    c.base.x[30] = 0x9000;
    c.base.sp = 0x2800;
    c
}
#[test]
fn zero_timeout_event_svc_polls_validate_all_handles_and_preserve_manual_signal_state() {
    assert_eq!((WAIT, TIMED_OUT, OUT_OF_RANGE), (0x18, 0xea01, 0xee01));
    let path = folder("poll");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let backend = EventServices::new(Abi::Horizon64EventReference, 4).unwrap();
        let first = backend.events().create().unwrap();
        let second = backend.events().create().unwrap();
        backend.events().signal(second.write).unwrap();
        let svc = backend.registry_with_poll();
        let other = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let foreign = other.events().create().unwrap();
        let mut last = vec![first.read.raw(); 63];
        last.push(second.read.raw());
        let arrays = [
            (last, 64, 0, 63),
            (vec![second.read.raw(); 64], 64, 0, 0),
            (vec![foreign.read.raw()], 1, INVALID_HANDLE as u64, 0x5000),
            (vec![first.read.raw()], 1, TIMED_OUT as u64, 0x5000),
            (vec![first.read.raw(), second.read.raw()], 2, 0, 1),
            (vec![second.read.raw(), second.read.raw()], 2, 0, 0),
            (vec![second.read.raw(), 0], 2, INVALID_HANDLE as u64, 0x5000),
            (vec![second.write.raw()], 1, INVALID_HANDLE as u64, 0x5000),
        ];
        for (array, count, result, index) in arrays {
            let mem = memory(&array);
            let mut bridge = Bridge::new(mem).unwrap();
            let mut c = cpu(0x5000, count, 0);
            let input = c.base.x;
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
            assert_eq!(c.base.x[0], result);
            assert_eq!(c.base.x[1], index);
            assert_eq!(&c.base.x[2..], &input[2..]);
            assert_eq!(c.base.sp, 0x2800);
        }
        assert_eq!(backend.events().poll(&[second.read]), Ok(Some(0)));
        for (count, result) in [
            (0, TIMED_OUT as u64),
            (0xdeadbeef00000000, TIMED_OUT as u64),
            (65, OUT_OF_RANGE as u64),
            (u32::MAX as u64, OUT_OF_RANGE as u64),
        ] {
            let mut bridge = Bridge::new(memory(&[])).unwrap();
            let mut c = cpu(u64::MAX, count, 0);
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
            assert_eq!(c.base.x[0], result);
            assert_eq!(c.base.x[1], u64::MAX);
        }
        let mut bridge = Bridge::new(memory(&[second.read.raw()])).unwrap();
        let mut c = cpu(0x5000, 0xdeadbeef00000001, 0);
        assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
        assert_eq!(&c.base.x[..2], &[0, 0]);
        let mut c = cpu(u64::MAX, 1, 0);
        let before = c.base.x;
        assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (5, 9));
        assert_eq!(c.base.x, before);
        assert_eq!(c.base.fault_address, u64::MAX);
        backend.events().close(second.read).unwrap();
        let mut c = cpu(0x5000, 1, 0);
        assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
        assert_eq!(c.base.x[0], INVALID_HANDLE as u64);
    }
}
#[test]
fn poll_only_binding_preserves_refusals_budgets_and_exception_monitor_contract() {
    let path = folder("boundary");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let backend = EventServices::new(Abi::Horizon64EventReference, 1).unwrap();
        let pair = backend.events().create().unwrap();
        let svc = backend.registry_with_poll();
        for timeout in [0, 1, u64::MAX] {
            let mem = memory(&[pair.read.raw()]);
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut c = cpu(0x5000, 1, timeout);
            c.base.x[5] = 0x2000;
            c.base.x[6] = 99;
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x5000, 10), (0, 8));
            let initial = c.base.x;
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 0x4000, 10),
                if timeout == 0 { (0, 8) } else { (3, 9) }
            );
            if timeout != 0 {
                assert_eq!(c.base.x, initial);
            } else {
                assert_eq!(c.base.x[0], TIMED_OUT as u64);
            }
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x6000, 10), (0, 8));
            assert_eq!(c.base.x[7], if timeout == 0 { 1 } else { 0 });
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x2000), Width::Double)
                    .unwrap(),
                if timeout == 0 { 0 } else { 99 }
            );
        }
        let mut bridge = Bridge::new(memory(&[pair.read.raw()])).unwrap();
        let legacy = backend.registry();
        let mut c = cpu(0x5000, 1, 0);
        let initial = c.base.x;
        assert_eq!(
            run(&mut bridge, &router, &legacy, &mut c, 0x4000, 10),
            (3, 9)
        );
        assert_eq!(c.base.x, initial);
        for budget in 0..3 {
            let mut c = cpu(0x5000, 1, 0);
            let initial = c.base.x;
            let status = run(&mut bridge, &router, &svc, &mut c, 0x4000, budget);
            assert_eq!(status, if budget < 2 { (2, 0) } else { (0, 0) });
            assert_eq!(
                c.base.x[0],
                if budget == 0 {
                    initial[0]
                } else {
                    TIMED_OUT as u64
                }
            );
        }
    }
}
