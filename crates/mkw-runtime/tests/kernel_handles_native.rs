//! Author-created transport fixtures only; no Horizon IPC or kernel ABI binding.
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    host_call::HostFault,
    kernel_handles::{Error, Handle, Table},
    owned_events::{Factory, ReadEnd, WriteEnd},
    services::{Call, Outputs, Reply, Service, Services},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
struct Adapter {
    table: Arc<Table>,
    op: u16,
}
impl Service for Adapter {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let h = Handle::from_raw(call.arguments()[0] as u32);
        let result: Result<Option<u32>, Error> = match self.op {
            0 => self.table.get::<ReadEnd>(h).and_then(|r| {
                r.ready()
                    .map(|ready| Some(u32::from(ready)))
                    .map_err(|_| Error::Poisoned)
            }),
            1 => self.table.close(h).map(|_| None),
            2 => self
                .table
                .get::<WriteEnd>(h)
                .and_then(|w| w.signal().map(|_| None).map_err(|_| Error::Poisoned)),
            _ => return Err(HostFault::unsupported()),
        };
        let mut o = Outputs::default();
        let code = match result {
            Ok(value) => {
                if let Some(v) = value {
                    o = o.write_w(1, v)?;
                }
                0
            }
            Err(Error::InvalidHandle | Error::WrongType) => 1,
            Err(_) => return Err(HostFault::unsupported()),
        };
        Ok(Reply::Return(o.write_w(0, code)?))
    }
}
fn service(table: &Arc<Table>) -> Services {
    Services::new(
        (0..3)
            .map(|op| {
                (
                    0x700 + op,
                    Arc::new(Adapter {
                        table: table.clone(),
                        op,
                    }) as Arc<dyn Service>,
                )
            })
            .collect(),
    )
    .unwrap()
}
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/kernel-handles-native")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    let mut source = String::new();
    for n in 0..6 {
        let words = [0xd4000001u32 | ((0x700 + n) << 5), 0xd65f03c0];
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let ir = mkw_arm64::lift_services_v3(&bytes, 0, 0).unwrap();
        source += &mkw_codegen::emit_services_v3(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", &format!("event_{n}"));
    }
    let src = path.join("synthetic.c");
    let lib = path.join(format!("synthetic{opt}.so"));
    std::fs::write(&src, source).unwrap();
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
    Router::new(
        (0..6)
            .map(|n| {
                let name = format!("event_{n}\0");
                let at = 0x4000 + n * 0x1000;
                unsafe { Target::load_services(lib, name.as_bytes(), GuestAddr(at), at).unwrap() }
            })
            .collect(),
        8,
    )
    .unwrap()
}
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0; 4096],
        Permissions::RW,
        "author-created buffer",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 4,
            },
        )
        .unwrap(),
    ))
}
fn call(
    bridge: &mut Bridge,
    router: &Router<'_>,
    svc: &Services,
    op: u64,
    handle: u64,
) -> [u64; 3] {
    let mut cpu = CpuV10::default();
    cpu.base.x[0] = handle;
    cpu.base.x[1] = 0x1111111111111111;
    cpu.base.x[2] = 0x2222222222222222;
    cpu.base.x[30] = 0x9000;
    let mut budget = 10;
    {
        let mut a = bridge.attach(&mut cpu).unwrap();
        let mut r = Routed::attach_services(&mut a, router, svc).unwrap();
        assert_eq!(
            r.run(GuestAddr(0x4000 + op * 0x1000), &mut budget).unwrap(),
            0
        );
    }
    assert_eq!(budget, 8);
    [cpu.base.x[0], cpu.base.x[1], cpu.base.x[2]]
}
#[test]
fn transferred_real_event_handles_reach_generated_native_code_without_guest_create_or_fake_ipc() {
    let path = folder("transferred");
    for opt in ["-O0", "-O2"] {
        let lib = build(&path, opt);
        let router = router(&lib);
        let f = Factory::new(1).unwrap();
        let owner = Table::new(2).unwrap();
        let guest = Arc::new(Table::new(2).unwrap());
        let (w, r) = f.create(&owner).unwrap();
        let pin = owner.export(r).unwrap();
        let received = guest.import(pin).unwrap();
        owner.close(r).unwrap();
        let svc = service(&guest);
        let mut bridge = Bridge::new(memory()).unwrap();
        assert_eq!(
            &call(&mut bridge, &router, &svc, 0, received.raw() as u64)[..2],
            &[0, 0]
        );
        assert_eq!(
            call(&mut bridge, &router, &svc, 2, received.raw() as u64)[0],
            1
        );
        assert_eq!(call(&mut bridge, &router, &svc, 0, r.raw() as u64)[0], 1);
        owner.get::<WriteEnd>(w).unwrap().signal().unwrap();
        for _ in 0..2 {
            assert_eq!(
                &call(&mut bridge, &router, &svc, 0, received.raw() as u64)[..2],
                &[0, 1]
            );
        }
        let second = guest.import(guest.export(received).unwrap()).unwrap();
        assert_ne!(received, second);
        call(&mut bridge, &router, &svc, 1, received.raw() as u64);
        assert_eq!(
            call(&mut bridge, &router, &svc, 0, received.raw() as u64)[0],
            1
        );
        assert_eq!(
            &call(&mut bridge, &router, &svc, 0, second.raw() as u64)[..2],
            &[0, 1]
        );
        owner.close(w).unwrap();
        assert!(f.create(&owner).is_err());
        let unknown = guest.insert(Arc::new(7u64)).unwrap();
        assert_eq!(
            call(&mut bridge, &router, &svc, 0, unknown.raw() as u64)[0],
            1
        );
        call(&mut bridge, &router, &svc, 1, unknown.raw() as u64);
        call(&mut bridge, &router, &svc, 1, second.raw() as u64);
        let (a, b) = f.create(&owner).unwrap();
        owner.close(a).unwrap();
        owner.close(b).unwrap();
    }
}
