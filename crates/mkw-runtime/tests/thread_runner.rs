use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Router, Target},
    exclusive_native::Bridge,
    thread_runner::{run as run_workers, Limits, Outcome, Range, Storage, Worker},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
// SAFETY: every loaded fixture is author-created reentrant C over a distinct
// CPU frame; shared guest memory goes exclusively through the owned mutex.
// The host-only refusal fixture uses an atomic and never becomes native entry.
fn run(
    memory: Arc<Mutex<ExclusiveMemory>>,
    router: &Router<'_>,
    workers: Vec<Worker>,
    limits: Limits,
) -> Result<Vec<Outcome>, String> {
    unsafe { run_workers(memory, router, workers, limits) }
}
const COUNTER: [u32; 7] = [
    0xc85ffc03, 0x91000463, 0xc804fc03, 0x35ffffa4, 0xf1000421, 0x54ffff61, 0xd65f03c0,
];
fn system(op2: u32, rt: u32) -> u32 {
    0xd5000000 | (1 << 21) | (3 << 19) | (3 << 16) | (13 << 12) | (op2 << 5) | rt
}
fn tls_code() -> Vec<u32> {
    vec![
        system(2, 9),
        system(3, 10),
        0xf940012b,
        0xf940014c,
        0xf9000520,
        0xd10043ff,
        0xf90003eb,
        0xf94003ed,
        0x910043ff,
        0xd65f03c0,
    ]
}
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/thread-runner")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut src = String::new();
    for (name, words) in [
        ("tls", tls_code()),
        ("counter", COUNTER.to_vec()),
        ("fault", vec![0xf9400000, 0xd65f03c0]),
        ("reserve", vec![0xc85ffc03, 0xd65f03c0]),
    ] {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let ir = mkw_arm64::lift_function_exclusive_v10(&bytes, 0, 0).unwrap();
        src += &mkw_codegen::emit_exclusive_calls_v10(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    src+="unsigned bad_return_abi_version(void){return 10;} unsigned bad_return_exclusive_version(void){return 2;} int bad_return(MkwCpuV10*c,uint64_t*b){if(!mkw_calls_v2_valid(c))return 3;if(!*b)return 2;--*b;c->base.pc=0xbad;return 0;}\n";
    let file = path.join("synthetic.c");
    std::fs::write(&file, src).unwrap();
    let library = path.join(format!("synthetic{opt}.so"));
    let output = Command::new("cc")
        .args([
            "-std=c11",
            opt,
            "-Wall",
            "-Wextra",
            "-Werror",
            "-fPIC",
            "-shared",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=undefined",
            "-I",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(file)
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    std::fs::write(path.join(format!("compiler{opt}.log")), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    unsafe { libloading::Library::new(library).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    let targets = [
        (0x4000, b"tls\0".as_slice()),
        (0x5000, b"counter\0".as_slice()),
        (0x6000, b"fault\0".as_slice()),
        (0x7000, b"bad_return\0".as_slice()),
        (0x8000, b"reserve\0".as_slice()),
    ]
    .into_iter()
    .map(|(at, name)| unsafe { Target::load(lib, name, GuestAddr(at), at).unwrap() })
    .collect();
    Router::new(targets, 8).unwrap()
}
fn limits() -> Limits {
    Limits {
        workers: 8,
        host_stack_bytes: 2 * 1024 * 1024,
    }
}
fn worker(i: u64, entry: u64, budget: u64) -> Worker {
    let base = 0x10000 + i * 0x4000;
    let range = |offset| Range {
        start: GuestAddr(base + offset),
        bytes: 0x100,
    };
    Worker {
        entry: GuestAddr(entry),
        return_pc: GuestAddr(0x9000 + i * 4),
        arguments: [0xa000 + i, 1000, 0, 0, 0, 0, 0, 0],
        budget,
        storage: Storage {
            stack: range(0),
            tls: range(0x1000),
            tlr: range(0x2000),
            tpidr_el0: GuestAddr(base + 0x1010),
            tpidrro_el0: GuestAddr(base + 0x2020),
        },
    }
}
fn memory(owners: usize) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 16],
        Permissions::RW,
        "shared synthetic counter",
    )
    .unwrap();
    for i in 0..8 {
        let w = worker(i, 0x4000, 100);
        for r in [w.storage.stack, w.storage.tls, w.storage.tlr] {
            m.map(
                r.start,
                vec![0; r.bytes],
                Permissions::RW,
                "explicit synthetic worker storage",
            )
            .unwrap();
        }
    }
    let mut m = ExclusiveMemory::new(
        m,
        Config {
            granule: 16,
            max_owners: owners,
        },
    )
    .unwrap();
    for i in 0..8 {
        let w = worker(i, 0x4000, 100);
        m.write(w.storage.tpidr_el0, &(0x11110000u64 + i).to_le_bytes())
            .unwrap();
        m.write(w.storage.tpidrro_el0, &(0x22220000u64 + i).to_le_bytes())
            .unwrap();
    }
    Arc::new(Mutex::new(m))
}
fn word(m: &Arc<Mutex<ExclusiveMemory>>, at: u64) -> u64 {
    m.lock()
        .unwrap()
        .read(GuestAddr(at), Width::Double)
        .unwrap() as u64
}
#[test]
fn translated_workers_keep_distinct_guest_tls_tlr_sp_and_exact_budgets() {
    let path = folder("tls");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let m = memory(8);
        let workers: Vec<_> = (0..8).map(|i| worker(i, 0x4000, 100)).collect();
        let result = run(m.clone(), &router, workers.clone(), limits()).unwrap();
        assert_eq!(result.len(), 8);
        for (i, (got, w)) in result.iter().zip(&workers).enumerate() {
            assert_eq!(got.status, 0);
            assert_eq!(got.remaining_budget, 90);
            let c = &got.registers;
            assert_eq!(c.pc, w.return_pc.0);
            assert_eq!(c.sp, w.storage.stack.start.0 + 0x100);
            assert_eq!(c.module_base, 0x4000);
            assert_eq!(
                (c.x[9], c.x[10]),
                (w.storage.tpidr_el0.0, w.storage.tpidrro_el0.0)
            );
            assert_eq!(
                (c.x[11], c.x[12], c.x[13]),
                (
                    0x11110000 + i as u64,
                    0x22220000 + i as u64,
                    0x11110000 + i as u64
                )
            );
            assert_eq!(word(&m, w.storage.tpidr_el0.0 + 8), w.arguments[0]);
            assert_eq!(c.nzcv, [0; 4]);
            assert_eq!(c.vectors, [[0; 2]; 32]);
        }
        for budget in 0..15 {
            let m = memory(1);
            let w = worker(0, 0x4000, budget);
            let result = run(m.clone(), &router, vec![w], limits()).unwrap();
            let got = &result[0];
            assert_eq!(got.status, if budget >= 10 { 0 } else { 2 });
            assert_eq!(got.remaining_budget, budget.saturating_sub(10));
            assert_eq!(
                got.registers.pc,
                if budget >= 10 {
                    w.return_pc.0
                } else {
                    0x4000 + budget * 4
                }
            );
            assert_eq!(
                got.registers.sp,
                w.storage.stack.start.0 + 0x100 - if (6..9).contains(&budget) { 16 } else { 0 }
            );
            assert_eq!(
                word(&m, w.storage.tpidr_el0.0 + 8),
                if budget >= 5 { w.arguments[0] } else { 0 }
            );
        }
    }
}
#[test]
fn translated_workers_share_coherent_exclusives_and_release_every_monitor_owner() {
    let path = folder("counter");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let m = memory(4);
        for round in 0..4 {
            let workers = (0..4)
                .map(|i| {
                    let mut w = worker(i, 0x5000, 200000);
                    w.arguments[0] = 0x1000;
                    w
                })
                .collect();
            let got = run(m.clone(), &router, workers, limits()).unwrap();
            assert!(got.iter().all(|r| r.status == 0
                && r.registers.x[1] == 0
                && r.registers.x[4] == 0
                && r.remaining_budget > 0));
            assert_eq!(word(&m, 0x1000), (round + 1) * 4000);
        }
        let m = memory(1);
        for _ in 0..32 {
            let mut w = worker(0, 0x8000, 10);
            w.arguments[0] = 0x1000;
            assert_eq!(
                run(m.clone(), &router, vec![w], limits()).unwrap()[0].status,
                0
            );
            let mut guard = m.lock().unwrap();
            let owner = guard.new_owner().unwrap();
            assert!(!guard
                .store_conditional(&owner, GuestAddr(0x1000), Width::Double, 99)
                .unwrap());
            guard.release(&owner).unwrap();
        }
    }
}
#[test]
fn malformed_worker_batches_refuse_before_guest_effects_and_owner_quota_rolls_back() {
    let path = folder("refusal");
    let lib = build(&path, "-O2");
    let router = router(&lib);
    for problem in 0..14 {
        let m = memory(8);
        let first = worker(0, 0x4000, 100);
        let mut second = worker(1, 0x4000, 100);
        let mut lim = limits();
        match problem {
            0 => second.entry = GuestAddr(0xdead0000),
            1 => second.return_pc = GuestAddr(3),
            2 => second.storage.stack.start.0 += 1,
            3 => second.storage.stack.bytes = 0,
            4 => second.storage.tpidr_el0 = GuestAddr(1),
            5 => second.storage.tpidrro_el0 = GuestAddr(second.storage.tlr.start.0 + 0x100),
            6 => second.storage.tls = first.storage.tls,
            7 => {
                second.storage.tlr = second.storage.tls;
                second.storage.tpidrro_el0 = second.storage.tpidr_el0;
            }
            8 => second.storage.stack.start = GuestAddr(u64::MAX - 15),
            9 => second.storage.tls.bytes = 16 * 1024 * 1024 + 1,
            10 => second.storage.tlr.start = GuestAddr(0xdead0000),
            11 => lim.workers = 0,
            12 => second.storage.stack.bytes = 0xf8,
            _ => lim.host_stack_bytes = 1,
        };
        assert!(
            run(m.clone(), &router, vec![first, second], lim).is_err(),
            "problem {problem}"
        );
        assert_eq!(word(&m, first.storage.tpidr_el0.0 + 8), 0);
        assert!(Bridge::new(m).is_ok());
    }
    // Host services cannot silently become translated thread entry points.
    struct Refuse(std::sync::Arc<std::sync::atomic::AtomicBool>);
    impl mkw_runtime::host_call::HostService for Refuse {
        fn invoke(
            &self,
            _: &mut mkw_runtime::host_call::HostCall<'_>,
        ) -> Result<mkw_runtime::host_call::HostReturn, mkw_runtime::host_call::HostFault> {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            Err(mkw_runtime::host_call::HostFault::unsupported())
        }
    }
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let host = Router::new(
        vec![Target::host(GuestAddr(0x4000), 0x4000, Arc::new(Refuse(called.clone()))).unwrap()],
        8,
    )
    .unwrap();
    assert!(run(memory(1), &host, vec![worker(0, 0x4000, 10)], limits()).is_err());
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    // A later read-only storage range invalidates the whole launch batch.
    let mut raw = Memory::default();
    for i in 0..2 {
        let w = worker(i, 0x4000, 10);
        for (kind, r) in [w.storage.stack, w.storage.tls, w.storage.tlr]
            .into_iter()
            .enumerate()
        {
            raw.map(
                r.start,
                vec![0; r.bytes],
                if i == 1 && kind == 1 {
                    Permissions::R
                } else {
                    Permissions::RW
                },
                "explicit permission fixture",
            )
            .unwrap();
        }
    }
    let read_only = Arc::new(Mutex::new(
        ExclusiveMemory::new(
            raw,
            Config {
                granule: 16,
                max_owners: 2,
            },
        )
        .unwrap(),
    ));
    assert!(run(
        read_only.clone(),
        &router,
        vec![worker(0, 0x4000, 10), worker(1, 0x4000, 10)],
        limits()
    )
    .is_err());
    assert_eq!(
        word(&read_only, worker(0, 0x4000, 10).storage.tpidr_el0.0 + 8),
        0
    );
    let mut too_few = limits();
    too_few.workers = 1;
    assert!(run(
        memory(2),
        &router,
        vec![worker(0, 0x4000, 10), worker(1, 0x4000, 10)],
        too_few
    )
    .is_err());
    let m = memory(1);
    assert!(run(
        m.clone(),
        &router,
        vec![worker(0, 0x4000, 10), worker(1, 0x4000, 10)],
        limits()
    )
    .is_err());
    assert_eq!(word(&m, worker(0, 0x4000, 10).storage.tpidr_el0.0 + 8), 0);
    assert!(Bridge::new(m).is_ok());
    let m = memory(8);
    assert!(run(m.clone(), &router, vec![], limits()).is_err());
    let mut lim = limits();
    lim.workers = 65;
    assert!(run(m, &router, vec![worker(0, 0x4000, 10)], lim).is_err());
}
#[test]
fn faulting_and_bad_return_workers_join_siblings_and_preserve_completed_effects() {
    let path = folder("faults");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let m = memory(2);
        let mut bad = worker(1, 0x6000, 10);
        bad.arguments[0] = 0xdead0000;
        let got = run(
            m.clone(),
            &router,
            vec![worker(0, 0x4000, 100), bad],
            limits(),
        )
        .unwrap();
        assert_eq!(got[0].status, 0);
        assert_eq!(got[1].status, 5);
        assert_eq!(got[1].registers.fault_address, 0xdead0000);
        assert_eq!(got[1].registers.x[0], 0xdead0000);
        assert_eq!(
            word(&m, worker(0, 0x4000, 100).storage.tpidr_el0.0 + 8),
            0xa000
        );
        let bad = worker(1, 0x7000, 10);
        assert!(run(
            m.clone(),
            &router,
            vec![worker(0, 0x4000, 100), bad],
            limits()
        )
        .is_err());
        let one = Bridge::new(m.clone()).unwrap();
        let two = Bridge::new(m).unwrap();
        drop((one, two));
    }
}
fn immediate(words: &mut Vec<u32>, reg: u32, value: u64) {
    for part in 0..4 {
        words.push(
            (if part == 0 { 0xd2800000 } else { 0xf2800000 })
                | (part << 21)
                | ((((value >> (part * 16)) & 65535) as u32) << 5)
                | reg,
        );
    }
}
fn qemu_image(iterations: u64, initial: u64) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut words = Vec::new();
    immediate(&mut words, 0, 0);
    immediate(&mut words, 1, iterations);
    immediate(
        &mut words,
        30,
        BASE + TEXT as u64 + (12 + COUNTER.len()) as u64 * 4,
    );
    words.extend(COUNTER);
    words.extend([0xf9000403, 0xf9000801, 0xf9000c04, 0xaa0003e1]);
    immediate(&mut words, 0, 1);
    immediate(&mut words, 2, 32);
    immediate(&mut words, 8, 64);
    words.push(0xd4000001);
    immediate(&mut words, 0, 0);
    immediate(&mut words, 8, 93);
    words.push(0xd4000001);
    let data = (TEXT + words.len() * 4 + 15) & !15;
    let mut patch = Vec::new();
    immediate(&mut patch, 0, BASE + data as u64);
    words[..4].copy_from_slice(&patch);
    let mut image = vec![0; data + 32];
    image[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    let p16 = |b: &mut [u8], at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let p64 = |b: &mut [u8], at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    p16(&mut image, 16, 2);
    p16(&mut image, 18, 183);
    p32(&mut image, 20, 1);
    p64(&mut image, 24, BASE + TEXT as u64);
    p64(&mut image, 32, 64);
    p16(&mut image, 52, 64);
    p16(&mut image, 54, 56);
    p16(&mut image, 56, 1);
    p32(&mut image, 64, 1);
    p32(&mut image, 68, 7);
    p64(&mut image, 80, BASE);
    p64(&mut image, 88, BASE);
    let length = image.len() as u64;
    p64(&mut image, 96, length);
    p64(&mut image, 104, length);
    p64(&mut image, 112, 4096);
    for (i, w) in words.iter().enumerate() {
        p32(&mut image, TEXT + i * 4, *w);
    }
    p64(&mut image, data, initial);
    image
}
#[test]
fn sequential_translated_worker_sequences_match_qemu_and_independent_counts() {
    let path = folder("qemu");
    let qemu = Command::new("qemu-aarch64").arg("--version").output();
    let available = qemu.is_ok_and(|o| o.status.success());
    let mut compared = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        for iterations in [1u64, 2, 3, 7, 32, 255] {
            for initial in [0u64, 1, 0x8000000000000000, u64::MAX - 1] {
                let expected = initial.wrapping_add(iterations);
                let m = memory(1);
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x1000), &initial.to_le_bytes())
                    .unwrap();
                let mut w = worker(0, 0x5000, 20000);
                w.arguments[0] = 0x1000;
                w.arguments[1] = iterations;
                let result = run(m.clone(), &router, vec![w], limits()).unwrap();
                let got = &result[0];
                assert_eq!(got.status, 0);
                assert_eq!(got.remaining_budget, 20000 - (6 * iterations + 1));
                assert_eq!(
                    [
                        word(&m, 0x1000),
                        got.registers.x[3],
                        got.registers.x[1],
                        got.registers.x[4]
                    ],
                    [expected, expected, 0, 0]
                );
                if available {
                    let file = path.join("original-synthetic.elf");
                    std::fs::write(&file, qemu_image(iterations, initial)).unwrap();
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700))
                        .unwrap();
                    let output = Command::new("qemu-aarch64")
                        .args(["-cpu", "max"])
                        .arg(&file)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert_eq!(output.stdout.len(), 32);
                    let actual: Vec<_> = output
                        .stdout
                        .chunks_exact(8)
                        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
                        .collect();
                    assert_eq!(actual, [expected, expected, 0, 0]);
                    compared += 1;
                }
            }
        }
    }
    std::fs::write(path.join("qemu-receipt.json"),serde_json::json!({"available":available,"compared":compared,"native_cases_per_optimization":24,"original_guest_execution":false,"limitations":"Sequential instruction paths only; concurrent host scheduling and hardware weak memory are not compared to QEMU."}).to_string()).unwrap();
}
