use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    GuestAddr, Memory, Permissions,
};
use mkw_runtime::{
    exclusive_entry::Entry,
    exclusive_native::{Bridge, CpuV10},
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
#[derive(Clone, Copy)]
struct Case {
    size: u32,
    ordered: bool,
    interference: u32,
    initial: u64,
    desired: u64,
}
fn immediate(code: &mut Vec<u32>, reg: u32, value: u64) {
    for part in 0..4 {
        code.push(
            (if part == 0 { 0xd2800000 } else { 0xf2800000 })
                | (part << 21)
                | ((((value >> (part * 16)) & 65535) as u32) << 5)
                | reg,
        );
    }
}
fn image(cases: &[Case]) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 20, 0);
    immediate(&mut code, 28, 0);
    for c in cases {
        let store = 0x39000000 | (c.size << 30);
        let load = 0x39400000 | (c.size << 30);
        immediate(&mut code, 21, c.initial);
        code.push(store | (20 << 5) | 21);
        let order = if c.ordered { 0x8000 } else { 0 };
        code.push(0x085f7c00 | (c.size << 30) | order | (20 << 5) | 23);
        match c.interference {
            0 => {}
            1 => code.push(0xd5033f5f),
            2 => {
                immediate(&mut code, 21, c.initial ^ 1);
                code.push(store | (20 << 5) | 21);
            }
            _ => panic!(),
        }
        immediate(&mut code, 21, c.desired);
        code.push(0x08007c00 | (c.size << 30) | order | (24 << 16) | (20 << 5) | 21);
        code.push(load | (20 << 5) | 25);
        code.push(0xf9000397);
        code.push(0xf9000798);
        code.push(0xf9000b99);
        code.push(0x9100639c);
    }
    let write_at = code.len();
    immediate(&mut code, 0, 1);
    immediate(&mut code, 1, 0);
    immediate(&mut code, 2, cases.len() as u64 * 24);
    immediate(&mut code, 8, 64);
    code.push(0xd4000001);
    immediate(&mut code, 0, 0);
    immediate(&mut code, 8, 93);
    code.push(0xd4000001);
    let data = (TEXT + code.len() * 4 + 15) & !15;
    let output = data + 16;
    let mut patch = Vec::new();
    immediate(&mut patch, 20, BASE + data as u64);
    code[..4].copy_from_slice(&patch);
    patch.clear();
    immediate(&mut patch, 28, BASE + output as u64);
    code[4..8].copy_from_slice(&patch);
    patch.clear();
    immediate(&mut patch, 1, BASE + output as u64);
    code[write_at + 4..write_at + 8].copy_from_slice(&patch);
    let mut bytes = vec![0u8; output + cases.len() * 24];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    let p16 = |b: &mut [u8], at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let p64 = |b: &mut [u8], at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    p16(&mut bytes, 16, 2);
    p16(&mut bytes, 18, 183);
    p32(&mut bytes, 20, 1);
    p64(&mut bytes, 24, BASE + TEXT as u64);
    p64(&mut bytes, 32, 64);
    p16(&mut bytes, 52, 64);
    p16(&mut bytes, 54, 56);
    p16(&mut bytes, 56, 1);
    p32(&mut bytes, 64, 1);
    p32(&mut bytes, 68, 7);
    p64(&mut bytes, 80, BASE);
    p64(&mut bytes, 88, BASE);
    let length = bytes.len() as u64;
    p64(&mut bytes, 96, length);
    p64(&mut bytes, 104, length);
    p64(&mut bytes, 112, 4096);
    for (i, w) in code.into_iter().enumerate() {
        p32(&mut bytes, TEXT + i * 4, w);
    }
    bytes
}

