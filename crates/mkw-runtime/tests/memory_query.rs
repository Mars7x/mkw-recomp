use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive::{Heap, HeapSpec},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    memory_catalog::{Declaration, MemoryCatalog, State},
    memory_query::{Abi, MemoryQuery},
    process_regions::Span,
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        ("query", vec![0xd40000c1u32, 0xd65f03c0]),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("store", vec![0xc807fca6, 0xd65f03c0]),
        (
            "wrapper",
            vec![
                0xf81f0fe1, 0xd40000c1, 0xf94003e2, 0xb9000041, 0x910043ff, 0xd65f03c0,
            ],
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
    let dir = root().join("private/testing/memory-query").join(label);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("synthetic{opt}.c"));
    let lib = src.with_extension("so");
    std::fs::write(&src, source).unwrap();
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
        .arg(root().join("native"))
        .arg(src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        cc.status.success(),
        "{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x4000, b"query\0".as_slice()),
            (0x6000, b"reserve\0".as_slice()),
            (0x7000, b"store\0".as_slice()),
            (0x8000, b"wrapper\0".as_slice()),
        ]
        .into_iter()
        .map(|(at, n)| unsafe { Target::load_services(lib, n, GuestAddr(at), at).unwrap() })
        .collect(),
        8,
    )
    .unwrap()
}
fn cpu(handle: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0x2100;
    c.base.x[1] = handle;
    c.base.x[2] = 0x2fff;
    c.base.x[30] = 0x9000;
    c.base.sp = 0x2800;
    c
}
fn run(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    cpu: &mut CpuV10,
    at: u64,
    limit: u64,
) -> (i32, u64) {
    let mut budget = limit;
    let status = {
        let mut a = bridge.attach(cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        r.run(GuestAddr(at), &mut budget).unwrap()
    };
    (status, budget)
}

fn span(base: u64, size: u64) -> Span {
    Span {
        base: GuestAddr(base),
        size,
    }
}
fn fixture(start: u64) -> (Arc<Mutex<ExclusiveMemory>>, Heap, MemoryCatalog) {
    let mut raw = Memory::default();
    let mut declarations = Vec::new();
    for (base, permissions, state) in [
        (0x2000, Permissions::RW, State::Stack),
        (0x3000, Permissions::RW, State::Stack),
        (0x4000, Permissions::RX, State::Code),
        (0x5000, Permissions::RW, State::CodeData),
        (0x6000, Permissions::RW, State::ThreadLocal),
    ] {
        raw.map(GuestAddr(base), vec![0; 4096], permissions, "synthetic")
            .unwrap();
        declarations.push(Declaration {
            span: span(base, 4096),
            state,
        });
    }
    let (m, h) = ExclusiveMemory::with_heap(
        raw,
        Config {
            granule: 16,
            max_owners: 3,
        },
        HeapSpec {
            base: GuestAddr(0x10000),
            capacity: 8192,
        },
    )
    .unwrap();
    let m = Arc::new(Mutex::new(m));
    let c = MemoryCatalog::new(m.clone(), &h, span(start, 0x20000 - start), declarations).unwrap();
    (m, h, c)
}
fn service(c: MemoryCatalog) -> Services {
    MemoryQuery::new(Abi::Nx64PrivateCatalog, c)
        .unwrap()
        .registry()
}
fn bytes(m: &Arc<Mutex<ExclusiveMemory>>, at: u64, n: usize) -> Vec<u8> {
    {
        let m = m.lock().unwrap();
        (0..n)
            .map(|i| m.read(GuestAddr(at + i as u64), Width::Byte).unwrap() as u8)
            .collect()
    }
}
fn packet(base: u64, size: u64, state: u32, perm: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(base.to_le_bytes());
    b.extend(size.to_le_bytes());
    for x in [state, 0, perm, 0, 0, 0] {
        b.extend(x.to_le_bytes());
    }
    b
}
#[test]
fn native_packets_cover_every_state_gaps_outside_and_budget_boundaries() {
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, h, catalog) = fixture(0);
        m.lock().unwrap().resize_heap(&h, 4096).unwrap();
        let s = service(catalog);
        let mut b = Bridge::new(m.clone()).unwrap();
        for (query, base, size, state, perm) in [
            (0, 0, 0x2000, 0, 0),
            (0x1fff, 0, 0x2000, 0, 0),
            (0x2000, 0x2000, 8192, 11, 3),
            (0x3fff, 0x2000, 8192, 11, 3),
            (0x4001, 0x4000, 4096, 3, 5),
            (0x5000, 0x5000, 4096, 4, 3),
            (0x6000, 0x6000, 4096, 12, 3),
            (0x7000, 0x7000, 0x9000, 0, 0),
            (0x10000, 0x10000, 4096, 5, 3),
            (0x11000, 0x11000, 0xf000, 0, 0),
            (0x20000, 0x20000, 0u64.wrapping_sub(0x20000), 16, 0),
            (u64::MAX, 0x20000, 0u64.wrapping_sub(0x20000), 16, 0),
        ] {
            for budget in 0..=3 {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x2100), &[0xaa; 40])
                    .unwrap();
                let mut c = cpu(u64::MAX);
                c.base.x[2] = query;
                c.base.x[5] = 0x2300;
                c.base.x[6] = 7;
                run(&mut b, &r, &s, &mut c, 0x6000, 2);
                let old = c.base.x;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x4000, budget).0,
                    if budget < 2 { 2 } else { 0 }
                );
                assert_eq!(
                    bytes(&m, 0x2100, 40),
                    if budget > 0 {
                        packet(base, size, state, perm)
                    } else {
                        vec![0xaa; 40]
                    }
                );
                if budget > 0 {
                    assert_eq!(&c.base.x[..2], &[0, 0]);
                    assert_eq!(&c.base.x[2..], &old[2..]);
                } else {
                    assert_eq!(c.base.x, old);
                }
                run(&mut b, &r, &s, &mut c, 0x7000, 2);
                assert_eq!(c.base.x[7], u64::from(budget > 0));
            }
        }
    }
}
#[test]
fn output_packet_crosses_adjacent_pages_atomically_or_faults_without_effects() {
    for opt in ["-O0", "-O2"] {
        let lib = build("output", opt);
        let r = router(&lib);
        for output in [0x2ff0, 0x3fd8, 0x3fd9, u64::MAX] {
            for budget in 0..=3 {
                let (m, _, cat) = fixture(0);
                let s = service(cat);
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(0x7777);
                c.base.x[0] = output;
                c.base.x[5] = 0x2300;
                c.base.x[6] = 7;
                run(&mut b, &r, &s, &mut c, 0x6000, 2);
                let old = c.base.x;
                let before = bytes(&m, 0x2000, 8192);
                let valid = output <= 0x3fd8;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x4000, budget).0,
                    if budget == 0 {
                        2
                    } else if !valid {
                        5
                    } else if budget < 2 {
                        2
                    } else {
                        0
                    }
                );
                if budget == 0 || !valid {
                    assert_eq!(bytes(&m, 0x2000, 8192), before);
                    assert_eq!(c.base.x, old);
                } else {
                    assert_eq!(bytes(&m, output, 40), packet(0x2000, 8192, 11, 3));
                }
                run(&mut b, &r, &s, &mut c, 0x7000, 2);
                assert_eq!(c.base.x[7], u64::from(budget > 0 && valid));
            }
        }
    }
}
#[test]
fn nonzero_origin_foreign_stale_and_restricted_profiles_reject() {
    assert!(MemoryQuery::new(Abi::Nx64PrivateCatalog, fixture(0x1000).2).is_err());
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let (m, _, cat) = fixture(0);
            let mut s = service(cat);
            let actual = if mode == 0 { fixture(0).0 } else { m.clone() };
            if mode == 1 {
                *m.lock().unwrap() = ExclusiveMemory::new(
                    Memory::default(),
                    Config {
                        granule: 16,
                        max_owners: 3,
                    },
                )
                .unwrap();
            }
            if mode == 2 {
                s = s.restricted_to(&Default::default());
            }
            let mut b = Bridge::new(actual).unwrap();
            let mut c = cpu(0x7777);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x4000, 2).0, 3);
            assert_eq!(c.base.x, old);
        }
        let (m, _, cat) = fixture(0);
        let s = service(cat);
        assert!(s
            .combined(
                &mkw_runtime::stack_query::StackQuery::new(
                    mkw_runtime::stack_query::Abi::Horizon64OwnedStack,
                    m,
                    GuestAddr(0x2000),
                    4096
                )
                .unwrap()
                .registry()
            )
            .is_err());
    }
}
#[test]
fn wrapper_page_info_fault_retains_completed_catalog_packet() {
    for opt in ["-O0", "-O2"] {
        let lib = build("wrapper", opt);
        let r = router(&lib);
        for output in [0x2200, 0x4000] {
            for budget in 0..=7 {
                let (m, _, cat) = fixture(0);
                let s = service(cat);
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(output);
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x2200), &[0xff; 8])
                    .unwrap();
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x8000, budget).0,
                    if budget < 4 {
                        2
                    } else if output == 0x4000 {
                        5
                    } else if budget < 6 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(
                    bytes(&m, 0x2100, 40),
                    if budget >= 2 {
                        packet(0x2000, 8192, 11, 3)
                    } else {
                        vec![0; 40]
                    }
                );
                assert_eq!(
                    bytes(&m, 0x2200, 8),
                    if output == 0x2200 && budget >= 4 {
                        vec![0, 0, 0, 0, 255, 255, 255, 255]
                    } else {
                        vec![255; 8]
                    }
                );
            }
        }
    }
}
#[test]
fn concurrent_heap_queries_return_one_coherent_partition() {
    for opt in ["-O0", "-O2"] {
        let lib = build("concurrent", opt);
        let r = router(&lib);
        let (m, h, cat) = fixture(0);
        let s = service(cat);
        let barrier = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                barrier.wait();
                for i in 0..256 {
                    m.lock().unwrap().resize_heap(&h, (i % 3) * 4096).unwrap();
                }
            });
            for output in [0x2100, 0x2200] {
                let m = &m;
                let s = &s;
                let r = &r;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut b = Bridge::new(m.clone()).unwrap();
                    barrier.wait();
                    for _ in 0..256 {
                        let mut c = cpu(0);
                        c.base.x[0] = output;
                        c.base.x[2] = 0x10000;
                        assert_eq!(run(&mut b, r, s, &mut c, 0x4000, 2).0, 0);
                        let got = bytes(m, output, 40);
                        assert!([
                            packet(0x7000, 0x19000, 0, 0),
                            packet(0x10000, 4096, 5, 3),
                            packet(0x10000, 8192, 5, 3)
                        ]
                        .contains(&got));
                    }
                });
            }
        });
    }
}
