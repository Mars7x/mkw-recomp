// Authored synthetic integration only. No original input is loaded or executed.
use mkw_arm64::terminal::{contract::Contract, Image};
use mkw_codegen::guarded::Artifact;
use mkw_runtime::{
    arbitration::{svc::Abi, Actor, Arbitration, Policy, WAITERS},
    exclusive::{Config, ExclusiveMemory},
    exclusive_native::Bridge,
    guarded::{Router, Target},
    kernel_handles::Table,
    pauth::{Backend, DiagnosticProfile, Keys},
    services::Services,
    thread_queries::ThreadIdentity,
    thread_runner::{Range, Storage, Worker},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
fn run(
    a: &mut Actor,
    r: &Router<'_>,
    s: &Services,
    p: &Backend<'_>,
    w: Worker,
) -> ([u64; 31], u64, i32, u64) {
    let sp = w.storage.stack.start.0 + w.storage.stack.bytes as u64;
    let outcome = a.run_guarded(r, s, p, w).unwrap();
    assert_eq!(outcome.registers.sp, sp);
    assert_eq!(
        (outcome.registers.tpidr_el0, outcome.registers.tpidrro_el0),
        (0xa000, 0xb000)
    );
    (
        outcome.registers.x,
        outcome.registers.pc,
        outcome.status,
        outcome.remaining_budget,
    )
}
fn with_fixture(name: &str, check: impl Fn(&Router<'_>, &Backend<'_>)) {
    let words = [
        0xd503237fu32,
        0xd4000341,
        0xd65f0fff,
        0xd503201f,
        0xd503237f,
        0xd4000361,
        0xd65f0fff,
        0xd503201f,
        0xd4000361, // Completed unlock survives the wrong return below.
        0xd280001e,
        0xd65f03c0,
    ];
    let image = Image::new(0, words.iter().flat_map(|w| w.to_le_bytes()).collect()).unwrap();
    let artifacts = [
        Artifact::new(
            Contract::plain_services(&image, 0, 12).unwrap(),
            "mkw_gv4_synthetic_lock",
        )
        .unwrap(),
        Artifact::new(
            Contract::plain_services(&image, 16, 12).unwrap(),
            "mkw_gv4_synthetic_unlock",
        )
        .unwrap(),
        Artifact::new(
            Contract::plain_services(&image, 32, 12).unwrap(),
            "mkw_gv4_wrong_return",
        )
        .unwrap(),
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("private/testing/actor-entry").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("synthetic.c");
    std::fs::write(
        &src,
        artifacts
            .iter()
            .map(|a| a.code())
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    for opt in ["-O0", "-O2"] {
        let file = dir.join(format!("synthetic{opt}.so"));
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
            .arg(root.join("native"))
            .arg(&src)
            .arg(root.join("native/pac.c"))
            .arg(root.join("native/pac_profile.c"))
            .arg(root.join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            cc.status.success(),
            "{}",
            String::from_utf8_lossy(&cc.stderr)
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
        let r = Router::new(
            artifacts
                .iter()
                .map(|a| unsafe {
                    Target::load(
                        &lib,
                        a.name().as_bytes(),
                        a.contract().clone(),
                        a.producer(),
                        0x400000,
                    )
                    .unwrap()
                })
                .collect(),
            4,
        )
        .unwrap();
        check(&r, &pac);
    }
}
#[test]
fn actor_entry_composes_blocking_arbitration_pac_and_exact_budgets() {
    with_fixture("blocking", |r, pac| {
        for budget in 0..=4 {
            let mut raw = Memory::default();
            for base in [0x2000, 0x7000, 0x8000, 0xa000, 0xb000] {
                raw.map(
                    GuestAddr(base),
                    vec![0; 4096],
                    Permissions::RW,
                    "explicit synthetic storage",
                )
                .unwrap();
            }
            let m = Arc::new(Mutex::new(
                ExclusiveMemory::new(
                    raw,
                    Config {
                        granule: 16,
                        max_owners: 2,
                    },
                )
                .unwrap(),
            ));
            let d = Arbitration::with_policy(m.clone(), 2, Policy::InheritedPriority).unwrap();
            let t = Arc::new(Table::new(2).unwrap());
            let ha = ThreadIdentity::register(&t, 50).unwrap();
            let hb = ThreadIdentity::register(&t, 10).unwrap();
            let mut a = d.bind(Bridge::new(m.clone()).unwrap(), &t, ha).unwrap();
            let mut b = d.bind(Bridge::new(m.clone()).unwrap(), &t, hb).unwrap();
            let sa = a
                .arbitration_services(Abi::Nx64OwnedArbitration39, t.clone())
                .unwrap();
            let sb = b
                .arbitration_services(Abi::Nx64OwnedArbitration39, t)
                .unwrap();
            m.lock()
                .unwrap()
                .write(GuestAddr(0x2000), &(ha.raw() | WAITERS).to_le_bytes())
                .unwrap();
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
                let waiter = scope.spawn(move || {
                    run(
                        &mut b,
                        r,
                        &sb,
                        pac,
                        worker(
                            0x400000,
                            [ha.raw() as u64, 0x2000, hb.raw() as u64],
                            budget,
                            0x9000,
                        ),
                    )
                });
                if budget >= 2 {
                    let end = Instant::now() + Duration::from_secs(5);
                    while d.pending().unwrap() != 1 {
                        assert!(Instant::now() < end);
                        std::thread::yield_now();
                    }
                }
                let (regs, pc, status, left) = run(
                    &mut a,
                    r,
                    &sa,
                    pac,
                    worker(0x400010, [0x2000, 0, 0], 3, 0x8000),
                );
                assert_eq!((regs[0], pc, status, left), (0, 0x500000, 0, 0));
                let (regs, pc, status, left) = waiter.join().unwrap();
                assert_eq!(status, if budget < 3 { 2 } else { 0 });
                assert_eq!(left, budget.saturating_sub(3));
                assert_eq!(regs[0], if budget < 2 { ha.raw() as u64 } else { 0 });
                if budget >= 3 {
                    assert_eq!(pc, 0x500000);
                }
                assert_eq!(d.pending().unwrap(), 0);
            });
        }
    });
}

fn worker(at: u64, args: [u64; 3], budget: u64, sp: u64) -> Worker {
    let mut arguments = [0; 8];
    arguments[..3].copy_from_slice(&args);
    Worker {
        entry: GuestAddr(at),
        return_pc: GuestAddr(0x500000),
        arguments,
        budget,
        storage: Storage {
            stack: Range {
                start: GuestAddr(sp - 4096),
                bytes: 4096,
            },
            tls: Range {
                start: GuestAddr(0xa000),
                bytes: 4096,
            },
            tlr: Range {
                start: GuestAddr(0xb000),
                bytes: 4096,
            },
            tpidr_el0: GuestAddr(0xa000),
            tpidrro_el0: GuestAddr(0xb000),
        },
    }
}

fn storage_memory(readonly: bool) -> ExclusiveMemory {
    let mut raw = Memory::default();
    for base in [0x2000, 0x7000, 0x8000, 0xa000, 0xb000] {
        raw.map(
            GuestAddr(base),
            vec![0x5a; 4096],
            if readonly && base == 0xb000 {
                Permissions::R
            } else {
                Permissions::RW
            },
            "synthetic explicit storage",
        )
        .unwrap();
    }
    ExclusiveMemory::new(
        raw,
        Config {
            granule: 16,
            max_owners: 2,
        },
    )
    .unwrap()
}
struct Environment {
    memory: Arc<Mutex<ExclusiveMemory>>,
    domain: Arc<Arbitration>,
    table: Arc<Table>,
    actor: Actor,
    services: Services,
}
impl Environment {
    fn new(readonly: bool) -> Self {
        let memory = Arc::new(Mutex::new(storage_memory(readonly)));
        let domain =
            Arbitration::with_policy(memory.clone(), 2, Policy::InheritedPriority).unwrap();
        let table = Arc::new(Table::new(2).unwrap());
        let handle = ThreadIdentity::register(&table, 20).unwrap();
        let actor = domain
            .bind(Bridge::new(memory.clone()).unwrap(), &table, handle)
            .unwrap();
        let services = actor
            .arbitration_services(Abi::Nx64OwnedArbitration39, table.clone())
            .unwrap();
        Self {
            memory,
            domain,
            table,
            actor,
            services,
        }
    }
    fn word(&self) -> u128 {
        self.memory
            .lock()
            .unwrap()
            .read(GuestAddr(0x2000), mkw_runtime::exclusive::Width::Word)
            .unwrap()
    }
    fn reset(&self) {
        self.memory
            .lock()
            .unwrap()
            .write(GuestAddr(0x2000), &0x12345678u32.to_le_bytes())
            .unwrap();
    }
}
#[test]
fn actor_entry_rejects_invalid_storage_and_retains_live_owner() {
    with_fixture("preflight", |r, pac| {
        for case in 0..18 {
            let mut e = Environment::new(case == 16);
            let spare = Bridge::new(e.memory.clone()).unwrap(); // Fill owner quota.
            assert!(Bridge::new(e.memory.clone()).is_err());
            e.reset();
            let mut w = worker(0x400010, [0x2000, 0, 0], 3, 0x8000);
            match case {
                0 => w.entry = GuestAddr(0x400004),
                1 => w.return_pc = GuestAddr(3),
                2 => w.storage.stack.bytes = 0,
                3 => w.storage.tls.bytes = 16 * 1024 * 1024 + 1,
                4 => w.storage.tlr.start = GuestAddr(u64::MAX - 4),
                5 => w.storage.stack.start = GuestAddr(0x7001),
                6 => w.storage.stack.bytes = 4095,
                7 => w.storage.tpidr_el0 = GuestAddr(0xb000),
                8 => w.storage.tpidrro_el0 = GuestAddr(0xafff),
                9 => {
                    w.storage.tls = w.storage.stack;
                    w.storage.tpidr_el0 = w.storage.stack.start;
                }
                10 => {
                    w.storage.tlr = w.storage.tls;
                    w.storage.tpidrro_el0 = w.storage.tls.start;
                }
                11 => w.storage.stack.start = GuestAddr(0x6000),
                12 => {
                    w.storage.tls.start = GuestAddr(0x9000);
                    w.storage.tpidr_el0 = GuestAddr(0x9000);
                }
                13 => {
                    w.storage.tlr.start = GuestAddr(0xc000);
                    w.storage.tpidrro_el0 = GuestAddr(0xc000);
                }
                14 => w.storage.tls.bytes = 4097,
                15 => w.storage.tlr.bytes = 4097,
                16 => {}
                17 => w.storage.tlr.bytes = 0,
                _ => unreachable!(),
            }
            assert!(
                e.actor.run_guarded(r, &e.services, pac, w).is_err(),
                "case {case}"
            );
            assert_eq!(e.word(), 0x12345678);
            assert!(e.actor.priorities().is_ok());
            assert!(Bridge::new(e.memory.clone()).is_err());
            if case != 16 {
                // Same owner succeeds repeatedly even with no free monitor slot.
                for _ in 0..3 {
                    let o = e
                        .actor
                        .run_guarded(
                            r,
                            &e.services,
                            pac,
                            worker(0x400010, [0x2000, 0, 0], 3, 0x8000),
                        )
                        .unwrap();
                    assert_eq!(
                        (o.status, o.registers.x[0], o.registers.pc),
                        (0, 0, 0x500000)
                    );
                }
            }
            drop(spare);
            drop(e.actor);
            let first = Bridge::new(e.memory.clone()).unwrap();
            let second = Bridge::new(e.memory.clone()).unwrap();
            assert!(Bridge::new(e.memory.clone()).is_err());
            drop((first, second));
        }
    });
}
#[test]
fn actor_entry_checks_service_identity_lifecycle_and_completed_effects() {
    with_fixture("lifecycle", |r, pac| {
        let mut e = Environment::new(false);
        let foreign = Environment::new(false);
        e.reset();
        let w = || worker(0x400010, [0x2000, 0, 0], 3, 0x8000);
        let out = e.actor.run_guarded(r, &foreign.services, pac, w()).unwrap();
        assert_eq!((out.status, out.registers.x[0]), (3, 0x2000));
        assert_eq!(e.word(), 0x12345678);
        // A wrong return is rejected after completed effects, not rolled back.
        let error = e
            .actor
            .run_guarded(
                r,
                &e.services,
                pac,
                worker(0x400020, [0x2000, 0, 0], 3, 0x8000),
            )
            .unwrap_err();
        assert!(error.contains("unexpected guest PC"));
        assert_eq!(e.word(), 0);
        e.reset();
        let out = e
            .actor
            .run_guarded(
                r,
                &e.services,
                pac,
                worker(0x400010, [0x2000, 0, 0], 2, 0x8000),
            )
            .unwrap();
        assert_eq!(
            (out.status, out.remaining_budget, out.registers.x[0]),
            (2, 0, 0)
        );
        assert_eq!(e.word(), 0);
        // A registered host wait prevents entering the same actor again.
        let handle = ThreadIdentity::register(&e.table, 10).unwrap();
        let other = e
            .domain
            .bind(Bridge::new(e.memory.clone()).unwrap(), &e.table, handle)
            .unwrap();
        e.memory
            .lock()
            .unwrap()
            .write(GuestAddr(0x2000), &(other.tag() | WAITERS).to_le_bytes())
            .unwrap();
        let wait = e
            .actor
            .prepare_wait(&other, GuestAddr(0x2000))
            .unwrap()
            .unwrap();
        let cancel = wait.cancellation();
        // Wait borrows Actor, so Rust also prevents run_guarded until it drops.
        assert!(cancel.cancel().unwrap());
        drop(wait);
        assert_eq!(e.domain.pending().unwrap(), 0);
        e.reset();
        e.domain.shutdown().unwrap();
        assert!(e.actor.run_guarded(r, &e.services, pac, w()).is_err());
        assert_eq!(e.word(), 0x12345678);
        let mut stale = Environment::new(false);
        *stale.memory.lock().unwrap() = storage_memory(false);
        stale.reset();
        assert!(stale
            .actor
            .run_guarded(r, &stale.services, pac, w())
            .is_err());
        assert_eq!(stale.word(), 0x12345678);
    });
}
