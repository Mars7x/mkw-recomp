use mkw_runtime::{abi::Cpu, bridge::MemoryBridge, GuestAddr, Memory, Permissions};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
// Independently authored system-register encoding fields from the A64 layout.
fn system(read: bool, op2: u32, rt: u32) -> u32 {
    0xd5000000 | (u32::from(read) << 21) | (3 << 19) | (3 << 16) | (13 << 12) | (op2 << 5) | rt
}
#[test]
fn configured_thread_registers_are_guest_state_and_unconfigured_access_refuses() {
    let path = folder("thread-registers");
    let code = generate(
        &[system(true, 2, 0), system(true, 3, 1), 0xd65f03c0],
        "read",
    ) + &generate(
        &[system(false, 2, 0), system(true, 2, 1), 0xd65f03c0],
        "write",
    ) + &generate(
        &[
            system(false, 2, 31),
            system(true, 2, 2),
            system(true, 3, 31),
            0xd65f03c0,
        ],
        "zero",
    );
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let read = unsafe { library.get::<Entry>(b"read\0").unwrap() };
        let write = unsafe { library.get::<Entry>(b"write\0").unwrap() };
        let zero = unsafe { library.get::<Entry>(b"zero\0").unwrap() };
        let mut cpu = Cpu {
            sp: 99,
            module_base: 0x8000,
            n: 1,
            z: 0,
            c: 1,
            v: 1,
            ..Cpu::default()
        };
        cpu.x[0] = 777;
        cpu.x[1] = 888;
        assert_eq!(unsafe { read(&mut cpu, 10) }, 3);
        assert_eq!((cpu.x[0], cpu.x[1], cpu.sp, cpu.pc), (777, 888, 99, 0x8000));
        assert_eq!(unsafe { write(&mut cpu, 10) }, 3);
        assert_eq!(cpu.tpidr_el0, 0);
        cpu.configure_thread_registers(0x700000001000, 0x700000002000);
        cpu.x[30] = 0xdead;
        assert_eq!(unsafe { read(&mut cpu, 10) }, 0);
        assert_eq!(
            (cpu.x[0], cpu.x[1], cpu.pc, cpu.sp),
            (0x700000001000, 0x700000002000, 0xdead, 99)
        );
        cpu.x[0] = 0xfedcba9876543210;
        assert_eq!(unsafe { write(&mut cpu, 10) }, 0);
        assert_eq!(
            (cpu.tpidr_el0, cpu.x[1], cpu.tpidrro_el0),
            (0xfedcba9876543210, 0xfedcba9876543210, 0x700000002000)
        );
        assert_eq!(unsafe { zero(&mut cpu, 10) }, 0);
        assert_eq!(
            (cpu.tpidr_el0, cpu.x[2], cpu.tpidrro_el0, cpu.sp),
            (0, 0, 0x700000002000, 99)
        );
        assert_eq!((cpu.n, cpu.z, cpu.c, cpu.v), (1, 0, 1, 1));
        cpu.thread_state_configured = 2;
        assert_eq!(unsafe { read(&mut cpu, 10) }, 3);
    }
}
#[test]
fn native_threads_use_distinct_guest_pointers_through_checked_memory() {
    let path = folder("thread-register-memory");
    let code = generate(
        &[
            system(true, 3, 0),
            0xf9400001,
            system(true, 2, 2),
            0xf9400043,
            0xd65f03c0,
        ],
        "thread",
    );
    let library = build(&code, &path, "-O2");
    let native = unsafe { *library.get::<Entry>(b"thread\0").unwrap() };
    let mut memory = Memory::default();
    for (address, value) in [(0x1000, 11u64), (0x2000, 22), (0x3000, 33), (0x4000, 44)] {
        memory
            .map(
                GuestAddr(address),
                value.to_le_bytes().to_vec(),
                Permissions::R,
                "synthetic guest thread block",
            )
            .unwrap();
    }
    memory.seal();
    let memory = MemoryBridge::new(memory);
    std::thread::scope(|scope| {
        for (rw, ro, a, b) in [(0x1000, 0x2000, 11, 22), (0x3000, 0x4000, 33, 44)] {
            let memory = &memory;
            scope.spawn(move || {
                let mut cpu = Cpu {
                    module_base: 0x8000,
                    ..Cpu::default()
                };
                cpu.configure_thread_registers(rw, ro);
                let mut attachment = memory.as_ref().attach(&mut cpu);
                for _ in 0..1000 {
                    assert_eq!(unsafe { native(attachment.cpu_mut(), 20) }, 0);
                    assert_eq!(
                        (
                            attachment.cpu_mut().x[0],
                            attachment.cpu_mut().x[1],
                            attachment.cpu_mut().x[2],
                            attachment.cpu_mut().x[3]
                        ),
                        (ro, b, rw, a)
                    );
                }
            });
        }
    });
    let mut cpu = Cpu::default();
    cpu.x[1] = 777;
    cpu.configure_thread_registers(0x1000, 0xdead0000);
    let mut attachment = memory.as_ref().attach(&mut cpu);
    assert_eq!(unsafe { native(attachment.cpu_mut(), 20) }, 5);
    assert_eq!(attachment.cpu_mut().fault_address, 0xdead0000);
    assert_eq!(attachment.cpu_mut().x[1], 777);
}
