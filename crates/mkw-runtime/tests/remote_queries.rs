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
    let dir = root.join("private/testing/remote-queries").join(label);
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

use mkw_runtime::process_regions::{Error, Layout, ProcessRegions, Span};
fn span(base: u64, size: u64) -> Span {
    Span {
        base: GuestAddr(base),
        size,
    }
}
fn catalog(m: Arc<Mutex<ExclusiveMemory>>, h: &Heap) -> ProcessRegions {
    ProcessRegions::new(
        m,
        h,
        Layout {
            address_space: span(0, 1 << 39),
            code: span(0x4000, 4096),
            stack: span(0x1000, 4096),
            heap: span(0x1_0000_0000, 16384),
            alias: span(0x2_0000_0000, 0x10000),
            remote: span(0x3_0000_0000, 0x10000),
        },
    )
    .unwrap()
}
fn aslr() -> Span {
    span(0x1000, 0x4_0000_0000)
}
fn configured() -> (Arc<Mutex<ExclusiveMemory>>, Heap, ProcessMemory) {
    let (m, h, p) = fixture();
    let p = p.with_regions(catalog(m.clone(), &h), aslr()).unwrap();
    (m, h, p)
}

#[test]
fn opt_in_requires_aligned_nonzero_region_and_resets_on_reconfiguration() {
    let (m, h, p) = fixture();
    assert!(matches!(
        p.clone().with_observed_remote_region(),
        Err(Error::Configuration)
    ));
    for remote in [span(0x3_0000_1000, 0x10000), span(0x3_0000_0000, 4096)] {
        let mut l = catalog(m.clone(), &h).snapshot().unwrap();
        l.remote = remote;
        let r = ProcessRegions::new(m.clone(), &h, l).unwrap();
        assert!(matches!(
            p.clone()
                .with_regions(r, aslr())
                .unwrap()
                .with_observed_remote_region(),
            Err(Error::Configuration)
        ));
    }
    let mut l = catalog(m.clone(), &h).snapshot().unwrap();
    l.remote = span(0x8_0000_0000, 0x10000);
    let r = ProcessRegions::new(m.clone(), &h, l).unwrap();
    assert!(p.clone().with_regions(r, aslr()).is_err());
    let (zero_memory, zero_heap) = ExclusiveMemory::with_heap(
        Memory::default(),
        Config {
            granule: 16,
            max_owners: 1,
        },
        HeapSpec {
            base: GuestAddr(0x1000000),
            capacity: 8192,
        },
    )
    .unwrap();
    let zero_memory = Arc::new(Mutex::new(zero_memory));
    let zero_regions = ProcessRegions::new(
        zero_memory.clone(),
        &zero_heap,
        Layout {
            address_space: span(0, 1 << 39),
            code: span(0x10000, 0x10000),
            stack: span(0x20000, 0x10000),
            alias: span(0x30000, 0x10000),
            heap: span(0x1000000, 8192),
            remote: span(0, 0x10000),
        },
    )
    .unwrap();
    let zero = ProcessMemory::new(
        Abi::Nx64IsolatedPrivatePages,
        OwnedMemoryBudget::new(zero_memory, LIMIT).unwrap(),
    )
    .with_regions(zero_regions, span(0, 1 << 39))
    .unwrap();
    assert!(matches!(
        zero.with_observed_remote_region(),
        Err(Error::Configuration)
    ));
    let good = p.with_regions(catalog(m, &h), aslr()).unwrap();
    assert!(good.with_observed_remote_region().is_ok());
}
#[test]
fn native_remote_success_and_refusals_preserve_registers_monitors_and_budgets() {
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, _, p) = configured();
        let s = p.with_observed_remote_region().unwrap().registry();
        let mut b = Bridge::new(m).unwrap();
        for (kind, handle, sub, value) in [
            (29u64, 0xffff8001u64, 0u64, Some(0x3_0000_0000)),
            (0x12340000001e, 0x5678ffff8001, 0, Some(0x10000)),
            (29, 0xffff8001, 1, None),
            (30, 0xffff8001, 1 << 32, None),
            (29, 0, 0, None),
            (30, 0xffff8000, 0, None),
            (31, 0xffff8001, 0, None),
            (30, 0xffff8001, u64::MAX, None),
        ] {
            for steps in 0..=3 {
                let mut c = cpu(kind, handle, sub);
                run(&mut b, &r, &s, &mut c, 0x11000, 2);
                let old = c.base.x;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x10000, steps),
                    if steps == 0 {
                        2
                    } else if value.is_none() {
                        3
                    } else if steps < 2 {
                        2
                    } else {
                        0
                    }
                );
                if let Some(value) = value.filter(|_| steps > 0) {
                    assert_eq!(c.base.x[0], 0);
                    assert_eq!(c.base.x[1], value);
                    assert_eq!(&c.base.x[2..], &old[2..]);
                } else {
                    assert_eq!(c.base.x, old);
                }
                run(&mut b, &r, &s, &mut c, 0x12000, 2);
                assert_eq!(c.base.x[7], u64::from(steps > 0 && value.is_some()));
            }
        }
    }
}
#[test]
fn default_reconfigured_foreign_and_stale_profiles_refuse_remote_queries() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..5 {
            let (m, h, p) = configured();
            let p = if mode == 0 {
                p
            } else {
                p.with_observed_remote_region().unwrap()
            };
            let p = if mode == 1 {
                p.with_regions(catalog(m.clone(), &h), aslr()).unwrap()
            } else {
                p
            };
            let mut s = p.registry();
            let target = if mode == 2 { fixture().0 } else { m.clone() };
            if mode == 3 {
                *m.lock().unwrap() = ExclusiveMemory::new(
                    Memory::default(),
                    Config {
                        granule: 16,
                        max_owners: 4,
                    },
                )
                .unwrap();
            }
            if mode == 4 {
                s = s.restricted_to(&Default::default());
            }
            let mut b = Bridge::new(target).unwrap();
            let mut c = cpu(29, u64::from(CURRENT_PROCESS), 0);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 3);
            assert_eq!(c.base.x, old);
        }
    }
}
#[test]
fn combined_profile_requires_explicit_remote_variant_and_preserves_other_queries() {
    use mkw_runtime::{
        memory_catalog::{Declaration, MemoryCatalog, State},
        process_queries,
    };
    for opt in ["-O0", "-O2"] {
        let lib = build("combined", opt);
        let r = router(&lib);
        for enabled in [false, true] {
            let (m, h, _) = fixture();
            let cat = MemoryCatalog::new(
                m.clone(),
                &h,
                span(0, 1 << 39),
                vec![
                    Declaration {
                        span: span(0x1000, 4096),
                        state: State::Stack,
                    },
                    Declaration {
                        span: span(0x4000, 4096),
                        state: State::Code,
                    },
                ],
            )
            .unwrap();
            let e = ProcessEntropy::from_synthetic_words(
                process_entropy::Abi::Horizon64EntropyReference,
                [7; 4],
            );
            let s = process_queries::registry(
                if enabled {
                    process_queries::Abi::Nx64PrivateOwnedProcessWithObservedRemote64K
                } else {
                    process_queries::Abi::Nx64PrivateOwnedProcess
                },
                OwnedMemoryBudget::new(m.clone(), LIMIT).unwrap(),
                catalog(m.clone(), &h),
                cat,
                aslr(),
                Some(&e),
            )
            .unwrap();
            let mut b = Bridge::new(m.clone()).unwrap();
            for size in [0, 4096, 8192] {
                m.lock().unwrap().resize_heap(&h, size).unwrap();
                for kind in [29, 30] {
                    let mut c = cpu(kind, u64::from(CURRENT_PROCESS), 0);
                    assert_eq!(
                        run(&mut b, &r, &s, &mut c, 0x10000, 2),
                        if enabled { 0 } else { 3 }
                    );
                    if enabled {
                        assert_eq!(
                            c.base.x[1],
                            if kind == 29 { 0x3_0000_0000 } else { 0x10000 }
                        );
                    }
                }
                let mut c = cpu(22, u64::from(CURRENT_PROCESS), 0);
                assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 0);
                assert_eq!(c.base.x[1], 8192 + size as u64);
            }
            let mut c = cpu(11, 0, 0);
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 0);
            assert_eq!(c.base.x[1], 7);
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
                let (m, _, p) = configured();
                let s = p.with_observed_remote_region().unwrap().registry();
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(29, u64::from(CURRENT_PROCESS), 0);
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
                assert_eq!(c.base.x[1], if steps >= 2 { 0x3_0000_0000u64 } else { 29 });
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(output), Width::Double)
                        .unwrap(),
                    if output == 0x1900 && steps >= 4 {
                        u128::from(0x3_0000_0000u64)
                    } else {
                        0
                    }
                );
            }
        }
    }
}
