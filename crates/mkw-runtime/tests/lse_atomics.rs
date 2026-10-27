use mkw_runtime::{
    abi::Cpu,
    atomics::{AtomicOp, AtomicOrder},
    bridge::{MemoryBridge, MemoryOrder},
    GuestAddr, Memory, Permissions,
};
use std::{path::Path, process::Command};
fn word(size: u32, order: u32, operation: u32, rs: u32, rt: u32, rn: u32) -> u32 {
    if operation == 9 {
        0x08a07c00
            | (size << 30)
            | ((order & 1) << 22)
            | ((order >> 1) << 15)
            | (rs << 16)
            | (rn << 5)
            | rt
    } else {
        0x38200000
            | (size << 30)
            | ((order & 1) << 23)
            | ((order >> 1) << 22)
            | (operation << 12)
            | (rs << 16)
            | (rn << 5)
            | rt
    }
}
fn bridge() -> std::pin::Pin<Box<MemoryBridge>> {
    let mut memory = Memory::default();
    for (at, len, perm) in [
        (0x1000, 4, Permissions::RW),
        (0x1004, 60, Permissions::RW),
        (0x2000, 16, Permissions::R),
    ] {
        memory
            .map(GuestAddr(at), vec![0; len], perm, "synthetic")
            .unwrap();
    }
    memory.seal();
    MemoryBridge::new(memory)
}
fn store(bridge: &MemoryBridge, value: u64) {
    bridge
        .access(MemoryOrder::Plain, true, GuestAddr(0x1000), &mut [value], 8)
        .unwrap();
}
fn read(bridge: &MemoryBridge) -> u64 {
    let mut words = [0];
    bridge
        .access(MemoryOrder::Plain, false, GuestAddr(0x1000), &mut words, 8)
        .unwrap();
    words[0]
}
fn source(words: &[u32], name: &str) -> String {
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    mkw_codegen::emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
        .unwrap()
        .replace(mkw_codegen::HEADER, "")
        .replace("mkw_guest", name)
}
fn build(code: &str, label: &str, opt: &str) -> libloading::Library {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing")
        .join(label);
    std::fs::create_dir_all(&folder).unwrap();
    let src = folder.join("native.c");
    let lib = folder.join(format!("native{}.so", &opt[1..]));
    std::fs::write(&src,format!("{}\n{code}\nsize_t cpu_size(void){{return sizeof(MkwCpu);}}\nsize_t atomic_offset(void){{return offsetof(MkwCpu,atomic_memory);}}\nunsigned abi_version(void){{return MKW_CPU_ABI_VERSION;}}\n",mkw_codegen::HEADER)).unwrap();
    let result = Command::new("cc")
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
        ])
        .arg(&src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // SAFETY: trusted synthetic C built above from our header and emitter.
    unsafe {
        let library = libloading::Library::new(&lib).unwrap();
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"cpu_size\0")
                .unwrap()(),
            std::mem::size_of::<Cpu>()
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"atomic_offset\0")
                .unwrap()(),
            std::mem::offset_of!(Cpu, atomic_memory)
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> u32>(b"abi_version\0")
                .unwrap()(),
            mkw_runtime::abi::CPU_ABI_VERSION
        );
        library
    }
}
type Entry = unsafe extern "C" fn(*mut Cpu, u64) -> i32;
fn signed(value: u64, bits: u32) -> i128 {
    let value = i128::from(value);
    if value & (1i128 << (bits - 1)) != 0 {
        value - (1i128 << bits)
    } else {
        value
    }
}
#[test]
fn native_lse_widths_orders_operations_cas_and_register_aliases() {
    let mut code = String::new();
    for size in 0..4 {
        for order in 0..4 {
            for op in 0..10 {
                code += &source(
                    &[word(size, order, op, 0, 1, 2), 0xd65f03c0],
                    &format!("lse_{size}_{order}_{op}"),
                );
            }
        }
    }
    code += &source(&[word(3, 1, 0, 1, 31, 31), 0xd65f03c0], "discard");
    code += &source(&[word(3, 3, 9, 2, 1, 2), 0xd65f03c0], "overlap");
    for opt in ["-O0", "-O2"] {
        let library = build(&code, "lse-native", opt);
        let bridge = bridge();
        for size in 0..4u32 {
            let bytes = 1usize << size;
            let bits = bytes as u32 * 8;
            let mask = if bytes == 8 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            for order in 0..4 {
                for op in 0..10 {
                    // SAFETY: symbols are trusted generated functions; library and
                    // pinned callback attachment outlive each call.
                    let native = unsafe {
                        library
                            .get::<Entry>(format!("lse_{size}_{order}_{op}\0").as_bytes())
                            .unwrap()
                    };
                    for old in [0, 1, mask, 1u64 << (bits - 1), (1u64 << (bits - 1)) - 1] {
                        for input in [0, 1, mask, 1u64 << (bits - 1)] {
                            let replacement = 0xa55aa55aa55aa55a;
                            let operand = input & mask;
                            let expected = match op {
                                0 => old.wrapping_add(operand),
                                1 => old & !operand,
                                2 => old ^ operand,
                                3 => old | operand,
                                4 => {
                                    if signed(old, bits) > signed(operand, bits) {
                                        old
                                    } else {
                                        operand
                                    }
                                }
                                5 => {
                                    if signed(old, bits) < signed(operand, bits) {
                                        old
                                    } else {
                                        operand
                                    }
                                }
                                6 => old.max(operand),
                                7 => old.min(operand),
                                8 => operand,
                                9 => {
                                    if old == operand {
                                        replacement
                                    } else {
                                        old
                                    }
                                }
                                _ => unreachable!(),
                            } & mask;
                            let initial = (0xfedcba9876543210 & !mask) | old;
                            store(&bridge, initial);
                            let mut cpu = Cpu {
                                n: 1,
                                z: 0,
                                c: 1,
                                v: 1,
                                ..Cpu::default()
                            };
                            cpu.x[0] = input;
                            cpu.x[1] = if op == 9 { replacement } else { 0xdeadbeef };
                            cpu.x[2] = 0x1000;
                            cpu.x[30] = 0xdead;
                            {
                                let mut attached = bridge.as_ref().attach(&mut cpu);
                                assert_eq!(unsafe { native(attached.cpu_mut(), 8) }, 0);
                            }
                            assert_eq!(cpu.x[if op == 9 { 0 } else { 1 }], old);
                            assert_eq!(
                                read(&bridge),
                                (initial & !mask) | expected,
                                "size {size} op {op}"
                            );
                            assert_eq!((cpu.n, cpu.z, cpu.c, cpu.v, cpu.pc), (1, 0, 1, 1, 0xdead));
                            assert!(cpu.atomic_memory.is_none());
                        }
                    }
                }
            }
        }
        let mut cpu = Cpu {
            sp: 0x1000,
            ..Cpu::default()
        };
        cpu.x[1] = 1;
        store(&bridge, 7);
        let before = bridge.atomic_counts();
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            let native = unsafe { library.get::<Entry>(b"discard\0").unwrap() };
            assert_eq!(unsafe { native(attached.cpu_mut(), 8) }, 0);
        }
        assert_eq!(read(&bridge), 8);
        assert_eq!(bridge.atomic_counts()[0], before[0] + 1);
        assert_eq!(bridge.atomic_counts()[1], before[1]);
        cpu = Cpu::default();
        cpu.x[2] = 0x1000;
        cpu.x[1] = 77;
        store(&bridge, 0x1000);
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            let native = unsafe { library.get::<Entry>(b"overlap\0").unwrap() };
            assert_eq!(unsafe { native(attached.cpu_mut(), 8) }, 0);
        }
        assert_eq!((cpu.x[2], read(&bridge)), (0x1000, 77));
    }
}
#[test]
fn atomic_faults_and_missing_backend_preserve_state() {
    let library = build(
        &source(&[word(3, 3, 9, 0, 1, 2), 0xd65f03c0], "cas"),
        "lse-fault",
        "-O2",
    );
    let bridge = bridge();
    let native = unsafe { library.get::<Entry>(b"cas\0").unwrap() };
    for address in [0x1001, 0x1040, 0x2000, u64::MAX - 7] {
        let mut cpu = Cpu::default();
        cpu.x[0] = 0xdead;
        cpu.x[1] = 3;
        cpu.x[2] = address;
        store(&bridge, 0x1234);
        let counts = bridge.atomic_counts();
        {
            let mut attached = bridge.as_ref().attach(&mut cpu);
            assert_eq!(unsafe { native(attached.cpu_mut(), 8) }, 5);
        }
        assert_eq!(cpu.x[0], 0xdead);
        assert_eq!(cpu.fault_address, address);
        assert_eq!(read(&bridge), 0x1234);
        assert_eq!(bridge.atomic_counts(), counts);
    }
    let mut cpu = Cpu::default();
    cpu.x[0] = 123;
    cpu.x[2] = 0x1000;
    assert_eq!(unsafe { native(&mut cpu, 8) }, 3);
    assert_eq!(cpu.x[0], 123);
    assert!(bridge
        .atomic_access(
            AtomicOrder::Relaxed,
            AtomicOp::Add,
            GuestAddr(0x1000),
            1,
            0,
            3
        )
        .is_err());
}
#[test]
fn contended_native_fetch_add_and_cas_have_no_lost_updates() {
    let code = source(&[word(3, 3, 0, 1, 0, 2), 0xd65f03c0], "add")
        + &source(&[word(3, 3, 9, 0, 1, 2), 0xd65f03c0], "cas");
    let library = build(&code, "lse-contention", "-O2");
    let bridge = bridge();
    let add = unsafe { *library.get::<Entry>(b"add\0").unwrap() };
    let cas = unsafe { *library.get::<Entry>(b"cas\0").unwrap() };
    let old = std::thread::scope(|scope| {
        let mut threads = Vec::new();
        for _ in 0..4 {
            let bridge = &bridge;
            threads.push(scope.spawn(move || {
                let mut cpu = Cpu::default();
                cpu.x[1] = 1;
                cpu.x[2] = 0x1000;
                let mut values = Vec::new();
                let mut attached = bridge.as_ref().attach(&mut cpu);
                for _ in 0..1000 {
                    assert_eq!(unsafe { add(attached.cpu_mut(), 8) }, 0);
                    values.push(attached.cpu_mut().x[0]);
                }
                values
            }));
        }
        threads
            .into_iter()
            .flat_map(|t| t.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut old = old;
    old.sort_unstable();
    assert_eq!(old, (0..4000).collect::<Vec<_>>());
    assert_eq!(read(&bridge), 4000);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let bridge = &bridge;
            scope.spawn(move || {
                let mut cpu = Cpu::default();
                cpu.x[2] = 0x1000;
                let mut attached = bridge.as_ref().attach(&mut cpu);
                for _ in 0..1000 {
                    let mut expected = read(bridge);
                    loop {
                        attached.cpu_mut().x[0] = expected;
                        attached.cpu_mut().x[1] = expected + 1;
                        assert_eq!(unsafe { cas(attached.cpu_mut(), 8) }, 0);
                        let actual = attached.cpu_mut().x[0];
                        if actual == expected {
                            break;
                        }
                        expected = actual;
                    }
                }
            });
        }
    });
    assert_eq!(read(&bridge), 8000);
}