fn words(size: u32, ordered: bool, interference: u32) -> Vec<u32> {
    let order = if ordered { 1 << 15 } else { 0 };
    let mut w = vec![0x085f7c00 | (size << 30) | order | (20 << 5) | 23];
    match interference {
        0 => {}
        1 => w.push(0xd5033f5f),
        2 => w.push(0x39000000 | (size << 30) | (20 << 5) | 22),
        _ => panic!(),
    }
    w.push(0x08007c00 | (size << 30) | order | (24 << 16) | (20 << 5) | 21);
    w.push(0x39400000 | (size << 30) | (20 << 5) | 25);
    w.push(0xd65f03c0);
    w
}
fn bytes(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn name(size: u32, order: bool, interference: u32) -> String {
    format!("case_{size}_{}_{}", u32::from(order), interference)
}
fn build(path: &Path, opt: &str) -> libloading::Library {
    std::fs::create_dir_all(path).unwrap();
    let mut source = String::new();
    for size in 0..4 {
        for order in [false, true] {
            for interference in 0..3 {
                let program = bytes(&words(size, order, interference));
                assert!(mkw_arm64::lift(&program, 0, 0).is_err());
                let ir = mkw_arm64::lift_exclusive_v10(&program, 0, 0).unwrap();
                assert!(mkw_codegen::emit(&ir, 0).is_err());
                source += &mkw_codegen::emit_exclusive_v10(&ir, 0)
                    .unwrap()
                    .replace("mkw_guest_v10_budget", &name(size, order, interference));
            }
        }
    }
    let c = path.join("generated.c");
    std::fs::write(&c, source).unwrap();
    let lib = path.join(format!("native{}.so", &opt[1..]));
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
            "-I",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(c)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    unsafe { libloading::Library::new(lib).unwrap() }
}
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(GuestAddr(0x1000), vec![0; 64], Permissions::RW, "rw")
        .unwrap();
    m.map(GuestAddr(0x2000), vec![0; 16], Permissions::R, "ro")
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
#[test]
fn generated_single_exclusives_match_qemu_and_independent_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-lifting/semantics");
    std::fs::create_dir_all(&root).unwrap();
    let mut cases = Vec::new();
    for size in 0..4 {
        for ordered in [false, true] {
            for interference in 0..3 {
                for initial in [0, 1, 0xff, 0x8000, 0x123456789abcdef0, u64::MAX] {
                    for desired in [0, 1, 0xabcdef0123456789, u64::MAX] {
                        cases.push(Case {
                            size,
                            ordered,
                            interference,
                            initial,
                            desired,
                        });
                    }
                }
            }
        }
    }
    let reference = if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_ok()
    {
        let elf = root.join("synthetic-aarch64");
        std::fs::write(&elf, image(&cases)).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = Command::new("qemu-aarch64")
            .args(["-cpu", "max"])
            .arg(elf)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), cases.len() * 24);
        Some(output.stdout)
    } else {
        None
    };
    for opt in ["-O0", "-O2"] {
        let lib = build(&root, opt);
        let mem = memory();
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        for (i, c) in cases.iter().enumerate() {
            let width = 1u64 << c.size;
            let mask = if width == 8 {
                u64::MAX
            } else {
                (1u64 << (width * 8)) - 1
            };
            mem.lock()
                .unwrap()
                .write(GuestAddr(0x1000), &c.initial.to_le_bytes())
                .unwrap();
            let mut frame = CpuV10::default();
            frame.base.x = std::array::from_fn(|i| 0x1234000000000000 + i as u64);
            frame.base.x[20] = 0x1000;
            frame.base.x[21] = c.desired;
            frame.base.x[22] = c.initial ^ 1;
            frame.base.x[30] = 0x9000;
            let before = frame.base.x;
            let mut guard = bridge.attach(&mut frame).unwrap();
            let entry = unsafe {
                Entry::load(
                    &lib,
                    name(c.size, c.ordered, c.interference).as_bytes(),
                    GuestAddr(0x4000),
                    0x4000,
                )
            }
            .unwrap();
            let mut budget = 20;
            assert_eq!(entry.run(&mut guard, &mut budget), Ok(0));
            let state = &guard.cpu_mut().base;
            let values = [state.x[23], state.x[24], state.x[25]];
            let expected = [
                c.initial & mask,
                u64::from(c.interference != 0),
                (if c.interference == 0 {
                    c.desired
                } else if c.interference == 2 {
                    c.initial ^ 1
                } else {
                    c.initial
                }) & mask,
            ];
            assert_eq!(values, expected, "case {i} {opt}");
            if let Some(ref output) = reference {
                for (k, value) in values.iter().enumerate() {
                    assert_eq!(
                        *value,
                        u64::from_le_bytes(
                            output[i * 24 + k * 8..i * 24 + k * 8 + 8]
                                .try_into()
                                .unwrap()
                        )
                    );
                }
            }
            assert_eq!(
                budget,
                20 - words(c.size, c.ordered, c.interference).len() as u64
            );
            assert_eq!(state.pc, 0x9000);
            assert_eq!(state.module_base, 0x4000);
            for (reg, prior) in before.iter().enumerate() {
                if ![23, 24, 25].contains(&reg) {
                    assert_eq!(state.x[reg], *prior);
                }
            }
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x1000), Width::try_from(width as usize).unwrap())
                    .unwrap(),
                u128::from(expected[2])
            );
        }
    }
    std::fs::write(root.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":cases.len(),"optimizations":["O0","O2"],"fatal_ubsan":true,"qemu_available":reference.is_some(),"default_acceptance_changed":false,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn generated_exclusive_budgets_faults_and_monitor_lifetime_are_exact() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-lifting/budgets");
    for opt in ["-O0", "-O2"] {
        let lib = build(&root, opt);
        for size in 0..4 {
            for ordered in [false, true] {
                for interference in 0..3 {
                    let w = words(size, ordered, interference);
                    let entry = unsafe {
                        Entry::load(
                            &lib,
                            name(size, ordered, interference).as_bytes(),
                            GuestAddr(0x4000),
                            0x4000,
                        )
                    }
                    .unwrap();
                    for budget_value in 0..w.len() as u64 + 2 {
                        let mem = memory();
                        mem.lock()
                            .unwrap()
                            .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                            .unwrap();
                        let mut bridge = Bridge::new(mem.clone()).unwrap();
                        let mut frame = CpuV10::default();
                        frame.base.x[20] = 0x1000;
                        frame.base.x[21] = 9;
                        frame.base.x[22] = 6;
                        frame.base.x[23] = 99;
                        frame.base.x[24] = 88;
                        frame.base.x[25] = 77;
                        frame.base.x[30] = 0x9000;
                        let mut guard = bridge.attach(&mut frame).unwrap();
                        let mut budget = budget_value;
                        assert_eq!(
                            entry.run(&mut guard, &mut budget),
                            Ok(if budget_value < w.len() as u64 { 2 } else { 0 })
                        );
                        let c = &guard.cpu_mut().base;
                        let store_position = if interference == 0 { 1 } else { 2 };
                        assert_eq!(budget, budget_value.saturating_sub(w.len() as u64));
                        assert_eq!(
                            c.pc,
                            if budget_value < w.len() as u64 {
                                0x4000 + 4 * budget_value
                            } else {
                                0x9000
                            }
                        );
                        assert_eq!(c.x[23], if budget_value == 0 { 99 } else { 7 });
                        assert_eq!(
                            c.x[24],
                            if budget_value <= store_position {
                                88
                            } else {
                                u64::from(interference != 0)
                            }
                        );
                        let expected = if interference == 0 && budget_value > store_position {
                            9
                        } else if interference == 2 && budget_value > 1 {
                            6
                        } else {
                            7
                        };
                        assert_eq!(
                            mem.lock()
                                .unwrap()
                                .read(GuestAddr(0x1000), Width::try_from(1usize << size).unwrap())
                                .unwrap(),
                            expected
                        );
                        assert_eq!(
                            c.x[25],
                            if budget_value > store_position + 1 {
                                expected as u64
                            } else {
                                77
                            }
                        );
                        // Consuming the host budget alone never fabricates a completed return.
                    }
                    for address in [0x2000, 0x3000, 0x1001] {
                        if size == 0 && address == 0x1001 {
                            continue;
                        }
                        let mem = memory();
                        let mut bridge = Bridge::new(mem).unwrap();
                        let mut frame = CpuV10::default();
                        frame.base.x[20] = address;
                        frame.base.x[21] = 9;
                        frame.base.x[22] = 6;
                        frame.base.x[23] = 99;
                        frame.base.x[24] = 88;
                        let mut guard = bridge.attach(&mut frame).unwrap();
                        let mut budget = 20;
                        assert_eq!(
                            entry.run(&mut guard, &mut budget),
                            Ok(if address == 0x2000 && interference == 1 {
                                0
                            } else if address == 0x2000 && interference == 2 {
                                5
                            } else {
                                1
                            })
                        );
                        let c = &guard.cpu_mut().base;
                        if address == 0x2000 && interference == 1 {
                            assert_eq!(c.x[24], 1);
                            continue;
                        }
                        let failed_at = if address == 0x2000 { 1 } else { 0 };
                        assert_eq!(c.pc, 0x4000 + failed_at * 4);
                        assert_eq!(c.fault_address, address);
                        assert_eq!(
                            c.fault_status,
                            if address == 0x2000 && interference == 2 {
                                5
                            } else {
                                1
                            }
                        );
                        assert_eq!(budget, 20 - failed_at - 1);
                        assert_eq!(c.x[24], 88);
                    }
                }
            }
        }
    }
}
#[test]
fn generated_single_exclusive_register31_sp_and_split_entries_are_exact() {
    use mkw_arm64::{exclusive::Exclusive, Reg};
    let invalid = [
        Exclusive::Clear { immediate: 16 },
        Exclusive::Load {
            bytes: 16,
            acquire: false,
            base: Reg::Gpr(0),
            dst: Reg::Gpr(1),
        },
        Exclusive::Load {
            bytes: 8,
            acquire: false,
            base: Reg::Zero,
            dst: Reg::Gpr(1),
        },
        Exclusive::Load {
            bytes: 8,
            acquire: false,
            base: Reg::Sp,
            dst: Reg::Gpr(31),
        },
        Exclusive::Store {
            bytes: 8,
            release: false,
            base: Reg::Gpr(0),
            src: Reg::Gpr(1),
            status: Reg::Gpr(1),
        },
        Exclusive::Store {
            bytes: 8,
            release: false,
            base: Reg::Gpr(0),
            src: Reg::Gpr(1),
            status: Reg::Gpr(0),
        },
        Exclusive::Store {
            bytes: 8,
            release: false,
            base: Reg::Sp,
            src: Reg::Zero,
            status: Reg::Zero,
        },
    ];
    for op in invalid {
        let mut ir =
            mkw_arm64::lift_exclusive_v10(&bytes(&[0xd5033f5f, 0xd65f03c0]), 0, 0).unwrap();
        ir[0].op = mkw_arm64::Op::Exclusive(op);
        assert!(mkw_codegen::emit_exclusive_v10(&ir, 0).is_err());
        assert!(mkw_codegen::emit(&ir, 0).is_err());
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-lifting/aliases");
    std::fs::create_dir_all(&root).unwrap();
    let patterns = [
        (31, 1, 3, 2),
        (0, 31, 3, 2),
        (0, 1, 31, 2),
        (0, 1, 3, 31),
        (0, 0, 3, 2),
        (0, 1, 0, 2),
    ];
    let mut source = String::new();
    for size in 0..4 {
        for order in 0..2 {
            for (p, &(n, t, src, s)) in patterns.iter().enumerate() {
                for (store, word) in [
                    (
                        false,
                        0x085f7c00 | (size << 30) | (order << 15) | (n << 5) | t,
                    ),
                    (
                        true,
                        0x08007c00 | (size << 30) | (order << 15) | (s << 16) | (n << 5) | src,
                    ),
                ] {
                    let ir =
                        mkw_arm64::lift_exclusive_v10(&bytes(&[word, 0xd65f03c0]), 0, 0).unwrap();
                    source += &mkw_codegen::emit_exclusive_v10(&ir, 0).unwrap().replace(
                        "mkw_guest_v10_budget",
                        &format!("alias_{size}_{order}_{p}_{}", u32::from(store)),
                    );
                }
            }
        }
    }
    let c = root.join("generated.c");
    std::fs::write(&c, source).unwrap();
    for opt in ["-O0", "-O2"] {
        let output = root.join(format!("native{}.so", &opt[1..]));
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
                "-I",
            ])
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
            .arg(&c)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let lib = unsafe { libloading::Library::new(output).unwrap() };
        for size in 0..4 {
            for order in 0..2 {
                for (p, &(n, t, src, s)) in patterns.iter().enumerate() {
                    let mem = memory();
                    mem.lock()
                        .unwrap()
                        .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                        .unwrap();
                    let mut bridge = Bridge::new(mem.clone()).unwrap();
                    let mut frame = CpuV10::default();
                    frame.base.x[0] = 0x1000;
                    frame.base.sp = 0x1000;
                    frame.base.x[1] = 99;
                    frame.base.x[2] = 88;
                    frame.base.x[3] = 9;
                    frame.base.x[30] = 0x9000;
                    let mut guard = bridge.attach(&mut frame).unwrap();
                    let load = unsafe {
                        Entry::load(
                            &lib,
                            format!("alias_{size}_{order}_{p}_0").as_bytes(),
                            GuestAddr(0x4000),
                            0x4000,
                        )
                    }
                    .unwrap();
                    let store = unsafe {
                        Entry::load(
                            &lib,
                            format!("alias_{size}_{order}_{p}_1").as_bytes(),
                            GuestAddr(0x4000),
                            0x4000,
                        )
                    }
                    .unwrap();
                    let mut budget = 4;
                    assert_eq!(load.run(&mut guard, &mut budget), Ok(0));
                    assert_eq!(budget, 2);
                    if t != 31 {
                        assert_eq!(guard.cpu_mut().base.x[t as usize], 7);
                    } else {
                        assert_eq!(guard.cpu_mut().base.x[1], 99);
                    }
                    if n != 31 {
                        guard.cpu_mut().base.x[n as usize] = 0x1000;
                    }
                    assert_eq!(store.run(&mut guard, &mut budget), Ok(0));
                    assert_eq!(budget, 0);
                    let c = &guard.cpu_mut().base;
                    assert_eq!(c.pc, 0x9000);
                    if s != 31 {
                        assert_eq!(c.x[s as usize], 0);
                    } else {
                        assert_eq!(c.x[2], 88);
                    }
                    let mask = if size == 3 {
                        u64::MAX
                    } else {
                        (1u64 << (8 << size)) - 1
                    };
                    assert_eq!(
                        mem.lock()
                            .unwrap()
                            .read(GuestAddr(0x1000), Width::try_from(1usize << size).unwrap())
                            .unwrap(),
                        u128::from(
                            (if src == 31 {
                                0
                            } else if src == 0 {
                                0x1000
                            } else {
                                9
                            }) & mask
                        )
                    );
                    if n == 31 {
                        guard.cpu_mut().base.sp = 0x1008;
                        let mut budget = 2;
                        assert_eq!(load.run(&mut guard, &mut budget), Ok(1));
                        let c = &guard.cpu_mut().base;
                        assert_eq!(
                            (c.pc, c.fault_status, c.fault_address, budget),
                            (0x4000, 1, 0x1008, 1)
                        );
                    }
                }
            }
        }
    }
}
