use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive::{Heap, HeapSpec},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    memory_catalog::{Declaration, MemoryCatalog, State},
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
        ("info", vec![0xd4000521u32, 0xd65f03c0]),
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
    let dir = root().join("private/testing/process-queries").join(label);
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
            (0x10000, b"info\0".as_slice()),
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

use mkw_runtime::{
    owned_memory_budget::OwnedMemoryBudget,
    process_entropy::ProcessEntropy,
    process_queries::{self, Abi},
    process_regions::{Layout, ProcessRegions},
};
fn regions(m: Arc<Mutex<ExclusiveMemory>>, h: &Heap) -> ProcessRegions {
    ProcessRegions::new(
        m,
        h,
        Layout {
            address_space: span(0, 0x20000),
            code: span(0x4000, 0x3000),
            stack: span(0x2000, 0x2000),
            heap: span(0x10000, 8192),
            alias: span(0x14000, 4096),
            remote: span(0x16000, 4096),
        },
    )
    .unwrap()
}
fn budget(m: Arc<Mutex<ExclusiveMemory>>) -> OwnedMemoryBudget {
    OwnedMemoryBudget::new(m, 0x20000).unwrap()
}
fn aslr() -> Span {
    span(0x1000, 0x17000)
}
fn service(
    m: Arc<Mutex<ExclusiveMemory>>,
    h: &Heap,
    c: MemoryCatalog,
    e: Option<&ProcessEntropy>,
) -> Services {
    process_queries::registry(
        Abi::Nx64PrivateOwnedProcess,
        budget(m.clone()),
        regions(m, h),
        c,
        aslr(),
        e,
    )
    .unwrap()
}
fn alternate_catalog(m: Arc<Mutex<ExclusiveMemory>>, h: &Heap, mode: usize) -> MemoryCatalog {
    let mut d = Vec::new();
    for (base, state) in [
        (0x2000, State::Stack),
        (0x3000, State::Stack),
        (0x4000, State::Code),
        (0x5000, State::CodeData),
        (0x6000, State::ThreadLocal),
    ] {
        let state = if mode == 1 && base == 0x2000 {
            State::CodeData
        } else if mode == 2 && base == 0x5000 {
            State::Stack
        } else {
            state
        };
        d.push(Declaration {
            span: span(base, 4096),
            state,
        });
    }
    MemoryCatalog::new(m, h, span(0, if mode == 0 { 0x30000 } else { 0x20000 }), d).unwrap()
}
#[test]
fn individually_valid_but_inconsistent_capabilities_are_rejected() {
    for mode in 0..7 {
        let (m, h, c) = fixture(0);
        let (f, fh, fc) = fixture(0);
        let b = if mode == 0 {
            budget(f.clone())
        } else {
            budget(m.clone())
        };
        let r = if mode == 1 {
            regions(f, &fh)
        } else {
            regions(m.clone(), &h)
        };
        let c = match mode {
            2 => fc,
            3..=5 => alternate_catalog(m, &h, mode - 3),
            _ => c,
        };
        let a = if mode == 6 {
            span(0x4000, 4096)
        } else {
            aslr()
        };
        assert!(
            process_queries::registry(Abi::Nx64PrivateOwnedProcess, b, r, c, a, None).is_err(),
            "{mode}"
        );
    }
}
#[test]
fn native_composed_queries_agree_across_heap_sizes_and_budgets() {
    for opt in ["-O0", "-O2"] {
        let lib = build("composition", opt);
        let r = router(&lib);
        let (m, h, catalog) = fixture(0);
        let entropy = ProcessEntropy::from_synthetic_words(
            mkw_runtime::process_entropy::Abi::Horizon64EntropyReference,
            [7, 8, 9, 10],
        );
        let s = service(m.clone(), &h, catalog, Some(&entropy));
        let mut b = Bridge::new(m.clone()).unwrap();
        for size in [0, 4096, 8192] {
            m.lock().unwrap().resize_heap(&h, size).unwrap();
            for steps in 0..=3 {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x2100), &[0xaa; 40])
                    .unwrap();
                let mut c = cpu(0);
                c.base.x[2] = 0x10000;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x4000, steps).0,
                    if steps < 2 { 2 } else { 0 }
                );
                assert_eq!(
                    bytes(&m, 0x2100, 40),
                    if steps == 0 {
                        vec![0xaa; 40]
                    } else if size == 0 {
                        packet(0x7000, 0x19000, 0, 0)
                    } else {
                        packet(0x10000, size as u64, 5, 3)
                    }
                );
                for (kind, value) in [
                    (5, 8192),
                    (12, 0x1000),
                    (13, 0x17000),
                    (21, 0x20000),
                    (22, 20480 + size as u64),
                    (11, 7),
                ] {
                    let mut c = cpu(0);
                    c.base.x[1] = kind;
                    c.base.x[2] = if kind == 11 { 0 } else { 0xffff8001 };
                    c.base.x[3] = 0;
                    assert_eq!(
                        run(&mut b, &r, &s, &mut c, 0x10000, steps).0,
                        if steps < 2 { 2 } else { 0 }
                    );
                    assert_eq!(c.base.x[1], if steps > 0 { value } else { kind });
                }
            }
        }
        assert!(s.combined(&entropy.registry()).is_err());
    }
}
#[test]
fn both_handlers_refuse_foreign_replaced_and_restricted_contexts() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let (m, h, c) = fixture(0);
            let mut s = service(m.clone(), &h, c, None);
            let target = if mode == 0 { fixture(0).0 } else { m.clone() };
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
            let mut b = Bridge::new(target).unwrap();
            for at in [0x4000, 0x10000] {
                let mut c = cpu(0);
                if at == 0x10000 {
                    c.base.x[1] = 5;
                    c.base.x[2] = 0xffff8001;
                    c.base.x[3] = 0;
                }
                let before = c.base.x;
                assert_eq!(run(&mut b, &r, &s, &mut c, at, 2).0, 3);
                assert_eq!(c.base.x, before);
            }
        }
        let (m, h, c) = fixture(0);
        let s = service(m.clone(), &h, c, None);
        let mut b = Bridge::new(m).unwrap();
        let mut c = cpu(0);
        c.base.x[1] = 11;
        c.base.x[2] = 0;
        c.base.x[3] = 0;
        assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2).0, 3);
    }
}
