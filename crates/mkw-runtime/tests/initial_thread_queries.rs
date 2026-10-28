use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    kernel_handles::Table,
    thread_queries::{Abi, ThreadIdentity, ThreadQueries, GET_PRIORITY},
    GuestAddr, Memory, Permissions,
};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
#[test]
fn guarded_initial_queries_use_registered_identity_and_explicit_permissions() {
    use mkw_arm64::terminal::{contract::Contract, Image};
    use mkw_codegen::guarded::Artifact;
    use mkw_runtime::{
        guarded, guarded_entry, pauth,
        thread_runner::{Range, Storage, Worker},
    };
    let words = [
        0xd503237fu32,
        0xaa1e03f6,
        0xaa0103f3,
        0xd10043e2,
        0xd40000c1,
        0x35000160,
        0xaa1303e1,
        0xd4000181,
        0x35000100,
        0x2a0103f4,
        0xaa1303e1,
        0xd40004a1,
        0x35000080,
        0xaa0103f5,
        0xaa1603fe,
        0xd65f0fff,
        0x14000000,
    ];
    let image = Image::new(0, words.iter().flat_map(|w| w.to_le_bytes()).collect()).unwrap();
    let a = Artifact::new(
        Contract::plain_services(&image, 0, 68).unwrap(),
        "mkw_gv4_thread_queries",
    )
    .unwrap();
    let dir = root().join("private/testing/initial-thread-queries");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("synthetic.c");
    std::fs::write(&src, a.code()).unwrap();
    for opt in ["-O0", "-O2"] {
        let file = dir.join(format!("synthetic{opt}.so"));
        let c = Command::new("cc")
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
            .arg(root().join("native/pac.c"))
            .arg(root().join("native/pac_profile.c"))
            .arg(root().join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(&file)
            .output()
            .unwrap();
        assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
        let lib = unsafe { libloading::Library::new(file).unwrap() };
        let pac = unsafe {
            pauth::Backend::load(
                &lib,
                pauth::DiagnosticProfile::Legacy39NoTbiQarma5,
                pauth::Keys::new(1, 2, 3, 4),
            )
            .unwrap()
        };
        let target = unsafe {
            guarded::Target::load(
                &lib,
                a.name().as_bytes(),
                a.contract().clone(),
                a.producer(),
                0x400000,
            )
            .unwrap()
        };
        let router = guarded::Router::new(vec![target], 4).unwrap();
        let table = Arc::new(Table::new(2).unwrap());
        let h = ThreadIdentity::register(&table, 44).unwrap();
        let id = table.get::<ThreadIdentity>(h).unwrap().id();
        let services =
            ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table.clone()).registry();

        let mut memory = Memory::default();
        for base in [0x8000, 0x9000, 0xa000] {
            memory
                .map(
                    GuestAddr(base),
                    vec![0; 4096],
                    Permissions::RW,
                    "synthetic thread storage",
                )
                .unwrap();
        }
        let owned = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                memory,
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let services = services
            .combined(
                &mkw_runtime::stack_query::StackQuery::new(
                    mkw_runtime::stack_query::Abi::Horizon64OwnedStack,
                    owned.clone(),
                    GuestAddr(0x8000),
                    4096,
                )
                .unwrap()
                .registry(),
            )
            .unwrap();
        let denied = services.restricted_to(&BTreeSet::from([6, GET_PRIORITY]));
        for mode in 0..3 {
            for budget in 0..=20 {
                let w = Worker {
                    entry: GuestAddr(0x400000),
                    return_pc: GuestAddr(0x500000),
                    arguments: [
                        0x8100,
                        if mode == 1 { 0 } else { u64::from(h.raw()) },
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    ],
                    budget,
                    storage: Storage {
                        stack: Range {
                            start: GuestAddr(0x8000),
                            bytes: 4096,
                        },
                        tls: Range {
                            start: GuestAddr(0x9000),
                            bytes: 4096,
                        },
                        tlr: Range {
                            start: GuestAddr(0xa000),
                            bytes: 4096,
                        },
                        tpidr_el0: GuestAddr(0x9000),
                        tpidrro_el0: GuestAddr(0xa000),
                    },
                };
                let out = guarded_entry::run(
                    owned.clone(),
                    &router,
                    if mode == 2 { &denied } else { &services },
                    &pac,
                    w,
                )
                .unwrap();
                let expected = if mode == 2 && budget >= 12 {
                    3
                } else if mode == 1 || budget < 16 {
                    2
                } else {
                    0
                };
                assert_eq!(out.status, expected);
                assert_eq!(
                    out.remaining_budget,
                    if expected == 0 {
                        budget - 16
                    } else if expected == 3 {
                        budget - 12
                    } else {
                        0
                    }
                );
                if out.status == 0 {
                    assert_eq!((out.registers.x[20], out.registers.x[21]), (44, id));
                    assert_eq!(out.registers.pc, 0x500000);
                    assert_eq!(
                        owned
                            .lock()
                            .unwrap()
                            .read(GuestAddr(0x8100), Width::Double)
                            .unwrap(),
                        0x8000
                    );
                    assert_eq!(
                        owned
                            .lock()
                            .unwrap()
                            .read(GuestAddr(0x8108), Width::Double)
                            .unwrap(),
                        4096
                    );
                    assert_eq!(
                        owned
                            .lock()
                            .unwrap()
                            .read(GuestAddr(0x8110), Width::Word)
                            .unwrap(),
                        11
                    );
                }
            }
        }
    }
}
