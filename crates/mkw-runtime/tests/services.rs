use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{OpsV2, OpsV3, Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services, EXIT_THREAD},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/services-v3")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn svc(n: u16) -> u32 {
    0xd4000001 | (u32::from(n) << 5)
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut out = String::new();
    for (name, words) in [
        ("packet", vec![svc(0x77), 0xd65f03c0]),
        ("reserve", vec![0xaa0003f4, 0xc85ffe83, 0xd65f03c0]),
        (
            "interrupted",
            vec![0xaa0003f4, 0xc85ffe83, svc(0x77), 0xc804fe85, 0xd65f03c0],
        ),
        ("store", vec![0xc804fe85, 0xd65f03c0]),
        (
            "nested",
            vec![0xaa1e03f3, 0xd63f00c0, 0xf900001f, 0xaa1303fe, 0xd65f03c0],
        ),
        ("svc_exit", vec![svc(0x88), 0xf900001f, 0xd65f03c0]),
        ("close_call", vec![svc(0x99), 0xd65f03c0]),
        (
            "close_monitor",
            vec![0xc85ffc83, svc(0x99), 0xc805fc83, 0xd65f03c0],
        ),
    ] {
        let bytes = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect::<Vec<_>>();
        let ir = mkw_arm64::lift_function_services_v3(&bytes, 0, 0).unwrap();
        if name == "packet" {
            assert!(mkw_codegen::emit(&ir, 0).is_err());
            assert!(mkw_codegen::emit_exclusive_v10(&ir, 0).is_err());
            assert!(mkw_codegen::emit_exclusive_calls_v10(&ir, 0).is_err());
        }
        out += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let ir = mkw_arm64::lift(&[0xc0, 0x03, 0x5f, 0xd6], 0, 0).unwrap();
    out += &mkw_codegen::emit_exclusive_calls_v10(&ir, 0)
        .unwrap()
        .replace("mkw_guest_v10_budget", "legacy");
    out+="int exit_code(void){return MKW_EXIT_THREAD;} size_t size_v3(void){return sizeof(MkwServicesV3);} size_t offset_svc(void){return offsetof(MkwServicesV3,svc);} size_t offset_context(void){return offsetof(MkwServicesV3,svc_context);}\nunsigned wrong_marker_abi_version(void){return 10;}unsigned wrong_marker_exclusive_version(void){return 2;}\n";
    out += "unsigned foreign_abi_version(void){return 10;}unsigned foreign_exclusive_version(void){return 3;}int foreign(MkwCpuV10*c,uint64_t*b){(void)b;MkwCpuV10 other=*c;return mkw_svc_v10(&other,0x77); }\nint accepts(const MkwCpuV10*c){return mkw_services_v3_valid(c);}\n";
    let src = path.join("synthetic.c");
    std::fs::write(&src, out).unwrap();
    let lib = path.join(format!("synthetic{opt}.so"));
    let built = Command::new("cc")
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
    std::fs::write(path.join(format!("compiler{opt}.log")), &built.stderr).unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn router(lib: &libloading::Library) -> Router<'_> {
    let targets = [
        (0x4000, b"packet\0".as_slice()),
        (0x5000, b"reserve\0".as_slice()),
        (0x6000, b"store\0".as_slice()),
        (0x7000, b"interrupted\0".as_slice()),
        (0x8000, b"nested\0".as_slice()),
        (0xa000, b"svc_exit\0".as_slice()),
        (0xb000, b"close_call\0".as_slice()),
        (0xc000, b"close_monitor\0".as_slice()),
    ]
    .into_iter()
    .map(|(at, name)| unsafe { Target::load_services(lib, name, GuestAddr(at), at).unwrap() })
    .collect();
    Router::new(targets, 8).unwrap()
}
fn memory(owners: usize) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0x5a; 0x2000],
        Permissions::RW,
        "author-created data",
    )
    .unwrap();
    m.map(
        GuestAddr(0x3000),
        vec![0; 16],
        Permissions::R,
        "read-only fixture",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: owners,
            },
        )
        .unwrap(),
    ))
}
fn cpu() -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0x1000;
    c.base.x[1] = 0xfedcba9876543210;
    c.base.x[5] = 99;
    c.base.x[30] = 0x9000;
    c.base.n = 1;
    c.base.c = 1;
    c.base.sp = 0x2000;
    c
}
struct Packet(u32);
impl Service for Packet {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        let mut o = Outputs::default()
            .write_w(0, 0x89abcdef)?
            .write_x(1, 0xfedcba9876543210)?
            .write_w(2, 0x87654321)?;
        match self.0 {
            0 => {}
            1 => {
                let b = call.read_bytes(GuestAddr(a[0]), 8)?;
                o = o.write_memory(GuestAddr(a[0]), b)?;
            }
            2 => {
                o = o
                    .write_memory(GuestAddr(a[0]), vec![1; 8])?
                    .write_memory(GuestAddr(0x3000), vec![2; 8])?;
            }
            3 => return Err(HostFault::unsupported()),
            _ => o = o.write_memory(GuestAddr(a[0]), vec![1; 8])?,
        }
        Ok(Reply::Return(o))
    }
}
struct Exit;
impl Service for Exit {
    fn invoke(&self, _: &Call<'_>) -> Result<Reply, HostFault> {
        Ok(Reply::ThreadExit)
    }
}
struct Close;
impl Service for Close {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        if a[0] != u64::MAX {
            return Err(HostFault::unsupported());
        }
        Ok(Reply::Return(
            Outputs::default().write_x(0, (-9i64) as u64)?,
        ))
    }
}
fn services(packet: u32) -> Services {
    Services::new(vec![
        (0x77, Arc::new(Packet(packet))),
        (0x88, Arc::new(Exit)),
        (0x99, Arc::new(Close)),
    ])
    .unwrap()
}
#[test]
fn native_v3_layout_profile_loading_and_multi_register_budget_transport() {
    assert_eq!(
        std::mem::offset_of!(OpsV3, svc),
        std::mem::size_of::<OpsV2>()
    );
    let path = folder("packet");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> i32>(b"exit_code\0")
                    .unwrap()()
            },
            EXIT_THREAD
        );
        unsafe {
            for (name, expected) in [
                (b"size_v3\0".as_slice(), std::mem::size_of::<OpsV3>()),
                (b"offset_svc\0".as_slice(), std::mem::offset_of!(OpsV3, svc)),
                (
                    b"offset_context\0".as_slice(),
                    std::mem::offset_of!(OpsV3, svc_context),
                ),
            ] {
                let query = lib.get::<unsafe extern "C" fn() -> usize>(name).unwrap();
                assert_eq!(query(), expected)
            }
        }
        assert!(unsafe { Target::load(&lib, b"packet\0", GuestAddr(0x4000), 0x4000) }.is_err());
        assert!(
            unsafe { Target::load_services(&lib, b"legacy\0", GuestAddr(0x4000), 0x4000) }.is_err()
        );
        let err =
            unsafe { Target::load_services(&lib, b"wrong_marker\0", GuestAddr(0x4000), 0x4000) }
                .err()
                .unwrap();
        assert!(err.contains("version mismatch"));
        for budget in 0..5 {
            let m = memory(1);
            let mut bridge = Bridge::new(m).unwrap();
            let mut c = cpu();
            let before = c.base.x;
            let mut b = budget;
            let svc = services(0);
            {
                let mut a = bridge.attach(&mut c).unwrap();
                assert!(Routed::attach(&mut a, &router).is_err());
                let mut r = Routed::attach_services(&mut a, &router, &svc).unwrap();
                assert_eq!(
                    r.run(GuestAddr(0x4000), &mut b).unwrap(),
                    if budget >= 2 { 0 } else { 2 }
                );
                let x = &r.cpu_mut().base;
                assert_eq!(
                    x.pc,
                    if budget >= 2 {
                        0x9000
                    } else {
                        0x4000 + budget * 4
                    }
                );
                assert_eq!([x.n, x.z, x.c, x.v], [1, 0, 1, 0]);
                assert_eq!(x.sp, 0x2000);
                if budget >= 1 {
                    assert_eq!(&x.x[..3], &[0x89abcdef, 0xfedcba9876543210, 0x87654321]);
                    assert_eq!(&x.x[3..], &before[3..]);
                } else {
                    assert_eq!(x.x, before);
                }
            }
            assert_eq!(b, budget.saturating_sub(2));
        }
    }
}
#[test]
fn svc_packet_faults_unknown_handlers_and_successful_returns_have_exact_monitor_effects() {
    let path = folder("monitor");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        for kind in 0..5 {
            let m = memory(2);
            let mut one = Bridge::new(m.clone()).unwrap();
            let mut two = Bridge::new(m.clone()).unwrap();
            let mut c1 = cpu();
            let mut c2 = cpu();
            let svc = services(kind);
            let empty = Services::new(vec![]).unwrap();
            let registry = if kind == 4 { &empty } else { &svc };
            let before = m
                .lock()
                .unwrap()
                .read(GuestAddr(0x1000), Width::Double)
                .unwrap();
            let mut a1 = one.attach(&mut c1).unwrap();
            let mut a2 = two.attach(&mut c2).unwrap();
            let mut r1 = Routed::attach_services(&mut a1, &router, registry).unwrap();
            let mut r2 = Routed::attach_services(&mut a2, &router, &svc).unwrap();
            assert_eq!(r2.run(GuestAddr(0x5000), &mut 10).unwrap(), 0);
            let initial = r1.cpu_mut().base.x;
            let status = r1.run(GuestAddr(0x7000), &mut 20).unwrap();
            if kind < 2 {
                assert_eq!(status, 0);
                assert_eq!(r1.cpu_mut().base.x[4], 1);
            } else {
                assert_eq!(status, if kind == 2 { 5 } else { 3 });
                assert_eq!(&r1.cpu_mut().base.x[..3], &initial[..3]);
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(0x1000), Width::Double)
                        .unwrap(),
                    before
                );
                assert_eq!(r1.run(GuestAddr(0x6000), &mut 10).unwrap(), 0);
                assert_eq!(r1.cpu_mut().base.x[4], 0);
            }
            if kind < 2 {
                assert_eq!(r2.run(GuestAddr(0x6000), &mut 10).unwrap(), 0);
                assert_eq!(r2.cpu_mut().base.x[4], if kind == 1 { 1 } else { 0 });
            }
            let value = m
                .lock()
                .unwrap()
                .read(GuestAddr(0x1000), Width::Double)
                .unwrap();
            assert_eq!(value, if kind == 1 { before } else { 99 });
        }
    }
}
#[test]
fn nested_service_thread_exit_propagates_without_caller_continuation() {
    let path = folder("exit");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let m = memory(1);
        let before = m
            .lock()
            .unwrap()
            .read(GuestAddr(0x1000), Width::Double)
            .unwrap();
        let mut bridge = Bridge::new(m.clone()).unwrap();
        let mut c = cpu();
        c.base.x[6] = 0xa000;
        let svc = services(0);
        let mut a = bridge.attach(&mut c).unwrap();
        let mut r = Routed::attach_services(&mut a, &router, &svc).unwrap();
        let mut budget = 20;
        assert_eq!(r.run(GuestAddr(0x8000), &mut budget).unwrap(), EXIT_THREAD);
        assert_eq!(budget, 17);
        assert_eq!(r.cpu_mut().base.pc, 0xa000);
        assert_eq!(r.cpu_mut().base.module_base, 0);
        assert_eq!(
            m.lock()
                .unwrap()
                .read(GuestAddr(0x1000), Width::Double)
                .unwrap(),
            before
        );
    }
}
#[test]
fn service_context_identity_and_registration_profiles_refuse_forged_frames() {
    let path = folder("identity");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let new =
            unsafe { Target::load_services(&lib, b"packet\0", GuestAddr(0x4000), 0x4000) }.unwrap();
        let old = unsafe { Target::load(&lib, b"legacy\0", GuestAddr(0x5000), 0x5000) }.unwrap();
        assert!(Router::new(vec![new, old], 8).is_err());
        let router = Router::new(
            vec![
                unsafe { Target::load_services(&lib, b"foreign\0", GuestAddr(0x4000), 0x4000) }
                    .unwrap(),
            ],
            8,
        )
        .unwrap();
        let m = memory(1);
        let mut bridge = Bridge::new(m).unwrap();
        let mut c = cpu();
        let initial = c.base.x;
        let svc = services(0);
        let mut a = bridge.attach(&mut c).unwrap();
        let mut r = Routed::attach_services(&mut a, &router, &svc).unwrap();
        assert_eq!(r.run(GuestAddr(0x4000), &mut 20).unwrap(), 3);
        assert_eq!(r.cpu_mut().base.x, initial);
        let table = unsafe { &*r.cpu_mut().exclusive.cast::<OpsV3>() };
        let callback = table.svc.unwrap();
        let context = table.svc_context;
        let mut copied = OpsV3 {
            base: table.base,
            svc: table.svc,
            svc_context: table.svc_context,
        };
        let original = r.cpu_mut().exclusive;
        r.cpu_mut().exclusive = std::ptr::addr_of!(copied.base.base);
        assert_eq!(unsafe { callback(context, r.cpu_mut(), 0x77) }, 3);
        let accepts = unsafe {
            lib.get::<unsafe extern "C" fn(*const CpuV10) -> i32>(b"accepts\0")
                .unwrap()
        };
        assert_eq!(unsafe { accepts(r.cpu_mut()) }, 1);
        copied.base.base.version = 2;
        copied.base.base.size = std::mem::size_of::<OpsV2>() as u32;
        assert_eq!(
            (copied.base.base.version, copied.base.base.size),
            (2, std::mem::size_of::<OpsV2>() as u32)
        );
        assert_eq!(unsafe { accepts(r.cpu_mut()) }, 0);
        r.cpu_mut().exclusive = original;
        assert_eq!(
            unsafe { callback(std::ptr::null_mut(), r.cpu_mut(), 0x77) },
            7
        );
        assert_eq!(unsafe { callback(context, r.cpu_mut(), 65536) }, 3);
        assert_eq!(r.cpu_mut().base.x, initial);
    }
    let h: Arc<dyn Service> = Arc::new(Packet(0));
    assert!(Services::new(vec![(1, h.clone()), (1, h.clone())]).is_err());
    assert!(Services::new((0..257).map(|i| (i, h.clone())).collect()).is_err());
}
fn immediate(code: &mut Vec<u32>, reg: u32, value: u64) {
    for part in 0..4 {
        code.push(
            (if part == 0 { 0xd2800000 } else { 0xf2800000 })
                | (part << 21)
                | ((((value >> (part * 16)) & 65535) as u32) << 5)
                | reg,
        )
    }
}
fn qemu_image(initial: u64, pattern: u64) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 0, u64::MAX);
    immediate(&mut code, 1, pattern);
    immediate(&mut code, 4, 0);
    immediate(&mut code, 8, 57);
    immediate(&mut code, 30, BASE + TEXT as u64 + 24 * 4);
    code.extend([0xc85ffc83, svc(0x99), 0xc805fc83, 0xd65f03c0]);
    code.extend([0xf9000480, 0xf9000881, 0xf9000c83, 0xf9001085, 0xaa0403e1]);
    immediate(&mut code, 0, 1);
    immediate(&mut code, 2, 40);
    immediate(&mut code, 8, 64);
    code.push(0xd4000001);
    immediate(&mut code, 0, 0);
    immediate(&mut code, 8, 93);
    code.push(0xd4000001);
    let data = (TEXT + code.len() * 4 + 15) & !15;
    let mut patch = Vec::new();
    immediate(&mut patch, 4, BASE + data as u64);
    code[8..12].copy_from_slice(&patch);
    let mut image = vec![0; data + 40];
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
    for (i, w) in code.into_iter().enumerate() {
        p32(&mut image, TEXT + i * 4, w)
    }
    p64(&mut image, data, initial);
    image
}
#[test]
fn svc_register_transport_and_exception_return_monitor_clear_match_linux_qemu() {
    let path = folder("qemu");
    let available = Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let mut compared = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let svc = services(0);
        for initial in [0u64, 1, u64::MAX] {
            for pattern in [0u64, 1, 0xfedcba9876543210, u64::MAX] {
                let m = memory(1);
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x1000), &initial.to_le_bytes())
                    .unwrap();
                let mut bridge = Bridge::new(m.clone()).unwrap();
                let mut c = cpu();
                c.base.x[0] = u64::MAX;
                c.base.x[1] = pattern;
                c.base.x[4] = 0x1000;
                c.base.x[8] = 57;
                let mut budget = 20;
                {
                    let mut a = bridge.attach(&mut c).unwrap();
                    let mut r = Routed::attach_services(&mut a, &router, &svc).unwrap();
                    assert_eq!(r.run(GuestAddr(0xc000), &mut budget).unwrap(), 0);
                }
                let expected = [initial, (-9i64) as u64, pattern, initial, 1];
                assert_eq!(
                    [
                        m.lock()
                            .unwrap()
                            .read(GuestAddr(0x1000), Width::Double)
                            .unwrap() as u64,
                        c.base.x[0],
                        c.base.x[1],
                        c.base.x[3],
                        c.base.x[5]
                    ],
                    expected
                );
                assert_eq!(budget, 16);
                if available {
                    let file = path.join("original-synthetic.elf");
                    std::fs::write(&file, qemu_image(initial, pattern)).unwrap();
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700))
                        .unwrap();
                    let output = Command::new("qemu-aarch64")
                        .args(["-cpu", "max"])
                        .arg(file)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let actual: Vec<_> = output
                        .stdout
                        .chunks_exact(8)
                        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                        .collect();
                    assert_eq!(actual, expected);
                    compared += 1;
                }
            }
        }
    }
    std::fs::write(path.join("qemu-receipt.json"),serde_json::json!({"available":available,"comparisons":compared,"cases_per_optimization":12,"reference":"Linux close(-1) returning EBADF, not Horizon service semantics","exception_return_local_monitor_clear":true,"original_game_execution":false}).to_string()).unwrap();
}
#[test]
fn service_workers_use_explicit_v3_attachments_and_stop_on_typed_thread_exit() {
    use mkw_runtime::thread_runner::{self, Limits, Range, Storage, Worker};
    let path = folder("workers");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let mut m = Memory::default();
        let mut workers = Vec::new();
        for i in 0..4 {
            let base = 0x10000 + i * 0x4000;
            let range = |offset| Range {
                start: GuestAddr(base + offset),
                bytes: 0x100,
            };
            for r in [range(0), range(0x1000), range(0x2000)] {
                m.map(
                    r.start,
                    vec![0; r.bytes],
                    Permissions::RW,
                    "author-created storage",
                )
                .unwrap();
            }
            workers.push(Worker {
                entry: GuestAddr(if i % 2 == 0 { 0x4000 } else { 0xa000 }),
                return_pc: GuestAddr(0x9000),
                arguments: [base + 0x1000, 0, 0, 0, 0, 0, 0, 0],
                budget: 10,
                storage: Storage {
                    stack: range(0),
                    tls: range(0x1000),
                    tlr: range(0x2000),
                    tpidr_el0: GuestAddr(base + 0x1000),
                    tpidrro_el0: GuestAddr(base + 0x2000),
                },
            });
        }
        let memory = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                m,
                Config {
                    granule: 16,
                    max_owners: 4,
                },
            )
            .unwrap(),
        ));
        let limits = Limits {
            workers: 4,
            host_stack_bytes: 2 * 1024 * 1024,
        };
        let svc = services(0);
        // SAFETY: every native fixture is author-created, reentrant and uses only its
        // own frame plus synchronized checked memory/service state.
        assert!(
            unsafe { thread_runner::run(memory.clone(), &router, workers.clone(), limits) }
                .is_err()
        );
        let result =
            unsafe { thread_runner::run_services(memory.clone(), &router, workers, limits, &svc) }
                .unwrap();
        for (i, r) in result.iter().enumerate() {
            assert_eq!(r.status, if i % 2 == 0 { 0 } else { EXIT_THREAD });
            assert_eq!(r.remaining_budget, if i % 2 == 0 { 8 } else { 9 });
        }
        let mut owners = Vec::new();
        for _ in 0..4 {
            owners.push(Bridge::new(memory.clone()).unwrap());
        }
    }
}
