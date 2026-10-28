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
    let dir = root.join("private/testing/region-queries").join(label);
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
fn configuration_requires_matching_generation_and_enclosing_aslr() {
    let (m, h, p) = fixture();
    let regions = catalog(m.clone(), &h);
    for bad in [
        span(0, 0),
        span(1, 4096),
        span(0x1000, 4097),
        span(u64::MAX - 4095, 8192),
        span(0, 1 << 40),
        span(0x4000, 4096),
        span(0x2000, 0x4_0000_0000),
    ] {
        assert!(matches!(
            p.clone().with_regions(regions.clone(), bad),
            Err(Error::Configuration)
        ));
    }
    assert!(matches!(
        fixture().2.with_regions(regions.clone(), aslr()),
        Err(Error::Configuration)
    ));
    let replacement = fixture().0;
    *m.lock().unwrap() = std::mem::replace(
        &mut *replacement.lock().unwrap(),
        ExclusiveMemory::new(
            Memory::default(),
            Config {
                granule: 16,
                max_owners: 4,
            },
        )
        .unwrap(),
    );
    assert!(matches!(
        p.with_regions(regions, aslr()),
        Err(Error::StaleGeneration)
    ));
}
#[test]
fn native_region_queries_preserve_bounds_widths_and_completion_rules() {
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, h, p) = configured();
        m.lock().unwrap().resize_heap(&h, 4096).unwrap();
        let e = ProcessEntropy::from_synthetic_words(
            process_entropy::Abi::Horizon64EntropyReference,
            [7; 4],
        );
        let s = p.registry_with_entropy(&e);
        assert!(s.combined(&e.registry()).is_err());
        let mut b = Bridge::new(m.clone()).unwrap();
        for (kind, value) in [
            (2, 0x2_0000_0000),
            (3, 0x10000),
            (4, 0x1_0000_0000),
            (5, 16384),
            (12, 0x1000),
            (13, 0x4_0000_0000),
            (14, 0x1000),
            (15, 4096),
            (21, LIMIT),
            (22, 12288),
        ] {
            for budget in 0..=3 {
                let mut c = cpu(kind | (1 << 40), u64::from(CURRENT_PROCESS) | (1 << 40), 0);
                run(&mut b, &r, &s, &mut c, 0x11000, 2);
                let before = c.base.x;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x10000, budget),
                    if budget < 2 { 2 } else { 0 }
                );
                if budget > 0 {
                    assert_eq!(c.base.x[0], 0);
                    assert_eq!(c.base.x[1], value);
                    assert_eq!(&c.base.x[2..], &before[2..]);
                } else {
                    assert_eq!(c.base.x, before);
                }
                run(&mut b, &r, &s, &mut c, 0x12000, 2);
                assert_eq!(c.base.x[7], u64::from(budget > 0));
            }
        }
        for (kind, handle, sub, status, code) in [
            (2, 0, 1, 0, 0xe801),
            (15, 0xffff8001, 1 << 32, 0, 0xe801),
            (4, 0, 0, 3, 0),
            (14, 0xffff8000, 0, 3, 0),
            (29, 0xffff8001, 0, 3, 0),
            (30, 0xffff8001, 1, 3, 0),
        ] {
            let mut c = cpu(kind, handle, sub);
            run(&mut b, &r, &s, &mut c, 0x11000, 2);
            let before = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), status);
            if status == 0 {
                assert_eq!(c.base.x[0], code);
                assert_eq!(&c.base.x[1..], &before[1..]);
            } else {
                assert_eq!(c.base.x, before);
            }
            run(&mut b, &r, &s, &mut c, 0x12000, 2);
            assert_eq!(c.base.x[7], u64::from(status == 0));
        }
        let mut c = cpu(11, 0, 0);
        assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 0);
        assert_eq!(c.base.x[1], 7);
    }
}
#[test]
fn absent_foreign_stale_and_restricted_region_bindings_refuse() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..4 {
            let (m, _, p) = configured();
            let mut s = p.registry();
            let target = match mode {
                0 => fixture().0,
                1 => {
                    s = fixture().2.registry();
                    m.clone()
                }
                2 => {
                    *m.lock().unwrap() = ExclusiveMemory::new(
                        Memory::default(),
                        Config {
                            granule: 16,
                            max_owners: 4,
                        },
                    )
                    .unwrap();
                    m.clone()
                }
                _ => {
                    s = s.restricted_to(&Default::default());
                    m.clone()
                }
            };
            let mut b = Bridge::new(target).unwrap();
            let mut c = cpu(5, u64::from(CURRENT_PROCESS), 0);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 3);
            assert_eq!(c.base.x, old);
        }
        let (m, _, p) = fixture();
        let mut b = Bridge::new(m).unwrap();
        let s = p.registry();
        let mut c = cpu(2, u64::from(CURRENT_PROCESS), 1);
        let old = c.base.x;
        assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), 3);
        assert_eq!(c.base.x, old);
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
                let s = p.registry();
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(13, u64::from(CURRENT_PROCESS), 0);
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
                assert_eq!(c.base.x[1], if steps >= 2 { aslr().size } else { 13 });
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(output), Width::Double)
                        .unwrap(),
                    if output == 0x1900 && steps >= 4 {
                        u128::from(aslr().size)
                    } else {
                        0
                    }
                );
            }
        }
    }
}
