use mkw_arm64::{structures::Writeback, Op, Reg};
use mkw_runtime::{
    abi::Cpu,
    bridge::{MemoryBridge, MemoryOrder},
    GuestAddr, Memory, Permissions,
};
use std::{collections::BTreeSet, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words() -> Vec<u32> {
    let mut out = BTreeSet::new();
    for opcode in [0, 2, 4, 6, 7, 8, 10] {
        for q in 0..2 {
            for size in 0..4 {
                if q == 0 && size == 3 && [0, 4, 8].contains(&opcode) {
                    continue;
                }
                for load in 0..2 {
                    for post in [0, 0x009f0000, 0x00820000, 0x00800000, 0x00810000] {
                        for (rt, rn) in [(0, 1), (31, 1), (0, 31), (31, 31)] {
                            out.insert(
                                0x0c000000
                                    | (opcode << 12)
                                    | (q << 30)
                                    | (size << 10)
                                    | (load << 22)
                                    | post
                                    | (rn << 5)
                                    | rt,
                            );
                        }
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("multiple_{w:x}")))
        .collect()
}
fn bridge(
    address: u64,
    data: Vec<u8>,
    permissions: Permissions,
) -> std::pin::Pin<Box<MemoryBridge>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(address),
        data,
        permissions,
        "synthetic interleaved bytes",
    )
    .unwrap();
    m.seal();
    MemoryBridge::new(m)
}
fn raw(v: [u64; 2]) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&v[0].to_le_bytes());
    out[8..].copy_from_slice(&v[1].to_le_bytes());
    out
}
fn pack(v: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(v[..8].try_into().unwrap()),
        u64::from_le_bytes(v[8..].try_into().unwrap()),
    ]
}
fn read(memory: &MemoryBridge, address: u64, size: usize) -> Vec<u8> {
    (0..size)
        .map(|n| {
            let mut b = [0];
            memory
                .access(
                    MemoryOrder::Plain,
                    false,
                    GuestAddr(address + n as u64),
                    &mut b,
                    1,
                )
                .unwrap();
            b[0] as u8
        })
        .collect()
}
#[test]
fn multiple_memory_byte_oracles_and_transactional_faults() {
    let path = folder("multiple-known");
    let words = words();
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for &w in &words {
            let Op::InterleavedStructure(v) = mkw_arm64::decode(w, 0).unwrap().op else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("multiple_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let first = usize::from(v.vector);
            let count = usize::from(v.registers);
            let b = usize::from(v.bytes);
            let total = usize::from(v.vector_bytes);
            for (unaligned, index) in [(0, 5), (1, 5), (0, u64::MAX)] {
                let address = 0x1000 + unaligned;
                let data: Vec<u8> = (0u8..128)
                    .map(|n| n.wrapping_mul(37).wrapping_add(3))
                    .collect();
                let memory = bridge(0x1000, data.clone(), Permissions::RW);
                let mut cpu = Cpu {
                    sp: address,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpcr: 0x02c00000,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|r| 0xdeadbeef00000000 | r as u64);
                cpu.x[0] = 9;
                cpu.x[1] = address;
                cpu.x[2] = index;
                cpu.vregs = std::array::from_fn(|r| {
                    pack(std::array::from_fn(|i| {
                        (r as u8)
                            .wrapping_mul(19)
                            .wrapping_add((i as u8).wrapping_mul(13))
                    }))
                });
                let before = cpu.vregs;
                let x = cpu.x;
                let mut expected = before;
                let mut expected_memory = data.clone();
                let old: Vec<_> = (0..count)
                    .map(|slot| raw(before[(first + slot) % 32]))
                    .collect();
                let mut output = vec![[0u8; 16]; count];
                // Independent byte de/interleaving, without packed integer extraction.
                for lane in 0..total / b {
                    for slot in 0..count {
                        let at = unaligned as usize
                            + if v.interleaved {
                                (lane * count + slot) * b
                            } else {
                                (slot * total / b + lane) * b
                            };
                        if v.load {
                            output[slot][lane * b..(lane + 1) * b]
                                .copy_from_slice(&data[at..at + b]);
                        } else {
                            expected_memory[at..at + b]
                                .copy_from_slice(&old[slot][lane * b..(lane + 1) * b]);
                        }
                    }
                }
                if v.load {
                    for slot in 0..count {
                        expected[(first + slot) % 32] = pack(output[slot]);
                    }
                }
                {
                    let mut a = memory.as_ref().attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0, "{opt} {w:x}");
                }
                assert_eq!(cpu.vregs, expected, "{opt} {w:x}");
                assert_eq!(read(&memory, 0x1000, 128), expected_memory, "{opt} {w:x}");
                let step = match v.writeback {
                    Writeback::None => 0,
                    Writeback::Immediate(n) => u64::from(n),
                    Writeback::Register(Reg::Gpr(n)) => x[usize::from(n)],
                    _ => unreachable!(),
                };
                let mut expected_x = x;
                let expected_sp = if v.base == Reg::Sp {
                    address.wrapping_add(step)
                } else {
                    address
                };
                if let Reg::Gpr(n) = v.base {
                    expected_x[usize::from(n)] = address.wrapping_add(step);
                }
                assert_eq!(cpu.x, expected_x);
                assert_eq!(cpu.sp, expected_sp);
                assert_eq!(
                    (cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (1, 0, 1, 1, 0x02c00000, 0x08000080)
                );
            }
            for fault in 0..5 {
                let address = if fault == 4 { u64::MAX - 7 } else { 0x1000 };
                let mut cpu = Cpu {
                    sp: address,
                    fpcr: 0x02c00000,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.x[1] = address;
                cpu.x[2] = 5;
                cpu.vregs = std::array::from_fn(|r| [r as u64 + 123, r as u64 + 456]);
                let before = cpu.vregs;
                let x = cpu.x;
                if fault == 0 {
                    assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
                } else {
                    let len = if fault == 1 { total * count - 1 } else { 128 };
                    let permissions = if fault == 2 {
                        Permissions::R
                    } else if fault == 3 {
                        Permissions {
                            read: false,
                            write: true,
                            execute: false,
                        }
                    } else {
                        Permissions::RW
                    };
                    let memory = bridge(0x1000, vec![77; len], permissions);
                    let rc = {
                        let mut a = memory.as_ref().attach(&mut cpu);
                        unsafe { native(a.cpu_mut(), 10) }
                    };
                    if (fault == 2 && v.load) || (fault == 3 && !v.load) {
                        assert_eq!(rc, 0);
                        continue;
                    }
                    assert_ne!(rc, 0, "{opt} {w:x} fault {fault}");
                    if permissions.read {
                        assert_eq!(read(&memory, 0x1000, len), vec![77; len], "{w:x}");
                    }
                }
                assert_eq!(cpu.vregs, before);
                assert_eq!(cpu.x, x);
                assert_eq!(cpu.sp, address);
                assert_eq!((cpu.fpcr, cpu.fpsr), (0x02c00000, 0x08000080));
                assert_eq!(cpu.fault_address, address);
                assert_ne!(cpu.fault_status, 0);
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"instructions":words.len(),"success_cases_per_optimization":words.len()*3,"fault_or_permission_cases_per_optimization":words.len()*5,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn multiple_results_match_synthetic_qemu() {
    let path = folder("multiple-reference");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\"}\n",
        )
        .unwrap();
        return;
    }
    let words = words();
    let (image, address) = reference::multiple_elf(&words);
    let executable = path.join("synthetic-aarch64");
    std::fs::write(&executable, image).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .current_dir(&path)
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout.len(), words.len() * 176);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for (i, w) in words.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("multiple_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let memory = bridge(address, (0..64).collect(), Permissions::RW);
            let mut cpu = Cpu {
                sp: address,
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                fpcr: 0x02c00000,
                fpsr: 0x08000080,
                ..Cpu::default()
            };
            cpu.x[0] = 9;
            cpu.x[1] = address;
            cpu.x[2] = 5;
            cpu.vregs[0] = reference::INITIAL_V[0];
            cpu.vregs[1] = reference::INITIAL_V[1];
            cpu.vregs[2] = reference::INITIAL_V[2];
            cpu.vregs[3] = [0xabcdef0198765432, 0x1020304050607080];
            cpu.vregs[31] = reference::INITIAL_V[3];
            {
                let mut a = memory.as_ref().attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            let mut actual: Vec<u8> = [
                cpu.vregs[0],
                cpu.vregs[1],
                cpu.vregs[2],
                cpu.vregs[3],
                cpu.vregs[31],
            ]
            .into_iter()
            .flatten()
            .flat_map(u64::to_le_bytes)
            .collect();
            actual.extend(read(&memory, address, 64));
            let base = if (w >> 5) & 31 == 31 {
                cpu.sp
            } else {
                cpu.x[1]
            };
            actual.extend(
                [
                    base.wrapping_sub(address),
                    u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
                    u64::from(cpu.fpcr),
                    u64::from(cpu.fpsr),
                ]
                .into_iter()
                .flat_map(u64::to_le_bytes),
            );
            assert_eq!(actual, &out.stdout[i * 176..(i + 1) * 176], "{opt} {w:08x}");
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":words.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}

struct CallbackProbe {
    calls: u32,
    bytes: u32,
    write: i32,
    fail: bool,
}
unsafe extern "C" fn checked_probe(
    context: *mut std::ffi::c_void,
    write: i32,
    address: u64,
    value: *mut u64,
    bytes: u32,
) -> i32 {
    let p = unsafe { &mut *context.cast::<CallbackProbe>() };
    p.calls += 1;
    if address != 0x1000 || write != p.write || bytes != p.bytes {
        return 99;
    }
    // Even a failing read callback may have filled its temporary output buffer.
    if write == 0 {
        for i in 0..bytes / 8 {
            unsafe {
                *value.add(i as usize) = u64::MAX - u64::from(i);
            }
        }
    }
    if p.fail {
        6
    } else {
        0
    }
}
#[test]
fn one_complete_callback_and_no_commit_on_callback_failure() {
    let path = folder("multiple-callback");
    let words = [
        0x0cdf8020, 0x4cdf8020, 0x0c9f8020, 0x4c9f8020, 0x0cdf4020, 0x4cdf4020, 0x4cdf0020,
        0x4c9f0020, 0x4cdf2020, 0x4c9f6020,
    ];
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for word in words {
            let Op::InterleavedStructure(v) = mkw_arm64::decode(word, 0).unwrap().op else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("multiple_{word:x}\0").as_bytes())
                    .unwrap()
            };
            for fail in [false, true] {
                let mut probe = CallbackProbe {
                    calls: 0,
                    bytes: u32::from(v.vector_bytes) * u32::from(v.registers),
                    write: i32::from(!v.load),
                    fail,
                };
                let mut cpu = Cpu {
                    memory: Some(checked_probe),
                    memory_context: std::ptr::addr_of_mut!(probe).cast(),
                    fpcr: 0x02c00000,
                    fpsr: 0x08000080,
                    ..Cpu::default()
                };
                cpu.x[1] = 0x1000;
                cpu.vregs = std::array::from_fn(|r| [r as u64 + 123, r as u64 + 456]);
                let before = cpu.vregs;
                let x = cpu.x;
                assert_eq!(unsafe { native(&mut cpu, 10) }, if fail { 6 } else { 0 });
                assert_eq!(probe.calls, 1);
                if fail {
                    assert_eq!(cpu.vregs, before);
                    assert_eq!(cpu.x, x);
                    assert_eq!(cpu.fault_status, 6);
                }
                assert_eq!(cpu.fault_address, 0x1000);
                assert_eq!((cpu.fpcr, cpu.fpsr), (0x02c00000, 0x08000080));
            }
        }
    }
}
