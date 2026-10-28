use mkw_runtime::{
    exclusive::{Config, Error, ExclusiveMemory, Heap, HeapSpec, Width},
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
fn cfg() -> Config {
    Config {
        granule: 16,
        max_owners: 4,
    }
}
fn raw() -> Memory {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x2000),
        vec![0x55; 4096],
        Permissions::RW,
        "fixed stack",
    )
    .unwrap();
    m
}
fn owned() -> (ExclusiveMemory, Heap) {
    ExclusiveMemory::with_heap(
        raw(),
        cfg(),
        HeapSpec {
            base: GuestAddr(0x4000),
            capacity: 8192,
        },
    )
    .unwrap()
}
#[test]
fn explicit_heap_reservation_checks_full_capacity_and_keeps_default_memory_sealed() {
    for (base, capacity) in [
        (0x4000, 0),
        (0x4001, 4096),
        (0x4000, 4095),
        (0x1000, 8192),
        (0x2000, 4096),
        (u64::MAX - 4095, 8192),
        (0x4000, mkw_formats::MAX_IMAGE + 4096),
    ] {
        assert!(ExclusiveMemory::with_heap(
            raw(),
            cfg(),
            HeapSpec {
                base: GuestAddr(base),
                capacity
            }
        )
        .is_err());
    }
    let mut m = raw();
    m.seal();
    assert!(m
        .map(GuestAddr(0x4000), vec![0; 4096], Permissions::RW, "late")
        .is_err());
    assert!(ExclusiveMemory::with_heap(
        m,
        cfg(),
        HeapSpec {
            base: GuestAddr(0x4000),
            capacity: 4096
        }
    )
    .is_err());
    let (m, h) = owned();
    assert_eq!(m.heap_size(&h), Ok(0));
    assert_eq!(h.spec().base, GuestAddr(0x4000));
    assert_eq!(h.spec().capacity, 8192);
    assert!(m.read(GuestAddr(0x4000), Width::Byte).is_err());
    assert_eq!(m.read(GuestAddr(0x2000), Width::Byte), Ok(0x55));
}
#[test]
fn grow_shrink_preserve_prefix_zero_changed_bytes_and_reject_cross_generation_caps() {
    let (mut m, h) = owned();
    m.resize_heap(&h, 8192).unwrap();
    m.write(GuestAddr(0x4000), &[7; 16]).unwrap();
    m.write(GuestAddr(0x5000), &[9; 16]).unwrap();
    m.resize_heap(&h, 4096).unwrap();
    assert!(m.read(GuestAddr(0x5000), Width::Byte).is_err());
    assert_eq!(m.read(GuestAddr(0x4000), Width::Byte), Ok(7));
    m.resize_heap(&h.clone(), 8192).unwrap();
    assert_eq!(m.read(GuestAddr(0x5000), Width::Pair), Ok(0));
    assert_eq!(m.read(GuestAddr(0x4000), Width::Byte), Ok(7));
    for n in [1, 4095, 8193, usize::MAX] {
        assert!(m.resize_heap(&h, n).is_err());
        assert_eq!(m.heap_size(&h), Ok(8192));
        assert_eq!(m.read(GuestAddr(0x4000), Width::Byte), Ok(7));
    }
    let (mut other, foreign) = owned();
    assert_eq!(other.resize_heap(&h, 4096), Err(Error::Owner));
    assert_eq!(m.heap_size(&foreign), Err(Error::Owner));
    m.resize_heap(&h, 0).unwrap();
    assert!(m.read(GuestAddr(0x4000), Width::Byte).is_err());
    m.resize_heap(&h, 8192).unwrap();
    assert_eq!(m.read(GuestAddr(0x4000), Width::Pair), Ok(0));
    assert_eq!(m.read(GuestAddr(0x2000), Width::Byte), Ok(0x55));
}
#[test]
fn resize_invalidates_only_overlapping_reservation_granules_and_prevents_aba() {
    let (mut m, h) = owned();
    m.resize_heap(&h, 8192).unwrap();
    let a = m.new_owner().unwrap();
    let b = m.new_owner().unwrap();
    let fixed = m.new_owner().unwrap();
    m.load(&a, GuestAddr(0x4000), Width::Double).unwrap();
    m.load(&b, GuestAddr(0x5000), Width::Double).unwrap();
    m.load(&fixed, GuestAddr(0x2000), Width::Double).unwrap();
    m.resize_heap(&h, 4096).unwrap();
    m.resize_heap(&h, 8192).unwrap();
    assert!(!m
        .store_conditional(&b, GuestAddr(0x5000), Width::Double, 99)
        .unwrap());
    assert!(m
        .store_conditional(&a, GuestAddr(0x4000), Width::Double, 42)
        .unwrap());
    assert!(m
        .store_conditional(&fixed, GuestAddr(0x2000), Width::Double, 43)
        .unwrap());
    m.load(&a, GuestAddr(0x4000), Width::Double).unwrap();
    m.resize_heap(&h, 8192).unwrap();
    assert!(m.resize_heap(&h, 8193).is_err());
    assert!(m
        .store_conditional(&a, GuestAddr(0x4000), Width::Double, 44)
        .unwrap());
    let (mut m, h) = ExclusiveMemory::with_heap(
        raw(),
        Config {
            granule: 8192,
            max_owners: 1,
        },
        HeapSpec {
            base: GuestAddr(0x4000),
            capacity: 8192,
        },
    )
    .unwrap();
    m.resize_heap(&h, 8192).unwrap();
    let a = m.new_owner().unwrap();
    m.load(&a, GuestAddr(0x4000), Width::Double).unwrap();
    m.resize_heap(&h, 4096).unwrap();
    assert!(!m
        .store_conditional(&a, GuestAddr(0x4000), Width::Double, 55)
        .unwrap());
}
#[test]
fn mutable_heap_cannot_be_misclassified_as_an_immutable_stack() {
    let (m, h) = owned();
    let mem = Arc::new(Mutex::new(m));
    for n in [0, 4096, 8192, 0] {
        mem.lock().unwrap().resize_heap(&h, n).unwrap();
        assert!(StackQuery::new(
            Abi::Horizon64OwnedStack,
            mem.clone(),
            GuestAddr(0x4000),
            n.max(4096)
        )
        .is_err());
        assert!(StackQuery::new(
            Abi::Horizon64OwnedStack,
            mem.clone(),
            GuestAddr(0x2000),
            4096
        )
        .is_ok());
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn build(opt: &str) -> libloading::Library {
    let mut source = String::new();
    for (name, words) in [
        ("load", vec![0xf9400020u32, 0xd65f03c0]),
        ("store", vec![0xf9000020, 0xd65f03c0]),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("conditional", vec![0xc807fca6, 0xd65f03c0]),
    ] {
        let bytes = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        source += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let dir = root().join("private/testing/heap-memory");
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
fn run(b: &mut Bridge, r: &Router<'_>, c: &mut CpuV10, at: u64, budget: u64) -> (i32, u64) {
    let svc = Services::new(vec![]).unwrap();
    let mut left = budget;
    let status = {
        let mut a = b.attach(c).unwrap();
        let mut rt = Routed::attach_services(&mut a, r, &svc).unwrap();
        rt.run(GuestAddr(at), &mut left).unwrap()
    };
    (status, left)
}
#[test]
fn native_transport_observes_live_heap_bounds_without_replacing_its_memory_authority() {
    for opt in ["-O0", "-O2"] {
        let lib = build(opt);
        let r = Router::new(
            [
                (0x10000, b"load\0".as_slice()),
                (0x11000, b"store\0".as_slice()),
                (0x12000, b"reserve\0".as_slice()),
                (0x13000, b"conditional\0".as_slice()),
            ]
            .into_iter()
            .map(|(at, n)| unsafe { Target::load_services(&lib, n, GuestAddr(at), at).unwrap() })
            .collect(),
            4,
        )
        .unwrap();
        let (m, h) = owned();
        let mem = Arc::new(Mutex::new(m));
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        for size in [0, 4096, 8192, 4096, 0, 8192] {
            mem.lock().unwrap().resize_heap(&h, size).unwrap();
            for offset in [0, 4088, 4092, 4096, 8184, 8192] {
                for budget in 0..=3 {
                    let mut c = CpuV10::default();
                    c.base.x[0] = 0x12345678;
                    c.base.x[1] = 0x4000 + offset;
                    c.base.x[30] = 0x20000;
                    let accessible = offset + 8 <= size as u64;
                    let expected = if budget == 0 {
                        2
                    } else if !accessible {
                        5
                    } else if budget == 1 {
                        2
                    } else {
                        0
                    };
                    assert_eq!(run(&mut bridge, &r, &mut c, 0x11000, budget).0, expected);
                    if budget > 0 && accessible {
                        assert_eq!(
                            mem.lock()
                                .unwrap()
                                .read(GuestAddr(0x4000 + offset), Width::Double),
                            Ok(0x12345678)
                        );
                    }
                    c.base.x[0] = 0xabcdef;
                    assert_eq!(run(&mut bridge, &r, &mut c, 0x10000, budget).0, expected);
                    assert_eq!(
                        c.base.x[0],
                        if budget > 0 && accessible {
                            0x12345678
                        } else {
                            0xabcdef
                        }
                    );
                }
            }
        }
        let mut c = CpuV10::default();
        c.base.x[30] = 0x20000;
        c.base.x[5] = 0x5000;
        c.base.x[6] = 99;
        assert_eq!(run(&mut bridge, &r, &mut c, 0x12000, 2), (0, 0));
        mem.lock().unwrap().resize_heap(&h, 4096).unwrap();
        mem.lock().unwrap().resize_heap(&h, 8192).unwrap();
        assert_eq!(run(&mut bridge, &r, &mut c, 0x13000, 2), (0, 0));
        assert_eq!(c.base.x[7], 1);
        mem.lock().unwrap().resize_heap(&h, 4096).unwrap();
        mem.lock()
            .unwrap()
            .write(GuestAddr(0x4ffc), &[0x77; 4])
            .unwrap();
        c.base.x[0] = 0;
        c.base.x[1] = 0x4ffc;
        assert_eq!(run(&mut bridge, &r, &mut c, 0x11000, 2).0, 5);
        assert_eq!(
            mem.lock().unwrap().read(GuestAddr(0x4ffc), Width::Word),
            Ok(0x77777777)
        );
    }
}