#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
#[test]
fn native_lse_matches_qemu_synthetic_reference() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/lse-reference");
    std::fs::create_dir_all(&folder).unwrap();
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            folder.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\",\"hardware_reference\":false}\n",
        )
        .unwrap();
        eprintln!("optional qemu-aarch64 reference unavailable");
        return;
    }
    let mut words = Vec::new();
    let mut code = String::new();
    for size in 0..4 {
        for order in 0..4 {
            for op in 0..10 {
                let w = if op == 9 {
                    word(size, order, op, 0, 1, 2)
                } else {
                    word(size, order, op, 1, 0, 2)
                };
                code += &source(&[w, 0xd65f03c0], &format!("fixture{}", words.len()));
                words.push(w);
            }
        }
    }
    let path = folder.join("synthetic-aarch64");
    std::fs::write(&path, reference::atomic_elf(&words)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let oracle = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "QEMU {:?}: {}",
        oracle.status.code(),
        String::from_utf8_lossy(&oracle.stderr)
    );
    assert_eq!(oracle.stdout.len(), words.len() * 32);
    for opt in ["-O0", "-O2"] {
        let library = build(&code, "lse-reference", opt);
        let bridge = bridge();
        for (i, &w) in words.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("fixture{i}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.x[..3].copy_from_slice(&reference::INITIAL_X);
            cpu.x[2] = 0x1000;
            store(&bridge, reference::INITIAL_X[0]);
            {
                let mut attachment = bridge.as_ref().attach(&mut cpu);
                assert_eq!(unsafe { native(attachment.cpu_mut(), 8) }, 0);
            }
            let mut actual = Vec::new();
            for value in [
                read(&bridge),
                0,
                cpu.x[0],
                ((cpu.n as u64) << 31)
                    | ((cpu.z as u64) << 30)
                    | ((cpu.c as u64) << 29)
                    | ((cpu.v as u64) << 28),
            ] {
                actual.extend(value.to_le_bytes());
            }
            assert_eq!(
                actual,
                &oracle.stdout[i * 32..(i + 1) * 32],
                "LSE word {w:08x}, {opt}"
            );
        }
    }
    let qemu_version = String::from_utf8(
        Command::new("qemu-aarch64")
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let receipt = serde_json::json!({"qemu_version":qemu_version.lines().next(),"synthetic_cases":words.len(),"reference":"qemu-aarch64 -cpu max","native_optimizations":["O0","O2"],"fatal_ubsan":true,"weak_memory_reference":false,"hardware_reference":false,"game_code_executed":false});
    std::fs::write(
        folder.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
}
