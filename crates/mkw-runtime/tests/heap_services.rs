use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Heap, HeapSpec, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    heap_services::{Abi, HeapServices, INVALID_SIZE, OUT_OF_MEMORY},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
const MIB: usize = 1024 * 1024;
const BASE: u64 = 0x1_0080_0000;
fn owned() -> (Arc<Mutex<ExclusiveMemory>>, Heap) {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x2000),
        vec![0; 4096],
        Permissions::RW,
        "synthetic stack",
    )
    .unwrap();
    let (m, h) = ExclusiveMemory::with_heap(
        m,
        Config {
            granule: 16,
            max_owners: 4,
        },
        HeapSpec {
            base: GuestAddr(BASE),
            capacity: 4 * MIB,
        },
    )
    .unwrap();
    (Arc::new(Mutex::new(m)), h)
}
fn services(m: &Arc<Mutex<ExclusiveMemory>>, h: &Heap) -> Services {
    HeapServices::new(Abi::Nx64ZeroedBoundedHeap, m.clone(), h.clone())
        .unwrap()
        .registry()
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(label: &str, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        ("heap", vec![0xd4000021u32, 0xd65f03c0]),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("conditional", vec![0xc807fca6, 0xd65f03c0]),
        (
            "wrapper",
            vec![
                0xf81f0fe0, 0xd4000021, 0xf94003e2, 0xf9000041, 0x910043ff, 0xd65f03c0,
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
    let dir = root().join("private/testing/heap-services").join(label);
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
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x10000, b"heap\0".as_slice()),
            (0x11000, b"reserve\0".as_slice()),
            (0x12000, b"conditional\0".as_slice()),
            (0x13000, b"wrapper\0".as_slice()),
        ]
        .into_iter()
        .map(|(at, n)| unsafe { Target::load_services(lib, n, GuestAddr(at), at).unwrap() })
        .collect(),
        4,
    )
    .unwrap()
}
fn cpu(size: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x = [0x123456789abcdef0; 31];
    c.base.x[1] = size;
    c.base.x[30] = 0x20000;
    c.base.sp = 0x2800;
    c
}
fn run(
    b: &mut Bridge,
    r: &Router<'_>,
    s: &Services,
    c: &mut CpuV10,
    at: u64,
    budget: u64,
) -> (i32, u64) {
    let mut left = budget;
    let status = {
        let mut a = b.attach(c).unwrap();
        let mut rt = Routed::attach_services(&mut a, r, s).unwrap();
        rt.run(GuestAddr(at), &mut left).unwrap()
    };
    (status, left)
}
#[test]
fn native_size_results_budgets_and_zeroed_regrowth_follow_explicit_reference() {
    assert_eq!((INVALID_SIZE, OUT_OF_MEMORY), (0xca01, 0xd001));
    for opt in ["-O0", "-O2"] {
        let lib = build("matrix", opt);
        let r = router(&lib);
        let (m, h) = owned();
        let s = services(&m, &h);
        let mut b = Bridge::new(m.clone()).unwrap();
        for size in [
            0,
            2 * MIB as u64,
            4 * MIB as u64,
            6 * MIB as u64,
            4096,
            1,
            1u64 << 32,
            8u64 << 30,
            u64::MAX,
        ] {
            for budget in 0..=3 {
                m.lock().unwrap().resize_heap(&h, 2 * MIB).unwrap();
                m.lock()
                    .unwrap()
                    .write(GuestAddr(BASE), &[0x77; 8])
                    .unwrap();
                let mut c = cpu(size);
                let old = c.base.x;
                assert_eq!(
                    run(&mut b, &r, &s, &mut c, 0x10000, budget),
                    if budget < 2 { (2, 0) } else { (0, budget - 2) }
                );
                let code = if size % (2 * MIB as u64) != 0 || size >= 8u64 << 30 {
                    INVALID_SIZE
                } else if size > 4 * MIB as u64 {
                    OUT_OF_MEMORY
                } else {
                    0
                };
                assert_eq!(
                    c.base.x[0],
                    if budget == 0 { old[0] } else { u64::from(code) }
                );
                assert_eq!(
                    c.base.x[1],
                    if budget > 0 && code == 0 { BASE } else { size }
                );
                assert_eq!(&c.base.x[2..], &old[2..]);
                let want = if budget > 0 && code == 0 {
                    size as usize
                } else {
                    2 * MIB
                };
                assert_eq!(m.lock().unwrap().heap_size(&h), Ok(want));
                if want > 0 {
                    assert_eq!(
                        m.lock().unwrap().read(GuestAddr(BASE), Width::Double),
                        Ok(0x7777777777777777)
                    );
                }
                if want > 2 * MIB {
                    assert_eq!(
                        m.lock()
                            .unwrap()
                            .read(GuestAddr(BASE + 2 * MIB as u64), Width::Double),
                        Ok(0)
                    );
                }
            }
        }
        m.lock().unwrap().resize_heap(&h, 4 * MIB).unwrap();
        m.lock()
            .unwrap()
            .write(GuestAddr(BASE + 2 * MIB as u64), &[0x88; 8])
            .unwrap();
        for size in [2 * MIB, 4 * MIB] {
            let mut c = cpu(size as u64);
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), (0, 0));
        }
        assert_eq!(
            m.lock()
                .unwrap()
                .read(GuestAddr(BASE + 2 * MIB as u64), Width::Double),
            Ok(0)
        );
    }
}
#[test]
fn native_wrapper_can_fault_after_a_committed_resize() {
    for opt in ["-O0", "-O2"] {
        let lib = build("wrapper", opt);
        let r = router(&lib);
        for dest in [0x2100, 0x3000] {
            for budget in 0..=7 {
                let (m, h) = owned();
                let s = services(&m, &h);
                let mut b = Bridge::new(m.clone()).unwrap();
                let mut c = cpu(2 * MIB as u64);
                c.base.x[0] = dest;
                let (status, _) = run(&mut b, &r, &s, &mut c, 0x13000, budget);
                assert_eq!(
                    status,
                    if budget < 4 {
                        2
                    } else if dest == 0x3000 {
                        5
                    } else if budget < 6 {
                        2
                    } else {
                        0
                    }
                );
                assert_eq!(
                    m.lock().unwrap().heap_size(&h),
                    Ok(if budget >= 2 { 2 * MIB } else { 0 })
                );
                assert_eq!(
                    m.lock().unwrap().read(GuestAddr(0x2100), Width::Double),
                    Ok(if dest == 0x2100 && budget >= 4 {
                        BASE as u128
                    } else {
                        0
                    })
                );
            }
        }
    }
}
#[test]
fn authority_replacement_and_policy_refuse_without_effects() {
    for opt in ["-O0", "-O2"] {
        let lib = build("scope", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let (m, h) = owned();
            let mut s = services(&m, &h);
            let (actual, actual_h) = if mode == 0 {
                owned()
            } else {
                (m.clone(), h.clone())
            };
            let actual_h = if mode == 1 {
                let (n, next) = owned();
                *m.lock().unwrap() = Arc::try_unwrap(n).unwrap().into_inner().unwrap();
                next
            } else {
                actual_h
            };
            if mode == 2 {
                s = s.restricted_to(&Default::default());
            }
            let mut b = Bridge::new(actual.clone()).unwrap();
            let mut c = cpu(2 * MIB as u64);
            let old = c.base.x;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x10000, 2), (3, 1));
            assert_eq!(c.base.x, old);
            assert_eq!(actual.lock().unwrap().heap_size(&actual_h), Ok(0));
        }
        let (m, h) = owned();
        let (other, foreign) = owned();
        assert!(HeapServices::new(Abi::Nx64ZeroedBoundedHeap, m.clone(), foreign).is_err());
        drop(other);
        assert!(services(&m, &h).combined(&services(&m, &h)).is_err());
    }
}
#[test]
fn completed_service_clears_own_monitor_and_preserves_unaffected_other_owners() {
    for opt in ["-O0", "-O2"] {
        let lib = build("monitor", opt);
        let r = router(&lib);
        for mode in 0..3 {
            let (m, h) = owned();
            m.lock().unwrap().resize_heap(&h, 4 * MIB).unwrap();
            let s = services(&m, &h);
            let denied = s.restricted_to(&Default::default());
            let mut b = Bridge::new(m.clone()).unwrap();
            let other = m.lock().unwrap().new_owner().unwrap();
            m.lock()
                .unwrap()
                .load(&other, GuestAddr(BASE), Width::Double)
                .unwrap();
            let mut c = cpu(if mode == 1 { 1 } else { 2 * MIB as u64 });
            c.base.x[5] = 0x2000;
            c.base.x[6] = 99;
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x11000, 2), (0, 0));
            assert_eq!(
                run(
                    &mut b,
                    &r,
                    if mode == 2 { &denied } else { &s },
                    &mut c,
                    0x10000,
                    2
                )
                .0,
                if mode == 2 { 3 } else { 0 }
            );
            assert_eq!(run(&mut b, &r, &s, &mut c, 0x12000, 2), (0, 0));
            assert_eq!(c.base.x[7], u64::from(mode != 2));
            assert!(m
                .lock()
                .unwrap()
                .store_conditional(&other, GuestAddr(BASE), Width::Double, 77)
                .unwrap());
        }
    }
}
#[test]
fn concurrent_services_serialize_heap_changes_and_keep_retained_prefix() {
    for opt in ["-O0", "-O2"] {
        let lib = build("concurrent", opt);
        let r = router(&lib);
        let (m, h) = owned();
        m.lock().unwrap().resize_heap(&h, 4 * MIB).unwrap();
        m.lock()
            .unwrap()
            .write(GuestAddr(BASE), &[0x33; 8])
            .unwrap();
        let s = services(&m, &h);
        std::thread::scope(|scope| {
            for thread in 0..4 {
                let m = m.clone();
                let r = &r;
                let s = &s;
                scope.spawn(move || {
                    let mut b = Bridge::new(m).unwrap();
                    for j in 0..32 {
                        let mut c = cpu(((2 + 2 * ((thread + j) % 2)) * MIB) as u64);
                        assert_eq!(run(&mut b, r, s, &mut c, 0x10000, 2), (0, 0));
                        assert_eq!(&c.base.x[..2], &[0, BASE]);
                    }
                });
            }
        });
        assert!(matches!(m.lock().unwrap().heap_size(&h),Ok(n) if n==2*MIB||n==4*MIB));
        assert_eq!(
            m.lock().unwrap().read(GuestAddr(BASE), Width::Double),
            Ok(0x3333333333333333)
        );
    }
}
