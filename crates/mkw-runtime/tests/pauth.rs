use mkw_arm64::terminal::{contract::Contract, Image};
use mkw_codegen::guarded::Artifact;
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_native::{Bridge, CpuV10},
    guarded::{Routed, Router, Target},
    pauth::{Backend, DiagnosticProfile, Keys},
    services::Services,
    GuestAddr, Memory,
};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
};
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn keys() -> Keys {
    Keys::new(1, 2, 3, 4)
}
fn build(name: &str, source: &str, opt: &str, metadata: bool) -> libloading::Library {
    let dir = root().join("private/testing/owned-pac");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join(format!("{name}{opt}.c"));
    let out = src.with_extension("so");
    std::fs::write(&src, source).unwrap();
    let mut cc = Command::new("cc");
    cc.args([
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
    .arg(root().join("third_party/gem5-qarma/qarma.c"));
    if metadata {
        cc.arg(root().join("native/pac_profile.c"));
    }
    let result = cc.arg("-o").arg(&out).output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    unsafe { libloading::Library::new(out).unwrap() }
}
fn invoke(cpu: &mut CpuV10, op: u32, key: u32, pointer: u64, modifier: u64) -> (i32, u64) {
    let mut out = 0xdead;
    let status = unsafe {
        cpu.base.pauth.unwrap()(cpu.base.pac_context, op, key, pointer, modifier, &mut out)
    };
    (status, out)
}
#[test]
fn immutable_keys_nested_attachment_unwind_and_authentication() {
    for opt in ["-O0", "-O2"] {
        let lib = build(
            "ownership",
            r#"
#include "pac.h"
uint64_t expected(uint32_t key,uint64_t pointer,uint64_t modifier) {
    MkwPac39 state={{1,3},{2,4}}; uint64_t result=0;
    if(mkw_pac39(&state,0,key,pointer,modifier,&result)) return 0;
    return result;
}
"#,
            opt,
            true,
        );
        let expected = unsafe {
            lib.get::<unsafe extern "C" fn(u32, u64, u64) -> u64>(b"expected\0")
                .unwrap()
        };
        let a =
            unsafe { Backend::load(&lib, DiagnosticProfile::Legacy39NoTbiQarma5, keys()).unwrap() };
        let b = unsafe {
            Backend::load(
                &lib,
                DiagnosticProfile::Legacy39NoTbiQarma5,
                Keys::new(11, 22, 33, 44),
            )
            .unwrap()
        };
        let mut cpu = CpuV10::default();
        // An existing opaque context is restored exactly and never dereferenced.
        cpu.base.pac_context = std::ptr::dangling_mut();
        let previous = cpu.base.pac_context;
        {
            let mut attached = a.attach(&mut cpu);
            let identity = attached.cpu_mut().base.pac_context;
            for key in 0..2 {
                for pointer in [0, 0x12345678, 0x7fffffffff, 0xffffff8000000000, u64::MAX] {
                    for modifier in [0, 16, u64::MAX] {
                        let (status, signed) =
                            invoke(attached.cpu_mut(), 0, key, pointer, modifier);
                        assert_eq!(status, 0);
                        assert_eq!(signed, unsafe { expected(key, pointer, modifier) });
                        assert_eq!(
                            invoke(attached.cpu_mut(), 1, key, signed, modifier),
                            (0, pointer)
                        );
                        assert_eq!(
                            invoke(attached.cpu_mut(), 3, key, signed, modifier),
                            (0, pointer)
                        );
                        assert_eq!(
                            invoke(attached.cpu_mut(), 2, key, signed, modifier),
                            (0, pointer)
                        );
                        assert_eq!(
                            invoke(attached.cpu_mut(), 3, key, signed ^ (1 << 54), modifier).0,
                            4
                        );
                        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let mut inner = b.attach(attached.cpu_mut());
                            assert_ne!(inner.cpu_mut().base.pac_context, identity);
                            assert_eq!(invoke(inner.cpu_mut(), 3, key, signed, modifier).0, 4);
                            panic!("synthetic unwind");
                        }));
                        assert!(failed.is_err());
                        assert_eq!(attached.cpu_mut().base.pac_context, identity);
                        assert_eq!(
                            invoke(attached.cpu_mut(), 3, key, signed, modifier),
                            (0, pointer)
                        );
                    }
                }
            }
            assert_eq!(invoke(attached.cpu_mut(), 4, 0, 0, 0), (3, 0xdead));
            assert_eq!(invoke(attached.cpu_mut(), 0, 2, 0, 0), (3, 0xdead));
        }
        assert!(cpu.base.pauth.is_none());
        assert_eq!(cpu.base.pac_context, previous);
        // Shared process keys remain immutable across concurrent per-thread frames.
        std::thread::scope(|scope| {
            for id in 0..4u64 {
                let backend = &a;
                scope.spawn(move || {
                    let mut frame = CpuV10::default();
                    let mut attached = backend.attach(&mut frame);
                    for n in 0..64u64 {
                        let pointer = 0x1000 + 4 * n;
                        let signed = invoke(attached.cpu_mut(), 0, 1, pointer, id).1;
                        assert_eq!(invoke(attached.cpu_mut(), 3, 1, signed, id), (0, pointer));
                    }
                });
            }
        });
    }
}
#[test]
fn missing_or_mismatched_profile_queries_refuse() {
    for changed in 0..5 {
        let names = ["callback_abi", "profile", "context_size", "low_offset"];
        let values = [9, 1, 32, 16];
        let mut src = String::from("#include <stdint.h>\n");
        for (i, (name, value)) in names.iter().zip(values).enumerate() {
            if changed == 4 && i == 0 {
                continue;
            }
            src += &format!(
                "uint32_t mkw_pac39_{name}(void) {{ return {}; }}\n",
                value + u32::from(i == changed)
            );
        }
        let lib = build(&format!("bad-{changed}"), &src, "-O2", false);
        assert!(
            unsafe { Backend::load(&lib, DiagnosticProfile::Legacy39NoTbiQarma5, keys()) }.is_err()
        );
    }
}
#[test]
fn guarded_pacibsp_retab_preserves_budget_and_auth_faults() {
    for corrupt in [false, true] {
        let words: Vec<u32> = if corrupt {
            vec![0xd503237f, 0x910043ff, 0xd65f0fff]
        } else {
            vec![0xd503237f, 0xd28000e0, 0xd65f0fff]
        };
        let image = Image::new(0, words.iter().flat_map(|w| w.to_le_bytes()).collect()).unwrap();
        let artifact = Artifact::new(
            Contract::plain_services(&image, 0, 12).unwrap(),
            "mkw_gv4_pac",
        )
        .unwrap();
        for opt in ["-O0", "-O2"] {
            let lib = build(&format!("guard-{corrupt}"), artifact.code(), opt, true);
            let backend = unsafe {
                Backend::load(&lib, DiagnosticProfile::Legacy39NoTbiQarma5, keys()).unwrap()
            };
            let target = unsafe {
                Target::load(
                    &lib,
                    artifact.name().as_bytes(),
                    artifact.contract().clone(),
                    artifact.producer(),
                    0x400000,
                )
                .unwrap()
            };
            let router = Router::new(vec![target], 4).unwrap();
            let services = Services::new(vec![]).unwrap();
            let owned = Arc::new(Mutex::new(
                ExclusiveMemory::new(
                    Memory::default(),
                    Config {
                        granule: 16,
                        max_owners: 1,
                    },
                )
                .unwrap(),
            ));
            let mut bridge = Bridge::new(owned).unwrap();
            for budget in 0..6 {
                let mut cpu = CpuV10::default();
                cpu.base.sp = 0x8000;
                cpu.base.x[30] = 0x500000;
                let mut left = budget;
                let status = {
                    let mut pac = backend.attach(&mut cpu);
                    let mut memory = bridge.attach(pac.cpu_mut()).unwrap();
                    let mut route = Routed::attach(&mut memory, &router, &services).unwrap();
                    route.run(GuestAddr(0x400000), &mut left).unwrap()
                };
                assert_eq!(
                    status,
                    if budget < 3 {
                        2
                    } else if corrupt {
                        4
                    } else {
                        0
                    }
                );
                assert_eq!(left, budget.saturating_sub(3));
                assert!(cpu.base.pauth.is_none());
                assert!(cpu.base.pac_context.is_null());
                if budget >= 3 && !corrupt {
                    assert_eq!(cpu.base.pc, 0x500000);
                    assert_ne!(cpu.base.x[30], 0x500000);
                    assert_eq!(cpu.base.x[0], 7);
                }
            }
        }
    }
}

