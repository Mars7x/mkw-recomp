use mkw_runtime::{
    arbitration::{
        svc::{Abi, INVALID_ADDRESS, INVALID_CURRENT_MEMORY, INVALID_HANDLE, LOCK, UNLOCK},
        Actor, Arbitration, Policy, WAITERS,
    },
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
const BASE: u64 = 0x1_0000_2000;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        ("lock", vec![0xd4000341u32, 0xd65f03c0]),
        ("unlock", vec![0xd4000361u32, 0xd65f03c0]),
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
    let dir = root().join("private/testing/arbitration-svc").join(label);
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

fn fixture() -> (Arc<Mutex<ExclusiveMemory>>, Arc<Arbitration>, Arc<Table>) {
    let mut raw = Memory::default();
    raw.map(
        GuestAddr(BASE),
        vec![0; 4096],
        Permissions::RW,
        "mutex and counter",
    )
    .unwrap();
    raw.map(
        GuestAddr(BASE + 0x2000),
        vec![0; 4],
        Permissions::R,
        "readonly mutex",
    )
    .unwrap();
    let m = Arc::new(Mutex::new(
        ExclusiveMemory::new(
            raw,
            Config {
                granule: 16,
                max_owners: 8,
            },
        )
        .unwrap(),
    ));
    let d = Arbitration::with_policy(m.clone(), 8, Policy::InheritedPriority).unwrap();
    (m, d, Arc::new(Table::new(8).unwrap()))
}
fn actor(m: &Arc<Mutex<ExclusiveMemory>>, d: &Arc<Arbitration>, t: &Table, p: u8) -> Actor {
    let h = ThreadIdentity::register(t, p).unwrap();
    d.bind(Bridge::new(m.clone()).unwrap(), t, h).unwrap()
}
fn registry(a: &Actor, t: &Arc<Table>) -> Services {
    a.arbitration_services(Abi::Nx64OwnedArbitration39, t.clone())
        .unwrap()
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x10000, b"lock\0".as_slice()),
            (0x11000, b"unlock\0".as_slice()),
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
    svc: &Services,
    at: u64,
    args: [u64; 3],
    budget: u64,
) -> (CpuV10, i32, u64) {
    let mut cpu = CpuV10::default();
    cpu.base.x = [0x9988776655443322; 31];
    cpu.base.x[..3].copy_from_slice(&args);
    cpu.base.x[30] = 0x90000;
    let mut left = budget;
    let status = {
        let mut attachment = a.attach(&mut cpu).unwrap();
        let mut route = Routed::attach_services(&mut attachment, r, svc).unwrap();
        route.run(GuestAddr(at), &mut left).unwrap()
    };
    (cpu, status, left)
}
fn pending(d: &Arbitration, n: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    while d.pending().unwrap() != n {
        assert!(Instant::now() < end, "wait registration deadline");
        std::thread::yield_now();
    }
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
fn exact_native_packets_widths_budgets_errors_and_denied_bindings() {
    assert_eq!((LOCK, UNLOCK), (0x1a, 0x1b));
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, d, t) = fixture();
        let mut a = actor(&m, &d, &t, 50);
        let b = actor(&m, &d, &t, 10);
        let svc = registry(&a, &t);
        let other = registry(&b, &t);
        let denied = svc.restricted_to(&Default::default());
        let hi = 0xfeedcafe00000000;
        for (at, args, word, result, clears) in [
            (0x10000, [hi | 0xffff, BASE, hi | 7], 0, Some(0), false),
            (
                0x10000,
                [hi | 0xffff, BASE, hi | 7],
                0xffff | WAITERS,
                Some(INVALID_HANDLE),
                false,
            ),
            (
                0x10000,
                [a.tag() as u64, BASE + 1, 0],
                0,
                Some(INVALID_ADDRESS),
                false,
            ),
            (
                0x10000,
                [0, BASE + 0x1000, 0],
                0,
                Some(INVALID_CURRENT_MEMORY),
                false,
            ),
            (
                0x10000,
                [b.tag() as u64, BASE, 7],
                b.tag() | WAITERS,
                None,
                false,
            ),
            (
                0x10000,
                [a.tag() as u64, BASE, a.tag() as u64],
                a.tag() | WAITERS,
                None,
                false,
            ),
            (0x10000, [0, 1 << 50, 0], 0, None, false),
            (0x11000, [BASE, 0, 0], 0x87654321, Some(0), true),
            (
                0x11000,
                [BASE + 0x2000, 0, 0],
                0,
                Some(INVALID_CURRENT_MEMORY),
                false,
            ),
            (
                0x11000,
                [BASE + 0x1000, 0, 0],
                0,
                Some(INVALID_CURRENT_MEMORY),
                false,
            ),
            (0x11000, [BASE + 1, 0, 0], 0, Some(INVALID_ADDRESS), false),
            (0x11000, [1 << 50, 0, 0], 0, None, false),
        ] {
            for budget in 0..=3 {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &word.to_le_bytes())
                    .unwrap();
                let (cpu, status, left) = run(&mut a, &r, &svc, at, args, budget);
                let expected_status = if budget == 0 {
                    2
                } else if result.is_none() {
                    3
                } else if budget == 1 {
                    2
                } else {
                    0
                };
                assert_eq!(status, expected_status);
                assert_eq!(
                    left,
                    if budget == 0 {
                        0
                    } else if result.is_none() {
                        budget - 1
                    } else {
                        budget.saturating_sub(2)
                    }
                );
                assert_eq!(
                    cpu.base.x[0],
                    if budget > 0 {
                        result.map(u64::from).unwrap_or(args[0])
                    } else {
                        args[0]
                    }
                );
                assert_eq!(&cpu.base.x[1..3], &args[1..]);
                assert!(cpu.base.x[3..30].iter().all(|&v| v == 0x9988776655443322));
                assert_eq!(cpu.base.x[30], 0x90000);
                assert_eq!(
                    m.lock().unwrap().read(GuestAddr(BASE), Width::Word),
                    Ok(if clears && budget > 0 {
                        0
                    } else {
                        word as u128
                    })
                );
                assert_eq!(d.pending(), Ok(0));
            }
        }

        let unbound = ThreadIdentity::register(&t, 44).unwrap();
        m.lock()
            .unwrap()
            .write(GuestAddr(BASE), &(unbound.raw() | WAITERS).to_le_bytes())
            .unwrap();
        let a_tag = a.tag();
        let (cpu, status, _) = run(
            &mut a,
            &r,
            &svc,
            0x10000,
            [unbound.raw() as u64, BASE, a_tag as u64],
            2,
        );
        assert_eq!(status, 3);
        assert_eq!(cpu.base.x[0], unbound.raw() as u64);
        assert_eq!(d.pending(), Ok(0));
        for s in [&denied, &other] {
            m.lock()
                .unwrap()
                .write(GuestAddr(BASE), &9u32.to_le_bytes())
                .unwrap();
            let (c, status, left) = run(&mut a, &r, s, 0x11000, [BASE, 77, 88], 2);
            assert_eq!((status, left), (3, 1));
            assert_eq!(c.base.x[0], BASE);
            assert_eq!(m.lock().unwrap().read(GuestAddr(BASE), Width::Word), Ok(9));
        }
    }
}
#[test]
fn native_blocking_svcs_donate_handoff_and_leave_other_workers_runnable() {
    for opt in ["-O0", "-O2"] {
        let lib = build("blocking", opt);
        let r = router(&lib);
        for _ in 0..16 {
            let (m, d, t) = fixture();
            let mut a = actor(&m, &d, &t, 50);
            let mut b = actor(&m, &d, &t, 30);
            let mut c = actor(&m, &d, &t, 10);
            let sa = registry(&a, &t);
            let sb = registry(&b, &t);
            let sc = registry(&c, &t);
            let owner = a.tag();
            m.lock()
                .unwrap()
                .write(GuestAddr(BASE), &(owner | WAITERS).to_le_bytes())
                .unwrap();
            std::thread::scope(|scope| {
                let _abort = Abort(d.clone());
                let (tx, rx) = mpsc::channel();
                let r = &r;
                let txb = tx.clone();
                let first = scope.spawn(move || {
                    let args = [
                        0xfeedcafe00000000 | owner as u64,
                        BASE,
                        0x1122334400000000 | b.tag() as u64,
                    ];
                    let (out, status, left) = run(&mut b, r, &sb, 0x10000, args, 2);
                    assert_eq!((status, left, out.base.x[0]), (0, 0, 0));
                    assert_eq!(&out.base.x[1..3], &args[1..]);
                    assert_eq!(run(&mut b, r, &sb, 0x12000, [0, 0, BASE], 4).1, 0);
                    txb.send(1).unwrap();
                    assert_eq!(run(&mut b, r, &sb, 0x11000, [BASE, 0, 0], 2).1, 0);
                });
                pending(&d, 1);
                assert_eq!(a.priorities().unwrap().effective, 30);
                let second = scope.spawn(move || {
                    let tag = c.tag();
                    let (out, status, _) =
                        run(&mut c, r, &sc, 0x10000, [owner as u64, BASE, tag as u64], 2);
                    assert_eq!((status, out.base.x[0]), (0, 0));
                    assert_eq!(run(&mut c, r, &sc, 0x12000, [0, 0, BASE], 4).1, 0);
                    tx.send(2).unwrap();
                    assert_eq!(run(&mut c, r, &sc, 0x11000, [BASE, 0, 0], 2).1, 0);
                });
                pending(&d, 2);
                assert_eq!(a.priorities().unwrap().effective, 10);
                let (out, status, left) = run(&mut a, r, &sa, 0x11000, [BASE, 0, 0], 2);
                assert_eq!((out.base.x[0], status, left), (0, 0, 0));
                assert_eq!(a.priorities().unwrap().effective, 50);
                assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
                assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
                first.join().unwrap();
                second.join().unwrap();
            });
            assert_eq!(d.pending(), Ok(0));
            let mem = m.lock().unwrap();
            assert_eq!(mem.read(GuestAddr(BASE), Width::Word), Ok(0));
            assert_eq!(mem.read(GuestAddr(BASE + 4), Width::Word), Ok(2));
        }
    }
}
#[test]
fn native_wait_teardown_refuses_without_fabricating_termination_results() {
    for opt in ["-O0", "-O2"] {
        let lib = build("teardown", opt);
        let r = router(&lib);
        for shutdown in [false, true] {
            let (m, d, t) = fixture();
            let a = actor(&m, &d, &t, 50);
            let mut b = actor(&m, &d, &t, 10);
            let svc = registry(&b, &t);
            let args = [a.tag() as u64, BASE, b.tag() as u64];
            m.lock()
                .unwrap()
                .write(GuestAddr(BASE), &(a.tag() | WAITERS).to_le_bytes())
                .unwrap();
            std::thread::scope(|scope| {
                let _abort = Abort(d.clone());
                let r = &r;
                let job = scope.spawn(move || {
                    let (cpu, status, left) = run(&mut b, r, &svc, 0x10000, args, 2);
                    (cpu.base.x, status, left)
                });
                pending(&d, 1);
                if shutdown {
                    d.shutdown().unwrap();
                } else {
                    drop(a);
                }
                let (out, status, left) = job.join().unwrap();
                assert_eq!((status, left), (3, 1));
                assert_eq!(&out[..3], &args);
                assert_eq!(d.pending(), Ok(0));
            });
        }
    }
}

