#![cfg(target_os = "linux")]
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    romfs_hle::{self as fs, Limits, ReadOnlyFs},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/romfs-hle")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(p: &Path, opt: &str) -> libloading::Library {
    let bytes = [0xaa1e03f3u32, 0xd63f0100, 0xaa1303fe, 0xd65f03c0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let ir = mkw_arm64::lift(&bytes, 0, 0).unwrap();
    let code = mkw_codegen::emit_exclusive_calls_v10(&ir, 0).unwrap();
    let src = p.join("caller.c");
    std::fs::write(&src, code).unwrap();
    let lib = p.join(format!("caller{}.so", &opt[1..]));
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
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(src)
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
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x1000),
        vec![0xa5; 0x4000],
        Permissions::RW,
        "guest-rw",
    )
    .unwrap();
    m.map(
        GuestAddr(0x6000),
        vec![0x5a; 0x100],
        Permissions::R,
        "guest-ro",
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
fn path(m: &Arc<Mutex<ExclusiveMemory>>, s: &[u8]) {
    let mut bytes = s.to_vec();
    bytes.push(0);
    m.lock().unwrap().write(GuestAddr(0x1100), &bytes).unwrap();
}
fn peek(m: &Arc<Mutex<ExclusiveMemory>>, at: u64, n: usize) -> Vec<u8> {
    let memory = m.lock().unwrap();
    (0..n)
        .map(|i| memory.read(GuestAddr(at + i as u64), Width::Byte).unwrap() as u8)
        .collect()
}
fn word(m: &Arc<Mutex<ExclusiveMemory>>, at: u64) -> u64 {
    m.lock()
        .unwrap()
        .read(GuestAddr(at), Width::Double)
        .unwrap() as u64
}
fn invoke(
    lib: &libloading::Library,
    fs: &Arc<ReadOnlyFs>,
    bridge: &mut Bridge,
    symbol: &str,
    args: [u64; 8],
    initial_budget: u64,
) -> (i32, CpuV10, u64) {
    let mut targets =
        vec![
            unsafe { Target::load(lib, b"mkw_guest_v10_budget\0", GuestAddr(0x4000), 0x4000) }
                .unwrap(),
        ];
    for (i, name) in fs::SYMBOLS.iter().enumerate() {
        targets.push(
            fs.target(name, GuestAddr(0x8000 + i as u64 * 16), 0x8000)
                .unwrap(),
        );
    }
    let index = fs::SYMBOLS.iter().position(|&n| n == symbol).unwrap();
    let router = Router::new(targets, 8).unwrap();
    let mut cpu = CpuV10::default();
    cpu.base.x[..8].copy_from_slice(&args);
    cpu.base.x[8] = 0x8000 + index as u64 * 16;
    cpu.base.x[30] = 0x9000;
    cpu.base.pc = 0xbeef;
    cpu.base.module_base = 0xdead;
    cpu.base.n = 1;
    cpu.base.c = 1;
    cpu.base.v = 1;
    cpu.base.fpcr = 0x400000;
    cpu.base.fpsr = 0x18;
    cpu.base.vregs = [[0x123456789abcdef0, 0xfedcba9876543210]; 32];
    let mut budget = initial_budget;
    let status;
    {
        let mut guard = bridge.attach(&mut cpu).unwrap();
        let mut routed = Routed::attach(&mut guard, &router).unwrap();
        status = routed.run(GuestAddr(0x4000), &mut budget).unwrap();
    }
    assert_eq!(
        (
            cpu.base.n,
            cpu.base.z,
            cpu.base.c,
            cpu.base.v,
            cpu.base.fpcr,
            cpu.base.fpsr
        ),
        (1, 0, 1, 1, 0x400000, 0x18)
    );
    assert_eq!(
        cpu.base.vregs,
        [[0x123456789abcdef0, 0xfedcba9876543210]; 32]
    );
    assert_eq!(cpu.base.module_base, 0xdead);
    assert_eq!(cpu.base.x[1..8], args[1..8]);
    (status, cpu, budget)
}
fn make_fs(p: &Path) -> Arc<ReadOnlyFs> {
    ReadOnlyFs::new(
        p,
        "rom",
        Limits {
            handles: 4,
            read_bytes: 4096,
        },
    )
    .unwrap()
}
fn open(
    lib: &libloading::Library,
    f: &Arc<ReadOnlyFs>,
    m: &Arc<Mutex<ExclusiveMemory>>,
    b: &mut Bridge,
) -> u64 {
    path(m, b"rom:/payload.bin");
    let (status, c, budget) = invoke(lib, f, b, fs::OPEN, [0x1200, 0x1100, 1, 0, 0, 0, 0, 0], 16);
    assert_eq!((status, c.base.x[0], c.base.pc, budget), (0, 0, 0x9000, 12));
    word(m, 0x1200)
}
#[test]
fn native_hle_reads_real_files_counts_eof_exact_and_close() {
    let p = folder("io");
    let data = (0u8..101).collect::<Vec<_>>();
    std::fs::write(p.join("payload.bin"), &data).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt);
        let f = make_fs(&p);
        let m = memory();
        let mut b = Bridge::new(m.clone()).unwrap();
        let h = open(&lib, &f, &m, &mut b);
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::SIZE,
            [0x1210, h, 0, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!(
            (s, c.base.x[0], word(&m, 0x1210)),
            (0, 0, data.len() as u64)
        );
        for offset in [0usize, 3, 99, 100, 101, 150] {
            for size in [0usize, 1, 8, 64] {
                m.lock()
                    .unwrap()
                    .write(GuestAddr(0x1300), &[0xa5; 64])
                    .unwrap();
                let (s, c, _) = invoke(
                    &lib,
                    &f,
                    &mut b,
                    fs::READ,
                    [0x1220, h, offset as u64, 0x1300, size as u64, 0, 0, 0],
                    16,
                );
                let got = size.min(data.len().saturating_sub(offset));
                let mut expected = vec![0xa5; 64];
                if got > 0 {
                    expected[..got].copy_from_slice(&data[offset..offset + got]);
                }
                assert_eq!((s, c.base.x[0], word(&m, 0x1220)), (0, 0, got as u64));
                assert_eq!(peek(&m, 0x1300, 64), expected);
            }
        }
        m.lock()
            .unwrap()
            .write(GuestAddr(0x1300), &[0xa5; 8])
            .unwrap();
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::READ_EXACT,
            [h, 100, 0x1300, 8, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0]), (0, u64::from(fs::RESULT_FAIL)));
        assert_eq!(
            peek(&m, 0x1300, 8),
            [100, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5, 0xa5]
        );
        let (s, c, _) = invoke(&lib, &f, &mut b, fs::CLOSE, [h, 0, 0, 0, 0, 0, 0, 0], 16);
        assert_eq!((s, c.base.x[0]), (0, h));
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::SIZE,
            [0x1210, h, 0, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0]), (0, u64::from(fs::RESULT_FAIL)));
        assert_eq!(std::fs::read(p.join("payload.bin")).unwrap(), data);
    }
}
#[test]
fn native_hle_budget_output_faults_and_handle_namespaces_are_transactional() {
    let p = folder("faults");
    std::fs::write(p.join("payload.bin"), b"original fixture").unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt);
        for initial in 0..6 {
            let f = ReadOnlyFs::new(
                &p,
                "rom",
                Limits {
                    handles: 1,
                    read_bytes: 64,
                },
            )
            .unwrap();
            let m = memory();
            let mut b = Bridge::new(m.clone()).unwrap();
            path(&m, b"rom:/payload.bin");
            let before = peek(&m, 0x1200, 8);
            let (s, c, left) = invoke(
                &lib,
                &f,
                &mut b,
                fs::OPEN,
                [0x1200, 0x1100, 1, 0, 0, 0, 0, 0],
                initial,
            );
            assert_eq!(s, if initial < 4 { 2 } else { 0 });
            assert_eq!(left, initial.saturating_sub(4));
            if initial < 3 {
                assert_eq!(peek(&m, 0x1200, 8), before);
            } else {
                assert_eq!(c.base.x[0], 0);
                assert_ne!(word(&m, 0x1200), 0);
            }
        }
        let f = make_fs(&p);
        let m = memory();
        let mut b = Bridge::new(m.clone()).unwrap();
        path(&m, b"rom:/payload.bin");
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::OPEN,
            [0x6000, 0x1100, 1, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0], c.base.fault_address), (5, 0x6000, 0x6000));
        let h = open(&lib, &f, &m, &mut b);
        assert_eq!(h & 0xffffffff, 1);
        let old = peek(&m, 0x4ffc, 4);
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::READ,
            [0x1220, h, 0, 0x4ffc, 8, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.fault_address), (5, 0x4ffc));
        assert_eq!(peek(&m, 0x4ffc, 4), old);
        let old = peek(&m, 0x1300, 16);
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::READ,
            [0x6000, h, 0, 0x1300, 16, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.fault_address), (5, 0x6000));
        assert_eq!(peek(&m, 0x1300, 16), old);
        let foreign = make_fs(&p);
        let other = open(&lib, &foreign, &m, &mut b);
        assert_ne!(other, h);
        let (s, c, _) = invoke(
            &lib,
            &foreign,
            &mut b,
            fs::SIZE,
            [0x1210, h, 0, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0]), (0, u64::from(fs::RESULT_FAIL)));
        assert!(f
            .target("unimplemented", GuestAddr(0x8000), 0x8000)
            .is_err());
    }
}
#[test]
fn native_hle_paths_symlinks_modes_limits_and_stale_handles_refuse() {
    use std::os::unix::fs::symlink;
    let p = folder("paths");
    std::fs::write(p.join("payload.bin"), b"keep unchanged").unwrap();
    std::fs::create_dir_all(p.join("sub")).unwrap();
    std::fs::write(p.join("sub/é.bin"), b"unicode").unwrap();
    let _ = std::fs::remove_file(p.join("escape"));
    symlink("/etc", p.join("escape")).unwrap();
    let _ = std::fs::remove_file(p.join("link.bin"));
    symlink(p.join("payload.bin"), p.join("link.bin")).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt);
        let f = ReadOnlyFs::new(
            &p,
            "rom",
            Limits {
                handles: 1,
                read_bytes: 64,
            },
        )
        .unwrap();
        let m = memory();
        let mut b = Bridge::new(m.clone()).unwrap();
        for name in [
            b"rom:/../payload.bin".as_slice(),
            b"rom:/escape/passwd",
            b"rom:/link.bin",
            b"rom://payload.bin",
            b"other:/payload.bin",
            b"rom:/missing",
            b"rom:/sub",
        ] {
            path(&m, name);
            let before = peek(&m, 0x1200, 8);
            let (s, c, _) = invoke(
                &lib,
                &f,
                &mut b,
                fs::OPEN,
                [0x1200, 0x1100, 1, 0, 0, 0, 0, 0],
                16,
            );
            assert_eq!(s, 0);
            assert_ne!(c.base.x[0], 0);
            assert_eq!(peek(&m, 0x1200, 8), before);
        }
        path(&m, b"rom:/payload.bin");
        for mode in [2, 3, 4, 5, 6, 7] {
            let (s, c, _) = invoke(
                &lib,
                &f,
                &mut b,
                fs::OPEN,
                [0x1200, 0x1100, mode, 0, 0, 0, 0, 0],
                16,
            );
            assert_eq!((s, c.base.x[0]), (0, u64::from(fs::RESULT_FAIL)));
        }
        for mode in [0, 0x100] {
            assert_eq!(
                invoke(
                    &lib,
                    &f,
                    &mut b,
                    fs::OPEN,
                    [0x1200, 0x1100, mode, 0, 0, 0, 0, 0],
                    16
                )
                .0,
                3
            );
        }
        let h = open(&lib, &f, &m, &mut b);
        assert_eq!(
            invoke(
                &lib,
                &f,
                &mut b,
                fs::OPEN,
                [0x1200, 0x1100, 1, 0, 0, 0, 0, 0],
                16
            )
            .0,
            3
        );
        assert_eq!(
            invoke(
                &lib,
                &f,
                &mut b,
                fs::READ,
                [0x1220, h, 0, 0x1300, 65, 0, 0, 0],
                16
            )
            .0,
            3
        );
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::READ,
            [0x1220, h, u64::MAX, 0x1300, 1, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0]), (0, u64::from(fs::RESULT_FAIL)));
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::READ,
            [0x1220, h, u64::MAX, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0], word(&m, 0x1220)), (0, 0, 0));
        invoke(&lib, &f, &mut b, fs::CLOSE, [h, 0, 0, 0, 0, 0, 0, 0], 16);
        path(&m, "rom:/sub/é.bin".as_bytes());
        let (s, c, _) = invoke(
            &lib,
            &f,
            &mut b,
            fs::OPEN,
            [0x1200, 0x1100, 1, 0, 0, 0, 0, 0],
            16,
        );
        assert_eq!((s, c.base.x[0]), (0, 0));
        let newer = word(&m, 0x1200);
        assert_ne!(newer, h);
        assert_eq!(
            invoke(&lib, &f, &mut b, fs::CLOSE, [h, 0, 0, 0, 0, 0, 0, 0], 16).0,
            3
        );
    }
}
#[test]
fn native_hle_guest_file_writes_share_exclusive_invalidation_and_failed_batches_preserve_it() {
    let p = folder("monitors");
    std::fs::write(p.join("payload.bin"), [0xa5; 16]).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt);
        let f = make_fs(&p);
        let m = memory();
        let mut b = Bridge::new(m.clone()).unwrap();
        let h = open(&lib, &f, &m, &mut b);
        for failed in [false, true] {
            let mut c = CpuV10::default();
            {
                let mut guard = b.attach(&mut c).unwrap();
                let c = guard.cpu_mut();
                let ops = unsafe { &*c.exclusive };
                let mut value = [0; 2];
                assert_eq!(
                    unsafe {
                        ops.load.unwrap()(c.exclusive_context, 0x1300, 8, 0, value.as_mut_ptr(), 2)
                    },
                    0
                );
            }
            let (s, _, _) = invoke(
                &lib,
                &f,
                &mut b,
                fs::READ,
                [
                    if failed { 0x6000 } else { 0x1220 },
                    h,
                    0,
                    0x1300,
                    8,
                    0,
                    0,
                    0,
                ],
                16,
            );
            assert_eq!(s, if failed { 5 } else { 0 });
            let mut c = CpuV10::default();
            let mut guard = b.attach(&mut c).unwrap();
            let c = guard.cpu_mut();
            let ops = unsafe { &*c.exclusive };
            let value = [0xa5a5a5a5a5a5a5a5, 0];
            let mut stored = 99;
            assert_eq!(
                unsafe {
                    ops.store.unwrap()(
                        c.exclusive_context,
                        0x1300,
                        8,
                        0,
                        value.as_ptr(),
                        2,
                        &mut stored,
                    )
                },
                0
            );
            assert_eq!(stored, u32::from(failed));
        }
    }
}
