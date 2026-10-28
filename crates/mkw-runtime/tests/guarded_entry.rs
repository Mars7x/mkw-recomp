use mkw_arm64::terminal::{contract::Contract, Image};
use mkw_codegen::guarded::Artifact;
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    guarded::{Router, Target},
    guarded_entry::run,
    pauth::{Backend, DiagnosticProfile, Keys},
    process_entropy::{Abi, ProcessEntropy},
    services::Services,
    thread_runner::{Range, Storage, Worker},
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
fn worker() -> Worker {
    Worker {
        entry: GuestAddr(0x400000),
        return_pc: GuestAddr(0x500000),
        arguments: [0; 8],
        budget: 40,
        storage: Storage {
            stack: Range {
                start: GuestAddr(0x8000),
                bytes: 256,
            },
            tls: Range {
                start: GuestAddr(0x9000),
                bytes: 256,
            },
            tlr: Range {
                start: GuestAddr(0xa000),
                bytes: 256,
            },
            tpidr_el0: GuestAddr(0x9000),
            tpidrro_el0: GuestAddr(0xa000),
        },
    }
}
fn memory(read_only: bool) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    for at in [0x8000, 0x9000, 0xa000] {
        m.map(
            GuestAddr(at),
            vec![0xab; 256],
            if read_only && at == 0xa000 {
                Permissions::R
            } else {
                Permissions::RW
            },
            "synthetic storage",
        )
        .unwrap();
    }
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
fn artifact(words: &[u32]) -> Artifact {
    let image = Image::new(0, words.iter().flat_map(|x| x.to_le_bytes()).collect()).unwrap();
    Artifact::new(
        Contract::plain_services(&image, 0, words.len() * 4).unwrap(),
        "mkw_gv4_run",
    )
    .unwrap()
}
fn build(name: &str, a: &Artifact, opt: &str) -> libloading::Library {
    let dir = root().join("private/testing/guarded-entry");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("{name}{opt}.c"));
    let file = src.with_extension("so");
    std::fs::write(&src, a.code()).unwrap();
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
        .arg(src)
        .arg(root().join("native/pac.c"))
        .arg(root().join("native/pac_profile.c"))
        .arg(root().join("third_party/gem5-qarma/qarma.c"))
        .arg("-o")
        .arg(&file)
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
    unsafe { libloading::Library::new(file).unwrap() }
}
fn backend(l: &libloading::Library) -> Backend<'_> {
    unsafe {
        Backend::load(
            l,
            DiagnosticProfile::Legacy39NoTbiQarma5,
            Keys::new(1, 2, 3, 4),
        )
        .unwrap()
    }
}
fn router<'a>(l: &'a libloading::Library, a: &Artifact) -> Router<'a> {
    Router::new(
        vec![unsafe {
            Target::load(
                l,
                a.name().as_bytes(),
                a.contract().clone(),
                a.producer(),
                0x400000,
            )
            .unwrap()
        }],
        4,
    )
    .unwrap()
}
#[test]
fn explicit_thread_storage_pac_and_entropy_compose_with_exact_budgets() {
    for (kind, handle) in [(11u32, 0u32), (11, 1), (12, 0)] {
        // Authored synthetic fixture. Thread offsets/contents are not SDK layouts.
        let mut words = vec![
            0xd503237f,
            0xd53bd049,
            0xf940012a,
            0xd53bd06b,
            0x7900417f,
            0xa9bf7bea,
            0xd2800001 | (kind << 5),
            0xd2800002 | (handle << 5),
            0xd2800043,
            0xd4000521,
            0,
            0xaa0103f4,
            0xd2800001 | (kind << 5),
            0xd2800002 | (handle << 5),
            0xd2800063,
            0xd4000521,
            0,
            0xaa0103f5,
            0xa8c17bea,
            0xd65f0fff,
            0x14000000,
        ];
        for at in [10, 16] {
            words[at] = 0x35000000 | (((20 - at) as u32) << 5);
        }
        let a = artifact(&words);
        for opt in ["-O0", "-O2"] {
            let lib = build(&format!("compose-{kind}-{handle}"), &a, opt);
            let pac = backend(&lib);
            let r = router(&lib, &a);
            let services = ProcessEntropy::from_synthetic_words(
                Abi::Horizon64EntropyReference,
                [10, 20, 30, 40],
            )
            .registry();
            let m = memory(false);
            for limit in 0..=25 {
                let mut w = worker();
                w.budget = limit;
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0xa020), &[0xab, 0xab])
                    .unwrap();
                let result = run(m.clone(), &r, &services, &pac, w).unwrap();
                let expected = if kind == 12 && limit >= 10 {
                    3
                } else if handle != 0 || limit < 20 {
                    2
                } else {
                    0
                };
                assert_eq!(
                    result.status, expected,
                    "kind{kind} handle{handle} limit{limit}"
                );
                let consumed = if expected == 3 {
                    10
                } else if expected == 0 {
                    20
                } else {
                    limit
                };
                assert_eq!(result.remaining_budget, limit - consumed);
                assert_eq!(
                    (result.registers.tpidr_el0, result.registers.tpidrro_el0),
                    (0x9000, 0xa000)
                );
                if result.status == 0 {
                    assert_eq!((result.registers.x[20], result.registers.x[21]), (30, 40));
                    assert_eq!(result.registers.x[10], 0xabababababababab);
                    assert_eq!(result.registers.sp, 0x8100);
                    assert_eq!(result.registers.pc, 0x500000);
                    assert_eq!(result.registers.module_base, 0x400000);
                }
                let actual = m
                    .lock()
                    .unwrap()
                    .read(GuestAddr(0xa020), Width::Half)
                    .unwrap();
                assert_eq!(actual, if limit >= 5 { 0 } else { 0xabab });
                // Owner quota one is reusable after every return/fault/budget stop.
                let owner = m.lock().unwrap().new_owner().unwrap();
                m.lock().unwrap().release(&owner).unwrap();
            }
        }
    }
}
#[test]
fn malformed_entry_storage_and_owner_quota_refuse_without_guest_effects() {
    let a = artifact(&[0xf9000001, 0xd65f03c0]);
    for opt in ["-O0", "-O2"] {
        let lib = build("refuse", &a, opt);
        let pac = backend(&lib);
        let r = router(&lib, &a);
        let services = Services::new(vec![]).unwrap();
        let mut good = worker();
        good.arguments[0] = 0x9000;
        good.arguments[1] = 99;
        let mut cases = Vec::new();
        for which in 0..14 {
            let mut w = good;
            match which {
                0 => w.entry = GuestAddr(0x400004),
                1 => w.return_pc = GuestAddr(3),
                2 => w.storage.stack.start = GuestAddr(0x8001),
                3 => w.storage.stack.bytes = 255,
                4 => w.storage.stack.bytes = 0,
                5 => w.storage.stack.bytes = 16 * 1024 * 1024 + 16,
                6 => w.storage.stack.start = GuestAddr(u64::MAX - 15),
                7 => w.storage.tpidr_el0 = GuestAddr(0x9100),
                8 => w.storage.tpidrro_el0 = GuestAddr(0x9fff),
                9 => {
                    w.storage.tls = w.storage.stack;
                    w.storage.tpidr_el0 = w.storage.stack.start;
                }
                10 => {
                    w.storage.tlr = w.storage.tls;
                    w.storage.tpidrro_el0 = w.storage.tpidr_el0;
                }
                11 => w.storage.tlr.bytes = 257,
                12 => w.storage.tls.bytes = 0,
                13 => w.storage.tlr.start = GuestAddr(u64::MAX - 127),
                _ => unreachable!(),
            };
            cases.push(w);
        }
        for w in cases {
            let m = memory(false);
            assert!(run(m.clone(), &r, &services, &pac, w).is_err());
            assert_eq!(
                m.lock()
                    .unwrap()
                    .read(GuestAddr(0x9000), Width::Double)
                    .unwrap(),
                0xababababababababu128
            );
        }
        let m = memory(true);
        assert!(run(m.clone(), &r, &services, &pac, good).is_err());
        assert_eq!(
            m.lock()
                .unwrap()
                .read(GuestAddr(0x9000), Width::Double)
                .unwrap(),
            0xababababababababu128
        );
        let m = memory(false);
        let owner = m.lock().unwrap().new_owner().unwrap();
        assert!(run(m.clone(), &r, &services, &pac, good).is_err());
        assert_eq!(
            m.lock()
                .unwrap()
                .read(GuestAddr(0x9000), Width::Double)
                .unwrap(),
            0xababababababababu128
        );
        m.lock().unwrap().release(&owner).unwrap();
        assert_eq!(run(m.clone(), &r, &services, &pac, good).unwrap().status, 0);
        assert_eq!(
            m.lock()
                .unwrap()
                .read(GuestAddr(0x9000), Width::Double)
                .unwrap(),
            99u128
        );
    }
}
#[test]
fn wrong_return_and_memory_fault_retain_effects_and_release_owner() {
    for bad_return in [false, true] {
        let words = if bad_return {
            vec![0xf9000001, 0x910013de, 0xd65f03c0]
        } else {
            vec![0xf9000001, 0xf9400043, 0xd65f03c0]
        };
        let a = artifact(&words);
        for opt in ["-O0", "-O2"] {
            let lib = build(&format!("failure-{bad_return}"), &a, opt);
            let pac = backend(&lib);
            let r = router(&lib, &a);
            let services = Services::new(vec![]).unwrap();
            let m = memory(false);
            let mut w = worker();
            w.arguments = [0x9000, 77, 0xdead, 0, 0, 0, 0, 0];
            let result = run(m.clone(), &r, &services, &pac, w);
            if bad_return {
                assert!(result.unwrap_err().contains("unexpected guest PC"));
            } else {
                let result = result.unwrap();
                assert_ne!(result.status, 0);
                assert_eq!(result.registers.fault_address, 0xdead);
            }
            assert_eq!(
                m.lock()
                    .unwrap()
                    .read(GuestAddr(0x9000), Width::Double)
                    .unwrap(),
                77u128
            );
            let owner = m.lock().unwrap().new_owner().unwrap();
            m.lock().unwrap().release(&owner).unwrap();
        }
    }
}

