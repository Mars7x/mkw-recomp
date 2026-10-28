use mkw_runtime::{
    arbitration::{Actor, Arbitration, WAITERS},
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
    let dir = root().join("private/testing/arbitration").join(label);
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
fn actor(d: &Arc<Arbitration>, m: &Arc<Mutex<ExclusiveMemory>>, t: &Table) -> Actor {
    let h = ThreadIdentity::register(t, 44).unwrap();
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
struct Stop(Arc<Arbitration>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.shutdown().unwrap();
    }
}
#[test]
fn native_cas_budget_transport_and_handoff_share_monitor_authority() {
    for opt in ["-O0", "-O2"] {
        let lib = build("budgets", opt);
        let r = router(&lib);
        for at in [0x10000, 0x11000] {
            for budget in 0..=3 {
                let m = memory();
                let d = Arbitration::new(m.clone(), 4).unwrap();
                let t = Table::new(4).unwrap();
                let mut a = actor(&d, &m, &t);
                let tag = a.tag();
                let (c, status, left) = run(&mut a, &r, at, 0, tag, budget);
                assert_eq!(status, if budget < 2 { 2 } else { 0 });
                assert_eq!(left, budget.saturating_sub(2));
                assert_eq!(c.base.x[0], 0);
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(0x2000), Width::Word)
                        .unwrap(),
                    if budget == 0 { 0 } else { tag as u128 }
                );
            }
        }
        let m = memory();
        let d = Arbitration::new(m.clone(), 4).unwrap();
        let t = Table::new(4).unwrap();
        let mut a = actor(&d, &m, &t);
        let b = actor(&d, &m, &t);
        let (near, far) = {
            let mut mem = m.lock().unwrap();
            (mem.new_owner().unwrap(), mem.new_owner().unwrap())
        };
        let tag = a.tag();
        assert_eq!(run(&mut a, &r, 0x10000, 0, tag | WAITERS, 2).1, 0);
        {
            let mut mem = m.lock().unwrap();
            mem.load(&near, GuestAddr(0x2000), Width::Word).unwrap();
            mem.load(&far, GuestAddr(0x2040), Width::Word).unwrap();
        }
        let w = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        a.unlock(GuestAddr(0x2000)).unwrap();
        w.wait().unwrap();
        let mut mem = m.lock().unwrap();
        assert!(!mem
            .store_conditional(&near, GuestAddr(0x2000), Width::Word, 0)
            .unwrap());
        assert!(mem
            .store_conditional(&far, GuestAddr(0x2040), Width::Word, 9)
            .unwrap());
    }
}
#[test]
fn native_contention_handoff_and_release_allow_repeated_real_worker_progress() {
    for opt in ["-O0", "-O2"] {
        let lib = build("contention", opt);
        let r = router(&lib);
        let m = memory();
        let d = Arbitration::new(m.clone(), 4).unwrap();
        let t = Table::new(4).unwrap();
        let mut a = actor(&d, &m, &t);
        let mut b = actor(&d, &m, &t);
        for round in 1..=64 {
            let a_tag = a.tag();
            assert_eq!(run(&mut a, &r, 0x10000, 0, a_tag, 2).0.base.x[0], 0);
            std::thread::scope(|scope| {
                // A failure in the controlling closure cancels before scope joins workers.
                struct AbortOnPanic(Arc<Arbitration>);
                impl Drop for AbortOnPanic {
                    fn drop(&mut self) {
                        if std::thread::panicking() {
                            self.0.shutdown().unwrap();
                        }
                    }
                }
                let _abort = AbortOnPanic(d.clone());
                let (tx, rx) = mpsc::channel();
                let b = &mut b;
                let a = &a;
                let r = &r;
                let job = scope.spawn(move || {
                    let tag = b.tag();
                    let (c, status, _) = run(b, r, 0x10000, 0, tag, 2);
                    assert_eq!(status, 0);
                    assert_eq!(c.base.x[0], a_tag as u64);
                    let (c, status, _) = run(b, r, 0x10000, a_tag, a_tag | WAITERS, 2);
                    assert_eq!(status, 0);
                    assert_eq!(c.base.x[0], a_tag as u64);
                    let w = b.prepare_wait(a, GuestAddr(0x2000)).unwrap().unwrap();
                    tx.send(()).unwrap();
                    w.wait().unwrap();
                    assert_eq!(run(b, r, 0x12000, 0, 0, 4).1, 0);
                    let (c, status, _) = run(b, r, 0x11000, tag, 0, 2);
                    assert_eq!(status, 0);
                    assert_eq!(c.base.x[0], tag as u64);
                });
                rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(d.pending(), Ok(1));
                a.unlock(GuestAddr(0x2000)).unwrap();
                job.join().unwrap();
            });
            let mem = m.lock().unwrap();
            assert_eq!(mem.read(GuestAddr(0x2000), Width::Word), Ok(0));
            assert_eq!(mem.read(GuestAddr(0x2004), Width::Word), Ok(round));
        }
        let _stop = Stop(d);
        assert_eq!(
            m.lock().unwrap().read(GuestAddr(0x2004), Width::Word),
            Ok(64)
        );
    }
}
