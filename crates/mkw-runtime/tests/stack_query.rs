use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    services::Services,
    stack_query::{Abi, StackQuery},
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
    let dir = root().join("private/testing/stack-query").join(label);
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
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x2000),
        vec![0; 4096],
        Permissions::RW,
        "synthetic stack/output",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 1,
            },
        )
        .unwrap(),
    ))
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
fn svc(m: &Arc<Mutex<ExclusiveMemory>>) -> Services {
    StackQuery::new(Abi::Horizon64OwnedStack, m.clone(), GuestAddr(0x2000), 4096)
        .unwrap()
        .registry()
}
fn bytes(m: &Arc<Mutex<ExclusiveMemory>>, at: u64, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| {
            m.lock()
                .unwrap()
                .read(GuestAddr(at + i as u64), Width::Byte)
                .unwrap() as u8
        })
        .collect()
}
fn expected() -> Vec<u8> {
    [
        0x2000u64.to_le_bytes().as_slice(),
        4096u64.to_le_bytes().as_slice(),
        &[
            11, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
    ]
    .concat()
}
#[test]
fn configuration_requires_an_exact_page_aligned_rw_mapping() {
    let m = memory();
    for (at, size) in [
        (0x2000, 0),
        (0x2000, 4095),
        (0x2001, 4096),
        (0x1000, 4096),
        (0x2000, 8192),
        (0x3000, 4096),
        (u64::MAX - 4095, 8192),
    ] {
        assert!(StackQuery::new(Abi::Horizon64OwnedStack, m.clone(), GuestAddr(at), size).is_err());
    }
    for permissions in [
        Permissions::R,
        Permissions::RX,
        Permissions {
            read: true,
            write: true,
            execute: true,
        },
    ] {
        let mut raw = Memory::default();
        raw.map(GuestAddr(0x2000), vec![0; 4096], permissions, "stack")
            .unwrap();
        let mem = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        assert!(StackQuery::new(Abi::Horizon64OwnedStack, mem, GuestAddr(0x2000), 4096).is_err());
    }
    let mut raw = Memory::default();
    raw.map(
        GuestAddr(0x2000),
        vec![0; 8192],
        Permissions::RW,
        "not inferred from name",
    )
    .unwrap();
    let mem = Arc::new(Mutex::new(
        ExclusiveMemory::new(
            raw,
            Config {
                granule: 16,
                max_owners: 1,
            },
        )
        .unwrap(),
    ));
    assert!(StackQuery::new(
        Abi::Horizon64OwnedStack,
        mem.clone(),
        GuestAddr(0x2000),
        4096
    )
    .is_err());
    assert!(StackQuery::new(Abi::Horizon64OwnedStack, mem, GuestAddr(0x2000), 8192).is_ok());
}
#[test]
fn native_stack_queries_have_exact_packets_budgets_boundaries_and_atomic_faults() {
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        for query in [
            0x1fff,
            0x2000,
            0x2800,
            0x2fff,
            0x3000,
            0x100002000,
            u64::MAX,
        ] {
            for out in [0x2100, 0x2fd8, 0x2fd9, 0x3000, u64::MAX] {
                for limit in 0..=3 {
                    let m = memory();
                    let services = svc(&m);
                    let mut bridge = Bridge::new(m.clone()).unwrap();
                    let mut c = cpu(0xffffffffffffffff);
                    c.base.x[0] = out;
                    c.base.x[2] = query;
                    let old = c.base.x;
                    let known = (0x2000..0x3000).contains(&query);
                    let writable = out == 0x2100 || out == 0x2fd8;
                    let (status, left) = run(&mut bridge, &r, &services, &mut c, 0x4000, limit);
                    let want = if limit == 0 {
                        2
                    } else if !known {
                        3
                    } else if !writable {
                        5
                    } else if limit == 1 {
                        2
                    } else {
                        0
                    };
                    assert_eq!(status, want, "{query:x} {out:x} {limit}");
                    assert_eq!(
                        left,
                        limit.saturating_sub(if known && writable { 2 } else { 1 })
                    );
                    assert_eq!(&c.base.x[2..], &old[2..]);
                    if limit > 0 && known && writable {
                        assert_eq!(&c.base.x[..2], &[0, 0]);
                        assert_eq!(bytes(&m, out, 40), expected());
                    } else {
                        assert_eq!(c.base.x, old);
                        assert_eq!(bytes(&m, 0x2000, 4096), vec![0; 4096]);
                    }
                }
            }
        }
    }
}
#[test]
fn authority_namespace_generation_and_final_allowlist_are_required() {
    for opt in ["-O0", "-O2"] {
        let lib = build("authority", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let m = memory();
            let mut services = svc(&m);
            let actual = if mode == 0 { memory() } else { m.clone() };
            if mode == 1 {
                *m.lock().unwrap() = Arc::try_unwrap(memory()).unwrap().into_inner().unwrap();
            }
            if mode == 2 {
                services = services.restricted_to(&Default::default());
            }
            let mut bridge = Bridge::new(actual.clone()).unwrap();
            let mut c = cpu(0x7777);
            let old = c.base.x;
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x4000, 2), (3, 1));
            assert_eq!(c.base.x, old);
            assert_eq!(bytes(&actual, 0x2000, 4096), vec![0; 4096]);
        }
        let m = memory();
        assert!(svc(&m).combined(&svc(&m)).is_err());
    }
}
#[test]
fn native_wrapper_preserves_completed_svc_effects_on_later_store_fault() {
    for opt in ["-O0", "-O2"] {
        let lib = build("wrapper", opt);
        let r = router(&lib);
        for output in [0x2200, 0x3000] {
            for limit in 0..=7 {
                let m = memory();
                let services = svc(&m);
                let mut bridge = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(output);
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x2200), &[0xff; 8])
                    .unwrap();
                let (status, _) = run(&mut bridge, &r, &services, &mut c, 0x8000, limit);
                let want = if limit < 4 {
                    2
                } else if output == 0x3000 {
                    5
                } else if limit < 6 {
                    2
                } else {
                    0
                };
                assert_eq!(status, want);
                assert_eq!(
                    bytes(&m, 0x2100, 40),
                    if limit >= 2 { expected() } else { vec![0; 40] }
                );
                assert_eq!(
                    bytes(&m, 0x2200, 8),
                    if output == 0x2200 && limit >= 4 {
                        vec![0, 0, 0, 0, 255, 255, 255, 255]
                    } else {
                        vec![255; 8]
                    }
                );
                assert_eq!(
                    c.base.sp,
                    if limit == 0 || (output == 0x2200 && limit >= 5) {
                        0x2800
                    } else {
                        0x27f0
                    }
                );
            }
        }
    }
}
#[test]
fn completed_packets_clear_monitors_but_refusals_and_output_faults_preserve_them() {
    for opt in ["-O0", "-O2"] {
        let lib = build("monitor", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let m = memory();
            let services = svc(&m);
            let mut bridge = Bridge::new(m.clone()).unwrap();
            let mut c = cpu(0);
            c.base.x[5] = 0x2400;
            c.base.x[6] = 42;
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x6000, 2), (0, 0));
            if mode == 1 {
                c.base.x[2] = 0x3000;
            }
            if mode == 2 {
                c.base.x[0] = 0x2fff;
            }
            assert_eq!(
                run(&mut bridge, &r, &services, &mut c, 0x4000, 2).0,
                [0, 3, 5][mode]
            );
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x7000, 2), (0, 0));
            assert_eq!(c.base.x[7], u64::from(mode == 0));
            assert_eq!(
                m.lock()
                    .unwrap()
                    .read(GuestAddr(0x2400), Width::Double)
                    .unwrap(),
                if mode == 0 { 0 } else { 42 }
            );
        }
    }
}
#[test]
fn read_only_output_rejects_without_writing_or_clobbering_registers() {
    for opt in ["-O0", "-O2"] {
        let lib = build("readonly", opt);
        let r = router(&lib);
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(0x2000),
            vec![0; 4096],
            Permissions::RW,
            "explicit stack",
        )
        .unwrap();
        raw.map(
            GuestAddr(0x3000),
            vec![0x55; 4096],
            Permissions::R,
            "output",
        )
        .unwrap();
        let m = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let services = svc(&m);
        let mut bridge = Bridge::new(m.clone()).unwrap();
        for out in [0x3000, 0x2ff0] {
            let mut c = cpu(17);
            c.base.x[0] = out;
            let old = c.base.x;
            assert_eq!(run(&mut bridge, &r, &services, &mut c, 0x4000, 2), (5, 1));
            assert_eq!(c.base.x, old);
            assert_eq!(bytes(&m, 0x2ff0, 16), vec![0; 16]);
            assert_eq!(bytes(&m, 0x3000, 40), vec![0x55; 40]);
        }
    }
}
