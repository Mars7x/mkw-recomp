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
fn forms() -> Vec<u32> {
    let mut out = Vec::new();
    for bytes in [1, 2, 4, 8] {
        for lane in 0..16 / bytes {
            let (op, q, s, size) = match bytes {
                1 => (0, lane / 8, (lane / 4) % 2, lane % 4),
                2 => (2, lane / 4, (lane / 2) % 2, (lane % 2) * 2),
                4 => (4, lane / 2, lane % 2, 0),
                8 => (4, lane, 0, 1),
                _ => unreachable!(),
            };
            for load in 0..2 {
                out.push(
                    0x0d000000 | (q << 30) | (load << 22) | (op << 13) | (s << 12) | (size << 10),
                );
            }
        }
    }
    for size in 0..4 {
        for q in 0..2 {
            out.push(0x0d40c000 | (q << 30) | (size << 10));
        }
    }
    out
}
fn words() -> Vec<u32> {
    let mut out = Vec::new();
    for base in forms() {
        for post in [0, 0x009f0000, 0x00820000, 0x00800000] {
            for (rt, rn) in [(0, 1), (31, 1), (0, 31), (31, 31)] {
                out.push(base | post | (rn << 5) | rt);
            }
        }
    }
    out
}
fn codes(words: &[u32]) -> String {
    words
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|w| generate(&[w, 0xd65f03c0], &format!("structure_{w:x}")))
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
        "synthetic structure bytes",
    )
    .unwrap();
    m.seal();
    MemoryBridge::new(m)
}
fn vector_bytes(v: [u64; 2]) -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(&v[0].to_le_bytes());
    out[8..].copy_from_slice(&v[1].to_le_bytes());
    out
}
#[test]
fn lane_memory_preservation_broadcast_writeback_and_faults() {
    let path = folder("structure-known");
    let mut words = words();
    // Post-index register aliases the base: its old value must be captured.
    words.extend(forms().into_iter().map(|w| w | 0x00810020));
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for &w in &words {
            let Op::Structure(v) = mkw_arm64::decode(w, 0).unwrap().op else {
                panic!()
            };
            let native = unsafe {
                lib.get::<Entry>(format!("structure_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for (unaligned, index) in [(0, 5), (1, 5), (0, u64::MAX)] {
                let address = 0x1000 + unaligned;
                let data: Vec<u8> = (0..32).map(|n| n * 7 + 3).collect();
                let memory = bridge(0x1000, data.clone(), Permissions::RW);
                let mut cpu = Cpu {
                    sp: address,
                    n: 1,
                    z: 0,
                    c: 1,
                    v: 1,
                    fpsr: 0x08000080,
                    fpcr: 0x02c00000,
                    ..Cpu::default()
                };
                cpu.x[0] = 9;
                cpu.x[1] = address;
                cpu.x[2] = index;
                cpu.vregs[0] = reference::INITIAL_V[0];
                cpu.vregs[31] = reference::INITIAL_V[3];
                let before = cpu.vregs;
                let x = cpu.x;
                let old = vector_bytes(cpu.vregs[usize::from(v.vector)]);
                let mut expected = old;
                let mut expected_memory = data.clone();
                let count = usize::from(v.bytes);
                let start = unaligned as usize;
                if v.load {
                    if v.replicate == 0 {
                        let lane = usize::from(v.lane) * count;
                        expected[lane..lane + count].copy_from_slice(&data[start..start + count]);
                    } else {
                        expected = [0; 16];
                        for slot in expected[..usize::from(v.replicate)].chunks_mut(count) {
                            slot.copy_from_slice(&data[start..start + count]);
                        }
                    }
                } else {
                    let lane = usize::from(v.lane) * count;
                    expected_memory[start..start + count].copy_from_slice(&old[lane..lane + count]);
                }
                {
                    let mut a = memory.as_ref().attach(&mut cpu);
                    assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0, "{w:x}");
                }
                let expected_vector = [
                    u64::from_le_bytes(expected[..8].try_into().unwrap()),
                    u64::from_le_bytes(expected[8..].try_into().unwrap()),
                ];
                for (reg, old) in before.iter().enumerate() {
                    assert_eq!(
                        cpu.vregs[reg],
                        if reg == usize::from(v.vector) && v.load {
                            expected_vector
                        } else {
                            *old
                        },
                        "{opt} {w:x} v{reg}"
                    );
                }
                let step = match v.writeback {
                    Writeback::None => 0,
                    Writeback::Immediate(n) => u64::from(n),
                    Writeback::Register(Reg::Gpr(n)) => x[usize::from(n)],
                    _ => unreachable!(),
                };
                assert_eq!(
                    cpu.x[1],
                    address.wrapping_add(if v.base == Reg::Gpr(1) { step } else { 0 })
                );
                assert_eq!(
                    cpu.sp,
                    address.wrapping_add(if v.base == Reg::Sp { step } else { 0 })
                );
                for (reg, value) in x.iter().enumerate() {
                    if reg != 1 {
                        assert_eq!(cpu.x[reg], *value);
                    }
                }
                assert_eq!(
                    (cpu.n, cpu.z, cpu.c, cpu.v, cpu.fpcr, cpu.fpsr),
                    (1, 0, 1, 1, 0x02c00000, 0x08000080)
                );
                let mut result = [0; 4];
                memory
                    .access(
                        MemoryOrder::Plain,
                        false,
                        GuestAddr(0x1000),
                        &mut result,
                        32,
                    )
                    .unwrap();
                assert_eq!(
                    result
                        .into_iter()
                        .flat_map(u64::to_le_bytes)
                        .collect::<Vec<_>>(),
                    expected_memory
                );
            }
            for fault in 0..3 {
                let mut cpu = Cpu {
                    sp: 0x1000,
                    ..Cpu::default()
                };
                cpu.x[1] = 0x1000;
                cpu.x[2] = 5;
                cpu.vregs[usize::from(v.vector)] = [123, 456];
                let before = cpu.vregs;
                if fault == 0 {
                    assert_eq!(unsafe { native(&mut cpu, 10) }, 3);
                } else {
                    let data = if fault == 1 {
                        vec![77; usize::from(v.bytes) - 1]
                    } else {
                        vec![77; 8]
                    };
                    // Empty mappings are disallowed; put byte faults just beyond a valid mapping.
                    let (base, data) = if data.is_empty() {
                        (0xfff, vec![77])
                    } else {
                        (0x1000, data)
                    };
                    let mapped_len = data.len();
                    let memory = bridge(
                        base,
                        data,
                        if fault == 2 {
                            Permissions::R
                        } else {
                            Permissions::RW
                        },
                    );
                    let mut a = memory.as_ref().attach(&mut cpu);
                    let rc = unsafe { native(a.cpu_mut(), 10) };
                    for offset in 0..mapped_len {
                        let mut byte = [0];
                        memory
                            .access(
                                MemoryOrder::Plain,
                                false,
                                GuestAddr(base + offset as u64),
                                &mut byte,
                                1,
                            )
                            .unwrap();
                        assert_eq!(byte[0], 77, "failed transfer changed memory {w:x}");
                    }
                    if fault == 2 && v.load {
                        assert_eq!(rc, 0);
                        continue;
                    } else {
                        assert_ne!(rc, 0, "{w:x} fault {fault}");
                    }
                }
                assert_eq!(cpu.vregs, before);
                assert_eq!((cpu.x[1], cpu.sp), (0x1000, 0x1000));
                assert_eq!(cpu.fault_address, 0x1000);
                assert_ne!(cpu.fault_status, 0);
            }
        }
    }
}
#[test]
fn structures_match_synthetic_qemu_memory_and_register_results() {
    let path = folder("structure-reference");
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
    let (elf, address) = reference::structure_elf(&words);
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, elf).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .current_dir(&path)
        .arg(&image)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout.len(), words.len() * 64);
    let code = codes(&words);
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for (i, w) in words.iter().enumerate() {
            let native = unsafe {
                lib.get::<Entry>(format!("structure_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let memory = bridge(address, (0..16).collect(), Permissions::RW);
            let mut cpu = Cpu {
                sp: address,
                n: 1,
                z: 0,
                c: 1,
                v: 1,
                ..Cpu::default()
            };
            cpu.x[0] = 9;
            cpu.x[1] = address;
            cpu.x[2] = 5;
            cpu.vregs[0] = reference::INITIAL_V[0];
            cpu.vregs[31] = reference::INITIAL_V[3];
            {
                let mut a = memory.as_ref().attach(&mut cpu);
                assert_eq!(unsafe { native(a.cpu_mut(), 10) }, 0);
            }
            let mut mem = [0; 2];
            memory
                .access(MemoryOrder::Plain, false, GuestAddr(address), &mut mem, 16)
                .unwrap();
            let base = if (w >> 5) & 31 == 31 {
                cpu.sp
            } else {
                cpu.x[1]
            };
            let actual: Vec<_> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                cpu.vregs[31][0],
                cpu.vregs[31][1],
                mem[0],
                mem[1],
                base - address,
                0xb0000000,
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(
                actual,
                &output.stdout[i * 64..(i + 1) * 64],
                "{opt} {w:08x}"
            );
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":words.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