#[test]
fn qemu_retab_branches_without_replacing_signed_lr() {
    fn imm(w: &mut Vec<u32>, r: u32, v: u64) {
        w.push(0xd2800000 | ((v as u32 & 65535) << 5) | r);
        for n in 1..4 {
            w.push(0xf2800000 | (n << 21) | ((((v >> (16 * n)) as u32) & 65535) << 5) | r);
        }
    }
    let mut words = Vec::new();
    imm(&mut words, 20, 0x404000);
    let pc = 0x401000 + 4 * words.len() as u64;
    words.push(0x94000000 | ((0x402000 - pc) / 4) as u32);
    words.extend([0xf9000280, 0xf9000693, 0xf9000a9e, 0xdac143fe, 0xf9000e9e]);
    for (r, v) in [(0, 1), (1, 0x404000), (2, 32), (8, 64)] {
        imm(&mut words, r, v);
    }
    words.push(0xd4000001);
    imm(&mut words, 0, 0);
    imm(&mut words, 8, 93);
    words.push(0xd4000001);
    let mut elf = vec![0u8; 0x4020];
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
    ] {
        elf[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    for (i, w) in words.iter().enumerate() {
        elf[0x1000 + 4 * i..0x1004 + 4 * i].copy_from_slice(&w.to_le_bytes());
    }
    // PACIBSP; MOV X19,X30; MOV X0,#7; RETAB.
    for (i, w) in [0xd503237fu32, 0xaa1e03f3, 0xd28000e0, 0xd65f0fff]
        .iter()
        .enumerate()
    {
        elf[0x2000 + 4 * i..0x2004 + 4 * i].copy_from_slice(&w.to_le_bytes());
    }
    let dir = root().join("private/testing/owned-pac");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("retab.elf");
    std::fs::write(&path, elf).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let run = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(path)
        .output()
        .expect("QEMU required for PAC transport");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout.len(), 32);
    let out: Vec<_> = run
        .stdout
        .chunks_exact(8)
        .map(|x| u64::from_le_bytes(x.try_into().unwrap()))
        .collect();
    assert_eq!(out[0], 7);
    assert_eq!(out[1], out[2]);
    assert_eq!(out[3], pc + 4);
    // QEMU chooses its own keys/profile; this checks transport, not bit equality
    // with the explicit legacy-39 diagnostic helper or retail compatibility.
}
