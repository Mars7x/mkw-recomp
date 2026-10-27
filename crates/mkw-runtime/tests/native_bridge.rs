use mkw_runtime::{
    abi::{Cpu, CPU_ABI_VERSION},
    bridge::{MemoryBridge, MemoryOrder},
    GuestAddr, Memory, Permissions,
};
use std::path::Path;
fn fixture() -> std::pin::Pin<Box<MemoryBridge>> {
    let mut memory = Memory::default();
    memory
        .map(GuestAddr(0x1000), vec![0; 8], Permissions::RW, "first")
        .unwrap();
    memory
        .map(GuestAddr(0x1008), vec![0; 8], Permissions::RW, "adjacent")
        .unwrap();
    memory
        .map(GuestAddr(0x2000), vec![0; 8], Permissions::R, "read-only")
        .unwrap();
    memory.seal();
    MemoryBridge::new(memory)
}
#[test]
fn cross_mapping_faults_are_transactional() {
    let bridge = fixture();
    let mut words = [0x8877665544332211, 0];
    bridge
        .access(MemoryOrder::Plain, true, GuestAddr(0x1004), &mut words, 8)
        .unwrap();
    words = [0; 2];
    bridge
        .access(MemoryOrder::Plain, false, GuestAddr(0x1004), &mut words, 8)
        .unwrap();
    assert_eq!(words[0], 0x8877665544332211);
    let original = words;
    assert!(bridge
        .access(MemoryOrder::Plain, true, GuestAddr(0x100c), &mut words, 8)
        .is_err());
    assert!(bridge
        .access(MemoryOrder::Plain, false, GuestAddr(0x100c), &mut words, 8)
        .is_err());
    assert_eq!(words, original);
    assert!(bridge
        .access(MemoryOrder::Release, true, GuestAddr(0x2000), &mut words, 8)
        .is_err());
    assert!(bridge
        .access(
            MemoryOrder::AcquireRcpc,
            false,
            GuestAddr(0x1001),
            &mut words,
            8
        )
        .is_err());
    let mut readback = [0; 2];
    bridge
        .access(
            MemoryOrder::Plain,
            false,
            GuestAddr(0x1008),
            &mut readback,
            8,
        )
        .unwrap();
    assert_eq!(readback[0], 0x88776655);
    assert!(bridge.last_fault().unwrap().contains("unaligned"));
}
#[test]
fn shared_bridge_publishes_release_to_acquire() {
    let bridge = fixture();
    std::thread::scope(|scope| {
        let shared = bridge.as_ref().get_ref();
        let writer = scope.spawn(move || {
            let mut data = [0xabcdef12, 0];
            shared
                .access(MemoryOrder::Plain, true, GuestAddr(0x1008), &mut data, 4)
                .unwrap();
            let mut flag = [1, 0];
            shared
                .access(MemoryOrder::Release, true, GuestAddr(0x1000), &mut flag, 4)
                .unwrap();
        });
        let reader = scope.spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                let mut flag = [0; 2];
                shared
                    .access(
                        MemoryOrder::AcquireRcpc,
                        false,
                        GuestAddr(0x1000),
                        &mut flag,
                        4,
                    )
                    .unwrap();
                if flag[0] == 1 {
                    let mut data = [0; 2];
                    shared
                        .access(MemoryOrder::Plain, false, GuestAddr(0x1008), &mut data, 4)
                        .unwrap();
                    assert_eq!(data[0], 0xabcdef12);
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "publication timed out"
                );
                std::thread::yield_now();
            }
        });
        writer.join().unwrap();
        reader.join().unwrap();
    });
    assert_eq!(bridge.counts()[3], 1);
    assert!(bridge.counts()[1] >= 1);
}
#[test]
fn generated_native_code_uses_rust_memory_abi() {
    // Independently authored STR W1,[X0,4]; STLR W2,[X0]; LDAPRB W3,[X0];
    // LDAR W5,[X0]; LDR W4,[X0,4]; RET.
    let words = [
        0xb9000401u32,
        0x889ffc02,
        0x38bfc003,
        0x88dffc05,
        0xb9400404,
        0xd65f03c0,
    ];
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let ir = mkw_arm64::lift(&bytes, 0x8000, 0x8000).unwrap();
    let mut code = mkw_codegen::emit(&ir, 0x8000).unwrap();
    code += r#"
unsigned cpu_abi(void){return MKW_CPU_ABI_VERSION;}
size_t cpu_size(void){return sizeof(MkwCpu);}
size_t cpu_offset(unsigned field){switch(field){case 0:return offsetof(MkwCpu,pauth);case 1:return offsetof(MkwCpu,memory);case 2:return offsetof(MkwCpu,fault_status);case 3:return offsetof(MkwCpu,call);default:return offsetof(MkwCpu,ordered_memory);}}
"#;
    let widths = [
        0x38bfc001u32,
        0x78bfc002,
        0xb8bfc003,
        0xf8bfc004,
        0x089ffca1,
        0x489ffca2,
        0x889ffca3,
        0xc89ffca4,
        0xd65f03c0,
    ];
    let width_bytes: Vec<_> = widths.iter().flat_map(|w| w.to_le_bytes()).collect();
    code += &mkw_codegen::emit(&mkw_arm64::lift(&width_bytes, 0, 0).unwrap(), 0)
        .unwrap()
        .replace(mkw_codegen::HEADER, "")
        .replace("mkw_guest", "widths");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/rust-native-memory");
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("generated.c");
    std::fs::write(&source, code).unwrap();
    for optimization in ["-O0", "-O2"] {
        let library = folder.join(format!("native{}.so", &optimization[2..]));
        let built = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
                "-shared",
                "-fPIC",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        // SAFETY: this test compiled the exact emitted ABI, validates layout before
        // invocation, and retains the library and pinned attachment until it returns.
        unsafe {
            let lib = libloading::Library::new(&library).unwrap();
            let size = lib
                .get::<unsafe extern "C" fn() -> usize>(b"cpu_size\0")
                .unwrap();
            let offset = lib
                .get::<unsafe extern "C" fn(u32) -> usize>(b"cpu_offset\0")
                .unwrap();
            let version = lib
                .get::<unsafe extern "C" fn() -> u32>(b"cpu_abi\0")
                .unwrap();
            assert_eq!(version(), CPU_ABI_VERSION);
            assert_eq!(size(), std::mem::size_of::<Cpu>());
            for (i, expected) in [
                std::mem::offset_of!(Cpu, pauth),
                std::mem::offset_of!(Cpu, memory),
                std::mem::offset_of!(Cpu, fault_status),
                std::mem::offset_of!(Cpu, call),
                std::mem::offset_of!(Cpu, ordered_memory),
            ]
            .iter()
            .enumerate()
            {
                assert_eq!(offset(i as u32), *expected);
            }
            let native = lib
                .get::<unsafe extern "C" fn(*mut Cpu, u64) -> i32>(b"mkw_guest\0")
                .unwrap();
            let bridge = fixture();
            let mut cpu = Cpu::default();
            cpu.x[0] = 0x1000;
            cpu.x[1] = 0xabcdef12;
            cpu.x[2] = 1;
            cpu.x[3] = u64::MAX;
            cpu.x[4] = u64::MAX;
            cpu.x[30] = 0xdead;
            {
                let mut attachment = bridge.as_ref().attach(&mut cpu);
                assert_eq!(native(attachment.cpu_mut(), 100), 0);
            }
            assert_eq!(
                (cpu.x[3], cpu.x[4], cpu.x[5], cpu.pc),
                (1, 0xabcdef12, 1, 0xdead)
            );
            assert!(cpu.memory.is_none());
            assert!(cpu.ordered_memory.is_none());
            assert_eq!(bridge.counts(), [2, 1, 1, 1]);
            let width_native = lib
                .get::<unsafe extern "C" fn(*mut Cpu, u64) -> i32>(b"widths\0")
                .unwrap();
            let width_bridge = fixture();
            let mut initial = [0x8877665544332211, 0];
            width_bridge
                .access(MemoryOrder::Plain, true, GuestAddr(0x1000), &mut initial, 8)
                .unwrap();
            let mut width_cpu = Cpu::default();
            width_cpu.x[0] = 0x1000;
            width_cpu.x[5] = 0x1008;
            for r in 1..=4 {
                width_cpu.x[r] = u64::MAX;
            }
            {
                let mut attached = width_bridge.as_ref().attach(&mut width_cpu);
                assert_eq!(width_native(attached.cpu_mut(), 100), 0);
            }
            assert_eq!(
                &width_cpu.x[1..5],
                &[0x11, 0x2211, 0x44332211, 0x8877665544332211]
            );
            let mut readback = [0; 2];
            width_bridge
                .access(
                    MemoryOrder::Plain,
                    false,
                    GuestAddr(0x1008),
                    &mut readback,
                    8,
                )
                .unwrap();
            assert_eq!(readback[0], initial[0]);
            assert_eq!(width_bridge.counts()[1], 4);
            assert_eq!(width_bridge.counts()[3], 4);
            cpu = Cpu::default();
            cpu.x[0] = 0x1000;
            {
                let mut attachment = bridge.as_ref().attach(&mut cpu);
                attachment.cpu_mut().ordered_memory = None;
                assert_eq!(native(attachment.cpu_mut(), 100), 3);
                assert_eq!(attachment.cpu_mut().pc, 0x8004);
            }
            cpu = Cpu::default();
            cpu.x[0] = 0x1ffc;
            {
                let mut attachment = bridge.as_ref().attach(&mut cpu);
                assert_eq!(native(attachment.cpu_mut(), 100), 5);
                assert_eq!(attachment.cpu_mut().pc, 0x8000);
            }
        }
    }
}

