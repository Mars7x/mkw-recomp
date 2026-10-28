use mkw_runtime::{
    arbitration::{Actor, Arbitration, Policy, WAITERS},
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    kernel_handles::Table,
    services::Services,
    thread_queries::ThreadIdentity,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        ("acquire", vec![0x88e07c41u32, 0xd65f03c0]),
        ("release", vec![0x88a0fc41u32, 0xd65f03c0]),
        (
            "increment",
            vec![0xb9400440u32, 0x11000400, 0xb9000440, 0xd65f03c0],
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
    let dir = root().join("private/testing/priority-native").join(label);
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

fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x2000),
        vec![0; 4096],
        Permissions::RW,
        "lock and counter",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 16,
            },
        )
        .unwrap(),
    ))
}
fn actor(d: &Arc<Arbitration>, m: &Arc<Mutex<ExclusiveMemory>>, t: &Table, priority: u8) -> Actor {
    let h = ThreadIdentity::register(t, priority).unwrap();
    d.bind(Bridge::new(m.clone()).unwrap(), t, h).unwrap()
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x10000, b"acquire\0".as_slice()),
            (0x11000, b"release\0".as_slice()),
            (0x12000, b"increment\0".as_slice()),
        ]
        .into_iter()
        .map(|(at, n)| unsafe { Target::load_services(lib, n, GuestAddr(at), at).unwrap() })
        .collect(),
        4,
    )
    .unwrap()
}
fn run(
    a: &mut Actor,
    r: &Router<'_>,
    at: u64,
    expected: u32,
    new: u32,
    budget: u64,
) -> (CpuV10, i32, u64) {
    let mut c = CpuV10::default();
    c.base.x[0] = expected as u64;
    c.base.x[1] = new as u64;
    c.base.x[2] = 0x2000;
    c.base.x[30] = 0x30000;
    let mut left = budget;
    let status = {
        let mut att = a.attach(&mut c).unwrap();
        let services = Services::new(vec![]).unwrap();
        let mut rt = Routed::attach_services(&mut att, r, &services).unwrap();
        rt.run(GuestAddr(at), &mut left).unwrap()
    };
    (c, status, left)
}

#[test]
fn native_contenders_handoff_by_donated_priority_and_restore_old_owner() {
    for opt in ["-O0", "-O2"] {
        let lib = build("handoff", opt);
        let r = router(&lib);
        for _ in 0..16 {
            let m = memory();
            let d = Arbitration::with_policy(m.clone(), 3, Policy::InheritedPriority).unwrap();
            let t = Table::new(3).unwrap();
            let mut a = actor(&d, &m, &t, 50);
            let mut b = actor(&d, &m, &t, 30);
            let mut c = actor(&d, &m, &t, 10);
            let owner = a.tag();
            assert_eq!(run(&mut a, &r, 0x10000, 0, owner, 2).1, 0);
            std::thread::scope(|scope| {
                struct Abort(Arc<Arbitration>);
                impl Drop for Abort {
                    fn drop(&mut self) {
                        if std::thread::panicking() {
                            self.0.shutdown().unwrap();
                        }
                    }
                }
                let _abort = Abort(d.clone());
                let (ready, rx) = mpsc::channel();
                let (order, order_rx) = mpsc::channel();
                let a_ref = &a;
                let r = &r;
                let tx = ready.clone();
                let record = order.clone();
                let first = scope.spawn(move || {
                    let tag = b.tag();
                    let (out, status, _) = run(&mut b, r, 0x10000, 0, tag, 2);
                    assert_eq!((out.base.x[0], status), (owner as u64, 0));
                    assert_eq!(run(&mut b, r, 0x10000, owner, owner | WAITERS, 2).1, 0);
                    let w = b.prepare_wait(a_ref, GuestAddr(0x2000)).unwrap().unwrap();
                    tx.send(()).unwrap();
                    w.wait().unwrap();
                    assert_eq!(run(&mut b, r, 0x12000, 0, 0, 4).1, 0);
                    record.send(1).unwrap();
                    let (out, status, _) = run(&mut b, r, 0x11000, tag, 0, 2);
                    assert_eq!((out.base.x[0], status), (tag as u64, 0));
                });
                rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(a.priorities().unwrap().effective, 30);
                let second = scope.spawn(move || {
                    let tag = c.tag();
                    let (out, status, _) = run(&mut c, r, 0x10000, 0, tag, 2);
                    assert_eq!((out.base.x[0], status), ((owner | WAITERS) as u64, 0));
                    let w = c.prepare_wait(a_ref, GuestAddr(0x2000)).unwrap().unwrap();
                    ready.send(()).unwrap();
                    w.wait().unwrap();
                    assert_eq!(run(&mut c, r, 0x12000, 0, 0, 4).1, 0);
                    order.send(2).unwrap();
                    c.unlock(GuestAddr(0x2000)).unwrap();
                });
                rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(a.priorities().unwrap().effective, 10);
                a.unlock(GuestAddr(0x2000)).unwrap();
                assert_eq!(a.priorities().unwrap().effective, 50);
                assert_eq!(order_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
                assert_eq!(order_rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
                first.join().unwrap();
                second.join().unwrap();
            });
            let memory = m.lock().unwrap();
            assert_eq!(memory.read(GuestAddr(0x2000), Width::Word), Ok(0));
            assert_eq!(memory.read(GuestAddr(0x2004), Width::Word), Ok(2));
        }
    }
}
