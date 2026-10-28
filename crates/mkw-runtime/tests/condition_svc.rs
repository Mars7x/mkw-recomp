use mkw_runtime::{
    arbitration::{
        svc::{
            condition::{Abi, TIMED_OUT},
            INVALID_ADDRESS, INVALID_CURRENT_MEMORY,
        },
        Actor, Arbitration, Policy,
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
        ("wait", vec![0xd4000381u32, 0xd65f03c0]),
        ("signal", vec![0xd40003a1u32, 0xd65f03c0]),
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
    let dir = root().join("private/testing/condition-svc").join(label);
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
    a.condition_services(Abi::Nx64OwnedUntimedCondition39, t.clone())
        .unwrap()
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x10000, b"wait\0".as_slice()),
            (0x13000, b"signal\0".as_slice()),
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
    args: [u64; 4],
    budget: u64,
) -> (CpuV10, i32, u64) {
    let mut cpu = CpuV10::default();
    cpu.base.x = [0x9988776655443322; 31];
    cpu.base.x[..4].copy_from_slice(&args);
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
fn exact_wait_zero_signal_void_and_refusal_budget_packets() {
    for opt in ["-O0", "-O2"] {
        let lib = build("packets", opt);
        let r = router(&lib);
        let (m, d, t) = fixture();
        let mut a = actor(&m, &d, &t, 10);
        let b = actor(&m, &d, &t, 50);
        let svc = registry(&a, &t);
        let foreign = registry(&b, &t);
        let denied = svc.restricted_to(&Default::default());
        for (at, args, result, word, key) in [
            (
                0x10000,
                [BASE, BASE + 0x83, 0xfeedcafeffffffff, 0],
                Some(TIMED_OUT),
                0,
                1,
            ),
            (
                0x10000,
                [BASE + 1, BASE + 0x80, 0, 0],
                Some(INVALID_ADDRESS),
                0xabcdef,
                0,
            ),
            (
                0x10000,
                [BASE + 0x2000, BASE + 0x80, 0, 0],
                Some(INVALID_CURRENT_MEMORY),
                0xabcdef,
                1,
            ),
            (0x10000, [BASE, BASE + 0x2000, 0, 0], Some(TIMED_OUT), 0, 0),
            (0x10000, [BASE, BASE + 0x5000, 0, 0], Some(TIMED_OUT), 0, 0),
            (0x10000, [BASE, BASE + 0x80, 0, 1], None, 0xabcdef, 0),
            (
                0x10000,
                [BASE, BASE + 0x80, 0, u64::MAX - 1],
                None,
                0xabcdef,
                0,
            ),
            (0x10000, [BASE, BASE + 0x80, 0, u64::MAX], None, 0xabcdef, 0),
            (0x10000, [1 << 39, BASE + 0x80, 0, 0], None, 0xabcdef, 0),
            (0x13000, [1 << 39, 0, 0, 0], None, 0xabcdef, 0),
        ] {
            for budget in 0..=3 {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &0xabcdefu32.to_le_bytes())
                    .unwrap();
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE + 0x80), &0u32.to_le_bytes())
                    .unwrap();
                let (cpu, status, left) = run(&mut a, &r, &svc, at, args, budget);
                let executed = budget > 0;
                assert_eq!(
                    status,
                    if !executed {
                        2
                    } else if result.is_none() {
                        3
                    } else if budget < 2 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(
                    left,
                    if !executed {
                        0
                    } else if result.is_none() {
                        budget - 1
                    } else {
                        budget.saturating_sub(2)
                    }
                );
                assert_eq!(
                    cpu.base.x[0],
                    if executed {
                        result.map(u64::from).unwrap_or(args[0])
                    } else {
                        args[0]
                    }
                );
                assert_eq!(cpu.base.x[1..4], args[1..]);
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(BASE), Width::Word)
                        .unwrap(),
                    if executed { word } else { 0xabcdef }
                );
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(BASE + 0x80), Width::Word)
                        .unwrap(),
                    if executed { key } else { 0 }
                );
            }
        }
        for count in [0, 1, u64::MAX, 0xabcdef0000000001] {
            for budget in 0..=3 {
                let args = [BASE + 0x83, count, 0x123456789, 0x987654321];
                let (cpu, status, _) = run(&mut a, &r, &svc, 0x13000, args, budget);
                assert_eq!(status, if budget < 2 { 2 } else { 0 });
                assert_eq!(cpu.base.x[..4], args);
            }
        }
        for rejected in [&foreign, &denied] {
            let args = [BASE, BASE + 0x80, 0, 0];
            let (cpu, status, _) = run(&mut a, &r, rejected, 0x10000, args, 2);
            assert_eq!(status, 3);
            assert_eq!(cpu.base.x[..4], args);
        }
    }
}
#[test]
fn native_indefinite_wait_signal_and_alias_owner_reacquisition() {
    for opt in ["-O0", "-O2"] {
        let lib = build("blocking", opt);
        let r = router(&lib);
        for owned in [false, true] {
            for _ in 0..16 {
                let (m, d, t) = fixture();
                let mut a = actor(&m, &d, &t, 10);
                let mut b = actor(&m, &d, &t, 50);
                let tag = a.tag();
                let sa = registry(&a, &t);
                let sb = registry(&b, &t)
                    .combined(
                        &b.arbitration_services(
                            mkw_runtime::arbitration::svc::Abi::Nx64OwnedArbitration39,
                            t.clone(),
                        )
                        .unwrap(),
                    )
                    .unwrap();
                let alias = t
                    .import(
                        t.export(mkw_runtime::kernel_handles::Handle::from_raw(b.tag()))
                            .unwrap(),
                    )
                    .unwrap();
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &tag.to_le_bytes())
                    .unwrap();
                std::thread::scope(|scope| {
                    let _abort = Abort(d.clone());
                    let r = &r;
                    let wait = scope.spawn(move || {
                        let (cpu, status, left) = run(
                            &mut a,
                            r,
                            &sa,
                            0x10000,
                            [BASE, BASE + 0x83, 0xbeef00000000 | tag as u64, u64::MAX],
                            2,
                        );
                        (cpu.base.x, status, left)
                    });
                    pending(&d, 1);
                    if owned {
                        m.lock()
                            .unwrap()
                            .write(GuestAddr(BASE), &alias.raw().to_le_bytes())
                            .unwrap();
                    }
                    let args = [BASE + 0x81, u64::MAX, 0x1234, 0x9876];
                    let (cpu, status, _) = run(&mut b, r, &sb, 0x13000, args, 2);
                    assert_eq!(status, 0);
                    assert_eq!(cpu.base.x[..4], args);
                    if owned {
                        assert_eq!(d.pending(), Ok(1));
                        assert_eq!(b.priorities().unwrap().effective, 10);
                        assert_eq!(run(&mut b, r, &sb, 0x11000, [BASE, 0, 0, 0], 2).1, 0);
                    }
                    let (regs, status, left) = wait.join().unwrap();
                    assert_eq!((regs[0], status, left), (0, 0, 0));
                    assert_eq!(regs[1], BASE + 0x83);
                    assert_eq!(
                        m.lock()
                            .unwrap()
                            .read(GuestAddr(BASE), Width::Word)
                            .unwrap(),
                        tag as u128
                    );
                    assert_eq!(d.pending(), Ok(0));
                    assert_eq!(b.priorities().unwrap().effective, 50);
                });
            }
        }
    }
}
#[test]
fn native_condition_shutdown_preserves_inputs_without_fake_termination() {
    for opt in ["-O0", "-O2"] {
        let lib = build("shutdown", opt);
        let r = router(&lib);
        let (m, d, t) = fixture();
        let mut a = actor(&m, &d, &t, 10);
        let tag = a.tag();
        let svc = registry(&a, &t);
        m.lock()
            .unwrap()
            .write(GuestAddr(BASE), &tag.to_le_bytes())
            .unwrap();
        let args = [BASE, BASE + 0x80, tag as u64, u64::MAX];
        std::thread::scope(|scope| {
            let _abort = Abort(d.clone());
            let r = &r;
            let wait = scope.spawn(move || {
                let (cpu, status, _) = run(&mut a, r, &svc, 0x10000, args, 2);
                (cpu.base.x, status)
            });
            pending(&d, 1);
            d.shutdown().unwrap();
            let (regs, status) = wait.join().unwrap();
            assert_eq!(status, 3);
            assert_eq!(regs[..4], args);
        });
    }
}
#[test]
fn blocked_wait_error_and_ignored_key_fault_keep_exact_budget_completion() {
    for opt in ["-O0", "-O2"] {
        let lib = build("completion", opt);
        let r = router(&lib);
        for invalid in [false, true] {
            for budget in [1, 2] {
                let (m, d, t) = fixture();
                let mut a = actor(&m, &d, &t, 10);
                let mut b = actor(&m, &d, &t, 50);
                let tag = a.tag();
                let sa = registry(&a, &t);
                let sb = registry(&b, &t);
                let key = if invalid { BASE + 0x80 } else { BASE + 0x2000 };
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &tag.to_le_bytes())
                    .unwrap();
                std::thread::scope(|scope| {
                    let _abort = Abort(d.clone());
                    let r = &r;
                    let waiting = scope.spawn(move || {
                        let (cpu, status, left) = run(
                            &mut a,
                            r,
                            &sa,
                            0x10000,
                            [BASE, key, tag as u64, u64::MAX],
                            budget,
                        );
                        (cpu.base.x, status, left)
                    });
                    pending(&d, 1);
                    if invalid {
                        m.lock()
                            .unwrap()
                            .write(GuestAddr(BASE), &0xdeadu32.to_le_bytes())
                            .unwrap();
                    }
                    let args = [key, 0, 0x12345678, 0x87654321];
                    let (cpu, status, _) = run(&mut b, r, &sb, 0x13000, args, 2);
                    assert_eq!(status, 0);
                    assert_eq!(cpu.base.x[..4], args);
                    let (regs, status, left) = waiting.join().unwrap();
                    assert_eq!(
                        regs[0],
                        if invalid {
                            mkw_runtime::arbitration::svc::condition::INVALID_STATE as u64
                        } else {
                            0
                        }
                    );
                    assert_eq!(status, if budget == 1 { 2 } else { 0 });
                    assert_eq!(left, 0);
                    assert_eq!(d.pending(), Ok(0));
                });
            }
        }
    }
}
#[test]
fn native_broadcast_completes_only_after_each_mutex_reacquisition() {
    for opt in ["-O0", "-O2"] {
        let lib = build("broadcast", opt);
        let r = router(&lib);
        for count in [0, u64::MAX] {
            let (m, d, t) = fixture();
            let mut a = actor(&m, &d, &t, 10);
            let mut b = actor(&m, &d, &t, 30);
            let mut c = actor(&m, &d, &t, 50);
            let ta = a.tag();
            let tb = b.tag();
            let sc = registry(&c, &t);
            let sa = registry(&a, &t)
                .combined(
                    &a.arbitration_services(
                        mkw_runtime::arbitration::svc::Abi::Nx64OwnedArbitration39,
                        t.clone(),
                    )
                    .unwrap(),
                )
                .unwrap();
            let sb = registry(&b, &t);
            m.lock()
                .unwrap()
                .write(GuestAddr(BASE), &ta.to_le_bytes())
                .unwrap();
            std::thread::scope(|scope| {
                let _abort = Abort(d.clone());
                let r = &r;
                let (ready_tx, ready_rx) = mpsc::channel();
                let (go_tx, go_rx) = mpsc::channel();
                let first = scope.spawn(move || {
                    let (cpu, status, _) = run(
                        &mut a,
                        r,
                        &sa,
                        0x10000,
                        [BASE, BASE + 0x80, ta as u64, u64::MAX],
                        2,
                    );
                    assert_eq!((status, cpu.base.x[0]), (0, 0));
                    ready_tx.send(()).unwrap();
                    go_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    assert_eq!(run(&mut a, r, &sa, 0x11000, [BASE, 0, 0, 0], 2).1, 0);
                });
                pending(&d, 1);
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &tb.to_le_bytes())
                    .unwrap();
                let second = scope.spawn(move || {
                    let (cpu, status, _) = run(
                        &mut b,
                        r,
                        &sb,
                        0x10000,
                        [BASE, BASE + 0x80, tb as u64, u64::MAX],
                        2,
                    );
                    (cpu.base.x[0], status)
                });
                pending(&d, 2);
                let args = [BASE + 0x83, count, 77, 88];
                let (cpu, status, _) = run(&mut c, r, &sc, 0x13000, args, 2);
                assert_eq!(status, 0);
                assert_eq!(cpu.base.x[..4], args);
                ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                assert_eq!(d.pending(), Ok(1));
                go_tx.send(()).unwrap();
                first.join().unwrap();
                assert_eq!(second.join().unwrap(), (0, 0));
                assert_eq!(d.pending(), Ok(0));
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(BASE), Width::Word)
                        .unwrap(),
                    tb as u128
                );
            });
        }
    }
}