#[test]
fn pac_stack_and_tpidr_transport_match_qemu() {
    const WORDS: [u32; 8] = [
        0xd503237f, 0xa9bf7bfd, 0xd53bd049, 0xf940012a, 0x8b01014a, 0xf900052a, 0xa8c17bfd,
        0xd65f0fff,
    ];
    let a = artifact(&WORDS);
    let references: [u64; 3] = [0, u64::MAX, 0xabababababababab];
    let expected: Vec<_> = references
        .iter()
        .map(|&initial| qemu_tls_pac(&WORDS, initial))
        .collect();
    for opt in ["-O0", "-O2"] {
        let lib = build("qemu", &a, opt);
        let pac = backend(&lib);
        let r = router(&lib, &a);
        let services = Services::new(vec![]).unwrap();
        let m = memory(false);
        for (&initial, &reference) in references.iter().zip(&expected) {
            assert_eq!(reference, initial.wrapping_add(19));
            for budget in 0..=10 {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x9000), &initial.to_le_bytes())
                    .unwrap();
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x9008), &0u64.to_le_bytes())
                    .unwrap();
                let mut w = worker();
                w.arguments[1] = 19;
                w.budget = budget;
                let out = run(m.clone(), &r, &services, &pac, w).unwrap();
                assert_eq!(out.status, if budget < 8 { 2 } else { 0 });
                assert_eq!(out.remaining_budget, budget.saturating_sub(8));
                assert_eq!(
                    m.lock()
                        .unwrap()
                        .read(GuestAddr(0x9008), Width::Double)
                        .unwrap(),
                    if budget >= 6 { reference as u128 } else { 0 }
                );
                if out.status == 0 {
                    assert_eq!(out.registers.x[10], reference);
                    assert_eq!(out.registers.sp, 0x8100);
                    assert_eq!(out.registers.pc, 0x500000);
                }
            }
        }
    }
}
fn qemu_tls_pac(body: &[u32], initial: u64) -> u64 {
    fn imm(w: &mut Vec<u32>, r: u32, v: u64) {
        w.push(0xd2800000 | ((v as u32 & 65535) << 5) | r);
        for n in 1..4 {
            w.push(0xf2800000 | (n << 21) | ((((v >> (16 * n)) as u32) & 65535) << 5) | r);
        }
    }
    let mut words = Vec::new();
    imm(&mut words, 0, 0x404000);
    words.push(0xd51bd040);
    imm(&mut words, 1, 19);
    let pc = 0x401000 + 4 * words.len() as u64;
    words.push(0x94000000 | ((0x402000 - pc) / 4) as u32);
    for (r, v) in [(0, 1), (1, 0x404008), (2, 8), (8, 64)] {
        imm(&mut words, r, v);
    }
    words.push(0xd4000001);
    imm(&mut words, 0, 0);
    imm(&mut words, 8, 93);
    words.push(0xd4000001);
    let mut elf = vec![0u8; 0x4010];
    elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    for (at, v) in [(16, 2u16), (18, 183), (52, 64), (54, 56), (56, 1)] {
        elf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    for (at, v) in [(20, 1u32), (64, 1), (68, 7)] {
        elf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    let len = elf.len() as u64;
    for (at, v) in [
        (24, 0x401000u64),
        (32, 64),
        (80, 0x400000),
        (88, 0x400000),
        (96, len),
        (104, len),
        (112, 4096),
        (0x4000, initial),
    ] {
        elf[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    for (offset, code) in [(0x1000, &words[..]), (0x2000, body)] {
        for (i, w) in code.iter().enumerate() {
            elf[offset + 4 * i..offset + 4 * i + 4].copy_from_slice(&w.to_le_bytes());
        }
    }
    let dir = root().join("private/testing/guarded-entry");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("tls-{initial}.elf"));
    std::fs::write(&path, elf).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(path)
        .output()
        .expect("QEMU required for guarded entry transport");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout.len(), 8);
    u64::from_le_bytes(out.stdout.try_into().unwrap())
}
