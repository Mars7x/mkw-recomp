use mkw_runtime::{
    arbitration::{Actor, Arbitration, Policy},
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
    time::{Duration, Instant},
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
    let dir = root().join("private/testing/condition-native").join(label);
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
struct Abort(Arc<Arbitration>);
impl Drop for Abort {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.shutdown().unwrap();
        }
    }
}
#[test]
fn native_condition_wait_reacquires_before_protected_work_and_retains_early_signal() {
    for opt in ["-O0", "-O2"] {
        let lib = build("handoff", opt);
        let r = router(&lib);
        for early in [false, true] {
            for _ in 0..16 {
                let m = memory();
                let d = Arbitration::with_policy(m.clone(), 2, Policy::InheritedPriority).unwrap();
                let t = Table::new(2).unwrap();
                let mut waiter = actor(&d, &m, &t, 10);
                let mut signaller = actor(&d, &m, &t, 50);
                let tag = waiter.tag();
                let signal_tag = signaller.tag();
                assert_eq!(run(&mut waiter, &r, 0x10000, 0, tag, 2).1, 0);
                std::thread::scope(|scope| {
                    let _abort = Abort(d.clone());
                    let (ready_tx, ready_rx) = mpsc::channel();
                    let (go_tx, go_rx) = mpsc::channel();
                    let r = &r;
                    let worker = scope.spawn(move || {
                        let wait = waiter
                            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                            .unwrap();
                        ready_tx.send(()).unwrap();
                        if early {
                            go_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                        }
                        wait.wait().unwrap();
                        let (cpu, status, left) = run(&mut waiter, r, 0x12000, 0, 0, 4);
                        assert_eq!((status, left), (0, 0));
                        waiter.unlock(GuestAddr(0x2000)).unwrap();
                        cpu.base.x[0]
                    });
                    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    if !early {
                        assert_eq!(run(&mut signaller, r, 0x10000, 0, signal_tag, 2).1, 0);
                        assert_eq!(signaller.signal_condition(GuestAddr(0x2080), 1), Ok(1));
                        assert_eq!(d.pending(), Ok(1));
                        assert_eq!(signaller.priorities().unwrap().effective, 10);
                        let (cpu, status, _) = run(&mut signaller, r, 0x12000, 0, 0, 4);
                        assert_eq!((cpu.base.x[0], status), (1, 0));
                        signaller.unlock(GuestAddr(0x2000)).unwrap();
                        assert_eq!(signaller.priorities().unwrap().effective, 50);
                    } else {
                        assert_eq!(signaller.signal_condition(GuestAddr(0x2080), 0), Ok(1));
                        assert_eq!(d.pending(), Ok(0)); // Completion before host wait() starts.
                        go_tx.send(()).unwrap();
                    }
                    assert_eq!(worker.join().unwrap(), if early { 1 } else { 2 });
                    assert_eq!(d.pending(), Ok(0));
                    assert_eq!(
                        m.lock()
                            .unwrap()
                            .read(GuestAddr(0x2000), Width::Word)
                            .unwrap(),
                        0
                    );
                });
            }
        }
    }
}
#[test]
fn signal_after_observed_native_mutex_release_never_misses_registration() {
    for opt in ["-O0", "-O2"] {
        let lib = build("release-race", opt);
        let r = router(&lib);
        for _ in 0..32 {
            let m = memory();
            let d = Arbitration::with_policy(m.clone(), 2, Policy::InheritedPriority).unwrap();
            let t = Table::new(2).unwrap();
            let mut waiter = actor(&d, &m, &t, 10);
            let signaller = actor(&d, &m, &t, 50);
            let tag = waiter.tag();
            assert_eq!(run(&mut waiter, &r, 0x10000, 0, tag, 2).1, 0);
            std::thread::scope(|scope| {
                let _abort = Abort(d.clone());
                let r = &r;
                let worker = scope.spawn(move || {
                    waiter
                        .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                        .unwrap()
                        .wait()
                        .unwrap();
                    let (cpu, status, _) = run(&mut waiter, r, 0x12000, 0, 0, 4);
                    assert_eq!(status, 0);
                    waiter.unlock(GuestAddr(0x2000)).unwrap();
                    cpu.base.x[0]
                });
                let deadline = Instant::now() + Duration::from_secs(5);
                while m
                    .lock()
                    .unwrap()
                    .read(GuestAddr(0x2000), Width::Word)
                    .unwrap()
                    != 0
                {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                assert_eq!(signaller.signal_condition(GuestAddr(0x2080), 1), Ok(1));
                assert_eq!(worker.join().unwrap(), 1);
                assert_eq!(d.pending(), Ok(0));
            });
        }
    }
}