#[test]
fn native_unlock_write_fault_completes_both_svc_frames_with_the_reference_error() {
    for opt in ["-O0", "-O2"] {
        let lib = build("write-fault", opt);
        let r = router(&lib);
        for budget in [1, 2] {
            let t = Arc::new(Table::new(2).unwrap());
            let ha = ThreadIdentity::register(&t, 50).unwrap();
            let hb = ThreadIdentity::register(&t, 10).unwrap();
            let mut raw = Memory::default();
            raw.map(GuestAddr(BASE), vec![0; 4096], Permissions::RW, "data")
                .unwrap();
            raw.map(
                GuestAddr(BASE + 0x2000),
                (ha.raw() | WAITERS).to_le_bytes().to_vec(),
                Permissions::R,
                "readonly mutex",
            )
            .unwrap();
            let m = Arc::new(Mutex::new(
                ExclusiveMemory::new(
                    raw,
                    Config {
                        granule: 16,
                        max_owners: 3,
                    },
                )
                .unwrap(),
            ));
            let d = Arbitration::with_policy(m.clone(), 2, Policy::InheritedPriority).unwrap();
            let mut a = d.bind(Bridge::new(m.clone()).unwrap(), &t, ha).unwrap();
            let mut b = d.bind(Bridge::new(m.clone()).unwrap(), &t, hb).unwrap();
            let sa = registry(&a, &t);
            let sb = registry(&b, &t);
            let observer = {
                let mut mem = m.lock().unwrap();
                let observer = mem.new_owner().unwrap();
                mem.load(&observer, GuestAddr(BASE), Width::Word).unwrap();
                observer
            };
            std::thread::scope(|scope| {
                let _abort = Abort(d.clone());
                let r = &r;
                let job = scope.spawn(move || {
                    let (cpu, status, left) = run(
                        &mut b,
                        r,
                        &sb,
                        0x10000,
                        [ha.raw() as u64, BASE + 0x2000, hb.raw() as u64],
                        budget,
                    );
                    (cpu.base.x, status, left)
                });
                pending(&d, 1);
                let (cpu, status, left) = run(&mut a, r, &sa, 0x11000, [BASE + 0x2000, 77, 88], 2);
                assert_eq!(
                    (cpu.base.x[0], status, left),
                    (INVALID_CURRENT_MEMORY as u64, 0, 0)
                );
                assert_eq!(&cpu.base.x[1..3], &[77, 88]);
                let (regs, status, left) = job.join().unwrap();
                assert_eq!(
                    (regs[0], status, left),
                    (
                        INVALID_CURRENT_MEMORY as u64,
                        if budget == 1 { 2 } else { 0 },
                        0
                    )
                );
                assert_eq!(&regs[1..3], &[BASE + 0x2000, hb.raw() as u64]);
            });
            assert_eq!(d.pending(), Ok(0));
            assert_eq!(a.priorities().unwrap().effective, 50);
            let mut mem = m.lock().unwrap();
            assert_eq!(
                mem.read(GuestAddr(BASE + 0x2000), Width::Word),
                Ok((ha.raw() | WAITERS) as u128)
            );
            assert!(mem
                .store_conditional(&observer, GuestAddr(BASE), Width::Word, 7)
                .unwrap());
        }
    }
}