#[test]
fn registered_indirect_native_calls_rebase_and_refuse_unknown_targets() {
    use mkw_runtime::dispatch::{NativeDispatcher, NativeTarget};
    // Independently authored caller saves LR, loads target, BLRs and restores LR.
    let caller = [
        0xa9bf7bfdu32,
        0xd2800541,
        0xd2920009,
        0xd63f0120,
        0xa8c17bfd,
        0xd65f03c0,
    ];
    let callee = [0x10000026u32, 0x11000420, 0xd65f03c0];
    let generate = |words: &[u32]| {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        mkw_codegen::emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0).unwrap()
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/native-dispatch");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("caller.c"), generate(&caller)).unwrap();
    std::fs::write(
        folder.join("callee.c"),
        generate(&callee).replace("mkw_guest", "callee"),
    )
    .unwrap();
    let library = folder.join("entries.so");
    let build = std::process::Command::new("cc")
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=undefined",
        ])
        .arg(folder.join("caller.c"))
        .arg(folder.join("callee.c"))
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    // SAFETY: both symbols were compiled above from the ABI-v4 emitter. The
    // library outlives all borrowed targets/attachments and all native invocations.
    unsafe {
        let lib = libloading::Library::new(&library).unwrap();
        let entry = lib
            .get::<unsafe extern "C" fn(*mut Cpu, u64) -> i32>(b"mkw_guest\0")
            .unwrap();
        let target =
            NativeTarget::load(&lib, b"callee_budget\0", GuestAddr(0x9000), 0x9000).unwrap();
        let dispatcher = NativeDispatcher::new(vec![target]).unwrap();
        let bridge = fixture();
        let mut cpu = Cpu {
            sp: 0x1010,
            module_base: 0x8000,
            ..Cpu::default()
        };
        cpu.x[30] = 0xdead;
        {
            let mut memory = bridge.as_ref().attach(&mut cpu);
            let mut calls = dispatcher.as_ref().attach(memory.cpu_mut());
            assert_eq!(entry(calls.cpu_mut(), 100), 0);
        }
        assert_eq!(
            (cpu.x[0], cpu.x[6], cpu.sp, cpu.pc, cpu.module_base),
            (43, 0x9004, 0x1010, 0xdead, 0x8000)
        );
        assert!(cpu.call.is_none());
        let missing = NativeDispatcher::new(vec![]).unwrap();
        cpu = Cpu::default();
        cpu.sp = 0x1010;
        cpu.module_base = 0x8000;
        {
            let mut memory = bridge.as_ref().attach(&mut cpu);
            let mut calls = missing.as_ref().attach(memory.cpu_mut());
            assert_eq!(entry(calls.cpu_mut(), 100), 3);
            assert_eq!(calls.cpu_mut().pc, 0x800c);
            assert_eq!(calls.cpu_mut().fault_address, 0x9000);
        }
        cpu = Cpu::default();
        cpu.sp = 0x1010;
        cpu.module_base = 0x8000;
        {
            let mut memory = bridge.as_ref().attach(&mut cpu);
            let mut calls = dispatcher.as_ref().attach(memory.cpu_mut());
            assert_eq!(entry(calls.cpu_mut(), 5), 2);
            assert_eq!(calls.cpu_mut().pc, 0x9004);
            assert_eq!(calls.cpu_mut().module_base, 0x8000);
        }
    }
}
