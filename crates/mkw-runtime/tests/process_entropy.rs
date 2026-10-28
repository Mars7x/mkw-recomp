use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    process_entropy::{Abi, ProcessEntropy, GET_INFO, INVALID_COMBINATION, INVALID_HANDLE},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/process-entropy")
        .join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut text = String::new();
    for (name, words) in [
        (
            "entropy",
            vec![0xd4000001u32 | (u32::from(GET_INFO) << 5), 0xd65f03c0],
        ),
        ("reserve", vec![0xc85ffca4, 0xd65f03c0]),
        ("store", vec![0xc807fca6, 0xd65f03c0]),
    ] {
        let bytes = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        text += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let src = path.join("synthetic.c");
    std::fs::write(&src, text).unwrap();
    let lib = path.join(format!("synthetic{opt}.so"));
    let output = Command::new("cc")
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
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    std::fs::write(path.join(format!("compiler{opt}.log")), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    Router::new(
        [
            (0x4000, b"entropy\0".as_slice()),
            (0x5000, b"reserve\0".as_slice()),
            (0x6000, b"store\0".as_slice()),
        ]
        .into_iter()
        .map(|(a, n)| unsafe { Target::load_services(lib, n, GuestAddr(a), a).unwrap() })
        .collect(),
        8,
    )
    .unwrap()
}
fn memory(array: &[u32]) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 0x2000],
        Permissions::RW,
        "author-created counter/array",
    )
    .unwrap();
    let bytes = array
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<_>>();
    m.map(
        GuestAddr(0x5000),
        bytes,
        Permissions::R,
        "read-only handle array",
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
fn run(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    cpu: &mut CpuV10,
    at: u64,
    budget: u64,
) -> (i32, u64) {
    let mut b = budget;
    let status = {
        let mut a = bridge.attach(cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        r.run(GuestAddr(at), &mut b).unwrap()
    };
    (status, b)
}
fn cpu(pointer: u64, count: u64, timeout: u64) -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0x1111222233334444;
    c.base.x[1] = pointer;
    c.base.x[2] = count;
    c.base.x[3] = timeout;
    c.base.x[30] = 0x9000;
    c.base.sp = 0x2800;
    c
}
#[test]
fn process_entropy_reference_svc_returns_stable_real_state_and_preserves_contract_boundaries() {
    assert_eq!(
        (GET_INFO, INVALID_HANDLE, INVALID_COMBINATION),
        (0x29, 0xe401, 0xe801)
    );
    let path = folder("reference");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let words = [0, 0x1122334455667788, 0x8000000000000001, u64::MAX];
        let source = ProcessEntropy::from_synthetic_words(Abi::Horizon64EntropyReference, words);
        let svc = source.registry();
        let clone = source.clone().registry();
        for services in [&svc, &clone] {
            for (index, expected) in words.into_iter().enumerate() {
                let mut bridge = Bridge::new(memory(&[])).unwrap();
                let mut c = cpu(11, 0, index as u64);
                let before = c.base.x;
                assert_eq!(
                    run(&mut bridge, &router, services, &mut c, 0x4000, 10),
                    (0, 8)
                );
                assert_eq!(&c.base.x[..2], &[0, expected]);
                assert_eq!(&c.base.x[2..], &before[2..]);
            }
        }
        let mut bridge = Bridge::new(memory(&[])).unwrap();
        let mut c = cpu(0xdeadbeef0000000b, 0xfeedface00000000, 2);
        assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
        assert_eq!(&c.base.x[..2], &[0, words[2]]);
        for (handle, index, error) in [
            (1, 4, INVALID_HANDLE),
            (0, 4, INVALID_COMBINATION),
            (0, u64::MAX, INVALID_COMBINATION),
            (0, 0xdeadbeef00000002, INVALID_COMBINATION),
        ] {
            let mut c = cpu(11, handle, index);
            let before = c.base.x;
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x4000, 10), (0, 8));
            assert_eq!(c.base.x[0], u64::from(error));
            assert_eq!(&c.base.x[1..], &before[1..]);
        }
        for services in [&svc, &Services::new(vec![]).unwrap()] {
            let mut c = cpu(12, 1, 99);
            let before = c.base.x;
            assert_eq!(
                run(&mut bridge, &router, services, &mut c, 0x4000, 10),
                (3, 9)
            );
            assert_eq!(c.base.x, before);
        }
        for budget in 0..3 {
            let mut c = cpu(11, 0, 2);
            let initial = c.base.x;
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 0x4000, budget),
                if budget < 2 { (2, 0) } else { (0, 0) }
            );
            assert_eq!(c.base.x[0], if budget == 0 { initial[0] } else { 0 });
            assert_eq!(c.base.x[1], if budget == 0 { 11 } else { words[2] });
        }
        for info in [11, 12] {
            let mem = memory(&[]);
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut c = cpu(info, 0, 0);
            c.base.x[5] = 0x2000;
            c.base.x[6] = 99;
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x5000, 10), (0, 8));
            let before = c.base.x;
            assert_eq!(
                run(&mut bridge, &router, &svc, &mut c, 0x4000, 10),
                if info == 11 { (0, 8) } else { (3, 9) }
            );
            if info == 12 {
                assert_eq!(c.base.x, before);
            }
            assert_eq!(run(&mut bridge, &router, &svc, &mut c, 0x6000, 10), (0, 8));
            assert_eq!(c.base.x[7], u64::from(info == 11));
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x2000), Width::Double)
                    .unwrap(),
                if info == 11 { 0 } else { 99 }
            );
        }
        let different =
            ProcessEntropy::from_synthetic_words(Abi::Horizon64EntropyReference, [9; 4]).registry();
        let mut c = cpu(11, 0, 2);
        assert_eq!(
            run(&mut bridge, &router, &different, &mut c, 0x4000, 10),
            (0, 8)
        );
        assert_eq!(c.base.x[1], 9);
    }
}
