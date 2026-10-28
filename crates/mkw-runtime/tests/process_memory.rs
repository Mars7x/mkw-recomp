use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Heap, HeapSpec, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    owned_memory_budget::OwnedMemoryBudget,
    process_entropy::{self, ProcessEntropy},
    process_memory::{Abi, ProcessMemory, CURRENT_PROCESS},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
const LIMIT: u64 = 0x2_0000_0000;
fn fixture() -> (Arc<Mutex<ExclusiveMemory>>, Heap, ProcessMemory) {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 4096],
        Permissions::RW,
        "synthetic stack",
    )
    .unwrap();
    m.map(
        GuestAddr(0x4000),
        vec![0; 4096],
        Permissions::R,
        "synthetic readonly",
    )
    .unwrap();
    let (m, h) = ExclusiveMemory::with_heap(
        m,
        Config {
            granule: 16,
            max_owners: 4,
        },
        HeapSpec {
            base: GuestAddr(0x1_0000_0000),
            capacity: 16384,
        },
    )
    .unwrap();
    let m = Arc::new(Mutex::new(m));
    let p = ProcessMemory::new(
        Abi::Nx64IsolatedPrivatePages,
        OwnedMemoryBudget::new(m.clone(), LIMIT).unwrap(),
    );
    (m, h, p)
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut source = String::new();
    for (name, words) in [
        ("query", vec![0xd4000521u32, 0xd65f03c0]),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("conditional", vec![0xc807fca6, 0xd65f03c0]),
        (
            "wrapper",
            vec![
                0xf81f0fe0, 0xd4000521, 0xf94003e2, 0xf9000041, 0x910043ff, 0xd65f03c0,
            ],
        ),
    ] {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        source +=
            &mkw_codegen::emit_services_v3(&mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap(), 0)
                .unwrap()
                .replace("mkw_guest_v10_budget", name);
    }
    let dir = root.join("private/testing/process-memory").join(label);
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
        .arg(root.join("native"))
        .arg(src)
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
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x10000, b"query\0".as_slice()),
            (0x11000, b"reserve\0".as_slice()),
            (0x12000, b"conditional\0".as_slice()),
            (0x13000, b"wrapper\0".as_slice()),
        ]
        .iter()
        .map(|(a, n)| unsafe { Target::load_services(lib, n, GuestAddr(*a), *a).unwrap() })
        .collect(),
        4,
    )
    .unwrap()
}
fn cpu(kind: u64, handle: u64, sub: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x = [0x123456789abcdef0; 31];
    c.base.x[1] = kind;
    c.base.x[2] = handle;
    c.base.x[3] = sub;
    c.base.x[5] = 0x1000;
    c.base.x[6] = 0x55;
    c.base.x[30] = 0x20000;
    c.base.sp = 0x1800;
    c
}
fn run(b: &mut Bridge, r: &Router<'_>, s: &Services, c: &mut CpuV10, at: u64, budget: u64) -> i32 {
    let mut left = budget;
    let mut a = b.attach(c).unwrap();
    let result = Routed::attach_services(&mut a, r, s)
        .unwrap()
        .run(GuestAddr(at), &mut left)
        .unwrap();
    result
}
#[test]
fn query_widths_error_order_and_entropy_dispatch_preserve_budget_semantics() {
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, h, p) = fixture();
        m.lock().unwrap().resize_heap(&h, 4096).unwrap();
        let entropy = ProcessEntropy::from_synthetic_words(
            process_entropy::Abi::Horizon64EntropyReference,
            [5, 6, 7, u64::MAX],
        );
        let s = p.registry_with_entropy(&entropy);
        assert!(s.combined(&entropy.registry()).is_err());
        let mut b = Bridge::new(m.clone()).unwrap();
        for (kind, handle, sub, status, code, value) in [
            (21, 0xffff8001, 0, 0, 0, Some(LIMIT)),
            (0xffff000000000016, 0x1234ffff8001, 0, 0, 0, Some(12288)),
            (22, 0, 1, 0, 0xe801, None),
            (21, 0xffff8001, 1u64 << 32, 0, 0xe801, None),
            (21, 0, 0, 3, 0, None),
            (22, 0xffff8000, 0, 3, 0, None),
            (11, 0, 3, 0, 0, Some(u64::MAX)),
            (11, 1, 9, 0, 0xe401, None),
            (11, 0, 4, 0, 0xe801, None),
            (6, 0xffff8001, 0, 3, 0, None),
        ] {
            for budget in 0..=3 {
                let mut c = cpu(kind, handle, sub);
                run(&mut b, &r, &s, &mut c, 0x11000, 2);
                let before = c.base.x;
                let result = run(&mut b, &r, &s, &mut c, 0x10000, budget);
                assert_eq!(
                    result,
                    if budget == 0 {
                        2
                    } else if status != 0 {
                        status
                    } else if budget < 2 {
                        2
                    } else {
                        0
                    }
                );
                if budget > 0 && status == 0 {
                    assert_eq!(c.base.x[0], code);
                    assert_eq!(c.base.x[1], value.unwrap_or(kind));
                    assert_eq!(&c.base.x[2..], &before[2..]);
                } else {
                    assert_eq!(c.base.x, before);
                }
                run(&mut b, &r, &s, &mut c, 0x12000, 2);
                assert_eq!(c.base.x[7], u64::from(budget > 0 && status == 0));
            }
        }
    }
}
#[test]
fn foreign_stale_denied_and_missing_entropy_profiles_refuse_without_effects() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..4 {
            let (m, _, p) = fixture();
            let entropy = ProcessEntropy::from_synthetic_words(
                process_entropy::Abi::Horizon64EntropyReference,
                [1; 4],
            );
            let mut s = p.registry_with_entropy(&entropy);
            let mut kind = 21;
            let target = match mode {
                0 => fixture().0,
                1 => {
                    *m.lock().unwrap() = ExclusiveMemory::new(
                        {
                            let mut n = Memory::default();
                            n.map(
                                GuestAddr(0x1000),
                                vec![0; 4096],
                                Permissions::RW,
                                "new generation",
                            )
                            .unwrap();
                            n
                        },
                        Config {
                            granule: 16,
                            max_owners: 4,
                        },
                    )
                    .unwrap();
                    m.clone()
                }
                2 => {
                    s = s.restricted_to(&Default::default());
                    m.clone()
                }
                _ => {
                    s = p.registry();
                    kind = 11;
                    m.clone()
                }
            };
            let mut b = Bridge::new(target).unwrap();
            let mut c = cpu(kind, u64::from(CURRENT_PROCESS), 0);
            run(&mut b, &r, &s, &mut c, 0x11000, 2);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 3);
            assert_eq!(c.base.x, old);
            run(&mut b, &r, &s, &mut c, 0x12000, 2);
            assert_eq!(c.base.x[7], 0);
            if kind != 11 {
                let mut c = cpu(11, 0, 0);
                run(&mut b, &r, &s, &mut c, 0x11000, 2);
                let old = c.base.x;
                assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 3);
                assert_eq!(c.base.x, old);
                run(&mut b, &r, &s, &mut c, 0x12000, 2);
                assert_eq!(c.base.x[7], 0);
            }
        }
    }
}
#[test]
fn wrapper_output_faults_follow_completed_query_and_budget_order() {
    for opt in ["-O0", "-O2"] {
        let lib = build("wrapper", opt);
        let r = router(&lib);
        for output in [0x1900, 0x4000] {
            for steps in 0..=7 {
                let (m, _, p) = fixture();
                let s = p.registry();
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(21, u64::from(CURRENT_PROCESS), 0);
                c.base.x[0] = output;
                let status = run(&mut b, &r, &s, &mut c, 0x13000, steps);
                assert_eq!(
                    status,
                    if output == 0x4000 && steps >= 4 {
                        5
                    } else if steps < 6 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(c.base.x[1], if steps >= 2 { LIMIT } else { 21 });
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(output), Width::Double)
                        .unwrap(),
                    if output == 0x1900 && steps >= 4 {
                        u128::from(LIMIT)
                    } else {
                        0
                    }
                );
            }
        }
    }
}
#[test]
fn multiple_live_owners_query_committed_heap_usage_during_resize() {
    for opt in ["-O0", "-O2"] {
        let lib = build("concurrent", opt);
        let r = router(&lib);
        let (m, h, p) = fixture();
        let s = p.registry();
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                for i in 0..256 {
                    m.lock().unwrap().resize_heap(&h, (i % 5) * 4096).unwrap();
                }
            });
            for _ in 0..2 {
                scope.spawn(|| {
                    let mut b = Bridge::new(m.clone()).unwrap();
                    barrier.wait();
                    for _ in 0..256 {
                        let mut c = cpu(22, u64::from(CURRENT_PROCESS), 0);
                        assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 0);
                        assert!((8192..=24576).contains(&c.base.x[1]));
                        assert_eq!(c.base.x[1] % 4096, 0);
                    }
                });
            }
        });
        let before = m.lock().unwrap().heap_size(&h).unwrap();
        assert!(m.lock().unwrap().resize_heap(&h, 20480).is_err());
        let mut b = Bridge::new(m.clone()).unwrap();
        let mut c = cpu(22, u64::from(CURRENT_PROCESS), 0);
        assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 0);
        assert_eq!(c.base.x[1], 8192 + before as u64);
    }
}
