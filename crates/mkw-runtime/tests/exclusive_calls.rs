use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{OpsV2, Routed, Router, Target},
    exclusive_entry::Entry,
    exclusive_native::{Bridge, CpuV10},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-calls")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}
fn build(path: &Path, opt: &str, interference: bool, indirect: bool) -> libloading::Library {
    let caller = [
        0xaa1e03f3,
        0xc85ffc01,
        if indirect { 0xd63f00a0 } else { 0x94000ffe },
        0xc802fc03,
        0xaa1303fe,
        0xd65f03c0,
    ];
    let callee = if interference {
        vec![0xf9000004, 0xd65f03c0]
    } else {
        vec![0xd65f03c0]
    };
    let mut source = String::new();
    for (name, words) in [("caller", caller.to_vec()), ("callee", callee)] {
        let ir = mkw_arm64::lift_function_exclusive_v10(&bytes(&words), 0, 0).unwrap();
        source += &mkw_codegen::emit_exclusive_calls_v10(&ir, 0)
            .unwrap()
            .replace("mkw_guest_v10_budget", name);
    }
    let stub = mkw_arm64::lift_function_exclusive_v10(
        &bytes(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]),
        0,
        0,
    )
    .unwrap();
    source += &mkw_codegen::emit_exclusive_calls_v10(&stub, 0)
        .unwrap()
        .replace("mkw_guest_v10_budget", "import_stub");
    let ir = mkw_arm64::lift_exclusive_v10(&bytes(&[0xd65f03c0]), 0, 0).unwrap();
    source += &mkw_codegen::emit_exclusive_v10(&ir, 0)
        .unwrap()
        .replace("mkw_guest_v10_budget", "legacy");
    source+="size_t table_size(void){return sizeof(MkwExclusiveV2);}\nsize_t call_offset(void){return offsetof(MkwExclusiveV2,call);}\n";
    source += r#"
unsigned recurse_abi_version(void){return 10;} unsigned recurse_exclusive_version(void){return 2;}
int recurse(MkwCpuV10 *c,uint64_t *b){if(!mkw_calls_v2_valid(c)){return 3;}c->base.pc=0x4000;if(!*b){return 2;}--*b;return mkw_call_v10(c,0x4000,b);}
unsigned tail_cycle_abi_version(void){return 10;} unsigned tail_cycle_exclusive_version(void){return 2;}
int tail_cycle(MkwCpuV10 *c,uint64_t *b){if(!mkw_calls_v2_valid(c)){return 3;}c->base.pc=0x4000;if(!*b){return 2;}--*b;return MKW_EXIT_TAIL;}
unsigned bad_pc_abi_version(void){return 10;} unsigned bad_pc_exclusive_version(void){return 2;}
int bad_pc(MkwCpuV10 *c,uint64_t *b){if(!mkw_calls_v2_valid(c)){return 3;}if(!*b){return 2;}--*b;c->base.pc=0xbad;return 0;}
unsigned bad_tail_abi_version(void){return 10;} unsigned bad_tail_exclusive_version(void){return 2;}
int bad_tail(MkwCpuV10 *c,uint64_t *b){(void)b;c->base.pc=0x8800;return MKW_EXIT_TAIL;}
"#;
    let c = path.join("native.c");
    std::fs::write(&c, source).unwrap();
    let out = path.join(format!("native{}.so", &opt[1..]));
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
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    unsafe { libloading::Library::new(out).unwrap() }
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
fn cpu() -> CpuV10 {
    let mut c = CpuV10::default();
    c.base.x[0] = 0x1000;
    c.base.x[1] = 99;
    c.base.x[2] = 88;
    c.base.x[3] = 9;
    c.base.x[4] = 7;
    c.base.x[5] = 0x8000;
    c.base.x[30] = 0x9000;
    c.base.module_base = 0xdead;
    c.base.pc = 0xbeef;
    c
}
fn router<'a>(lib: &'a libloading::Library, callee: bool, depth: u16) -> Router<'a> {
    let mut entries =
        vec![unsafe { Target::load(lib, b"caller\0", GuestAddr(0x4000), 0x4000) }.unwrap()];
    if callee {
        entries.push(unsafe { Target::load(lib, b"callee\0", GuestAddr(0x8000), 0x8000) }.unwrap());
    }
    Router::new(entries, depth).unwrap()
}
#[test]
fn generated_v2_nested_calls_preserve_authority_budget_return_pc_and_module_base() {
    for interference in [false, true] {
        for indirect in [false, true] {
            let p = folder(&format!("semantic-{interference}-{indirect}"));
            for opt in ["-O0", "-O2"] {
                let lib = build(&p, opt, interference, indirect);
                let router = router(&lib, true, 8);
                for initial_budget in 0u64..10 {
                    let mem = memory();
                    mem.lock()
                        .unwrap()
                        .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                        .unwrap();
                    let mut bridge = Bridge::new(mem.clone()).unwrap();
                    let mut c = cpu();
                    let mut guard = bridge.attach(&mut c).unwrap();
                    let mut routed = Routed::attach(&mut guard, &router).unwrap();
                    let mut budget = initial_budget;
                    let total = if interference { 8 } else { 7 };
                    assert_eq!(
                        routed.run(GuestAddr(0x4000), &mut budget),
                        Ok(if initial_budget < total { 2 } else { 0 })
                    );
                    let c = &routed.cpu_mut().base;
                    assert_eq!(budget, initial_budget.saturating_sub(total));
                    assert_eq!(c.module_base, 0xdead);
                    assert_eq!(c.x[1], if initial_budget < 2 { 99 } else { 7 });
                    let store_at = if interference { 6 } else { 5 };
                    assert_eq!(
                        c.x[2],
                        if initial_budget < store_at {
                            88
                        } else {
                            u64::from(interference)
                        }
                    );
                    assert_eq!(
                        mem.lock()
                            .unwrap()
                            .read(GuestAddr(0x1000), Width::Double)
                            .unwrap(),
                        if !interference && initial_budget >= store_at {
                            9
                        } else {
                            7
                        }
                    );
                    let pcs = if interference {
                        vec![
                            0x4000, 0x4004, 0x4008, 0x8000, 0x8004, 0x400c, 0x4010, 0x4014,
                        ]
                    } else {
                        vec![0x4000, 0x4004, 0x4008, 0x8000, 0x400c, 0x4010, 0x4014]
                    };
                    assert_eq!(
                        c.pc,
                        if initial_budget < total {
                            pcs[initial_budget as usize]
                        } else {
                            0x9000
                        }
                    );
                    assert!(c.call.is_none() && c.call_context.is_null());
                }
            }
        }
    }
}
#[test]
fn v2_layout_profiles_depth_unknown_targets_and_attachment_tampering_refuse() {
    let p = folder("refusal");
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt, false, false);
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> usize>(b"table_size\0")
                    .unwrap()()
            },
            std::mem::size_of::<OpsV2>()
        );
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> usize>(b"call_offset\0")
                    .unwrap()()
            },
            std::mem::offset_of!(OpsV2, call)
        );
        assert!(unsafe { Target::load(&lib, b"legacy\0", GuestAddr(0x4000), 0x4000) }.is_err());
        assert!(Router::new(vec![], 0).is_err());
        assert!(Router::new(vec![], 129).is_err());
        for (callee, depth, expected) in [(false, 8, 3), (true, 1, 6)] {
            let router = router(&lib, callee, depth);
            let mut bridge = Bridge::new(memory()).unwrap();
            let mut c = cpu();
            let mut guard = bridge.attach(&mut c).unwrap();
            let mut routed = Routed::attach(&mut guard, &router).unwrap();
            let mut budget = 20;
            assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(expected));
            let c = &routed.cpu_mut().base;
            assert_eq!(
                (c.module_base, c.pc, c.x[30], budget),
                (0xdead, 0x4008, 0x400c, 17)
            );
            assert_eq!(c.fault_status, expected);
        }
        let router = router(&lib, true, 8);
        let mut bridge = Bridge::new(memory()).unwrap();
        let mut c = cpu();
        let mut guard = bridge.attach(&mut c).unwrap();
        let before = guard.cpu_mut().exclusive;
        {
            let mut routed = Routed::attach(&mut guard, &router).unwrap();
            let ops = routed.cpu_mut().exclusive;
            routed.cpu_mut().exclusive = std::ptr::NonNull::dangling().as_ptr();
            let mut budget = 8;
            assert!(routed.run(GuestAddr(0x4000), &mut budget).is_err());
            assert_eq!((routed.cpu_mut().base.pc, budget), (0xbeef, 8));
            routed.cpu_mut().exclusive = ops;
            let function = unsafe {
                lib.get::<unsafe extern "C" fn(*mut CpuV10, *mut u64) -> i32>(b"legacy\0")
                    .unwrap()
            };
            assert_eq!(unsafe { function(routed.cpu_mut(), &mut budget) }, 3);
            assert_eq!(budget, 8);
        }
        assert_eq!(guard.cpu_mut().exclusive, before);
        let legacy = unsafe { Entry::load(&lib, b"legacy\0", GuestAddr(0x4000), 0x4000) }.unwrap();
        let mut budget = 1;
        assert_eq!(legacy.run(&mut guard, &mut budget), Ok(0));
        assert_eq!(budget, 0);
    }
}
#[test]
fn v2_faults_bad_returns_tail_progress_foreign_frames_and_stale_authorities_refuse() {
    let p = folder("faults");
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt, true, false);
        for (symbol, budget_expected, pc) in [
            (b"bad_pc\0".as_slice(), 16, 0xbad),
            (b"bad_tail\0", 17, 0x8800),
        ] {
            let router = Router::new(
                vec![
                    unsafe { Target::load(&lib, b"caller\0", GuestAddr(0x4000), 0x4000) }.unwrap(),
                    unsafe { Target::load(&lib, symbol, GuestAddr(0x8000), 0x8000) }.unwrap(),
                ],
                8,
            )
            .unwrap();
            let mem = memory();
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut c = cpu();
            let mut guard = bridge.attach(&mut c).unwrap();
            let mut routed = Routed::attach(&mut guard, &router).unwrap();
            for _ in 0..2 {
                routed.cpu_mut().base.x[30] = 0x9000;
                let mut budget = 20;
                assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(6));
                let c = &routed.cpu_mut().base;
                assert_eq!(
                    (c.module_base, c.pc, c.x[2], budget),
                    (0xdead, pc, 88, budget_expected)
                );
                assert_eq!(c.fault_status, 6);
                assert_eq!(
                    mem.lock()
                        .unwrap()
                        .read(GuestAddr(0x1000), Width::Double)
                        .unwrap(),
                    0
                );
            }
        }
        let router = router(&lib, true, 8);
        let mut bridge = Bridge::new(memory()).unwrap();
        let mut c = cpu();
        c.base.x[0] = 0x2000;
        let mut guard = bridge.attach(&mut c).unwrap();
        let mut routed = Routed::attach(&mut guard, &router).unwrap();
        let mut budget = 20;
        assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(5));
        let c = &routed.cpu_mut().base;
        assert_eq!(
            (
                c.module_base,
                c.pc,
                c.fault_address,
                c.fault_status,
                c.x[2],
                budget
            ),
            (0xdead, 0x8000, 0x2000, 5, 88, 16)
        );
        drop(routed);
        drop(guard);
        drop(bridge);
        let mem = memory();
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut c = cpu();
        let mut guard = bridge.attach(&mut c).unwrap();
        let mut routed = Routed::attach(&mut guard, &router).unwrap();
        let table = unsafe { *(routed.cpu_mut().exclusive.cast::<OpsV2>()) };
        let mut foreign = cpu();
        let mut budget = 20;
        assert_eq!(
            unsafe { table.call.unwrap()(table.call_context, &mut foreign, 0x8000, &mut budget) },
            3
        );
        assert_eq!((foreign.base.pc, budget), (0xbeef, 20));
        assert_eq!(
            unsafe {
                table.call.unwrap()(std::ptr::null_mut(), routed.cpu_mut(), 0x8000, &mut budget)
            },
            7
        );
        let replacement = memory();
        std::mem::swap(&mut *mem.lock().unwrap(), &mut *replacement.lock().unwrap());
        assert!(routed.run(GuestAddr(0x4000), &mut budget).is_err());
        assert_eq!((routed.cpu_mut().base.pc, budget), (0xbeef, 20));
        std::mem::swap(&mut *mem.lock().unwrap(), &mut *replacement.lock().unwrap());
        let poison = mem.clone();
        assert!(std::thread::spawn(move || {
            let _guard = poison.lock().unwrap();
            panic!("intentional poison");
        })
        .join()
        .is_err());
        assert!(routed.run(GuestAddr(0x4000), &mut budget).is_err());
        assert_eq!((routed.cpu_mut().base.pc, budget), (0xbeef, 20));
    }
}

#[derive(Clone, Copy)]
struct QCase {
    initial: u64,
    desired: u64,
    interference: bool,
    indirect: bool,
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

fn qemu_image(cases: &[QCase]) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 20, 0);
    immediate(&mut code, 28, 0);
    let mut calls = Vec::new();
    for c in cases {
        immediate(&mut code, 21, c.initial);
        code.push(0xf9000000 | (20 << 5) | 21);
        immediate(&mut code, 3, c.desired);
        immediate(&mut code, 4, c.initial ^ 1);
        code.push(0xc85ffc00 | (20 << 5) | 23);
        let at = code.len();
        if c.indirect {
            immediate(&mut code, 5, 0);
            code.push(0xd63f00a0);
        } else {
            code.push(0x94000000);
        }
        calls.push((at, c.interference, c.indirect));
        code.push(0xc800fc00 | (24 << 16) | (20 << 5) | 3);
        code.push(0xf9400000 | (20 << 5) | 25);
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
    let plain = code.len();
    code.push(0xd65f03c0);
    let changed = code.len();
    code.push(0xf9000000 | (20 << 5) | 4);
    code.push(0xd65f03c0);
    for (at, interference, indirect) in calls {
        let target = if interference { changed } else { plain };
        if indirect {
            let mut patch = Vec::new();
            immediate(&mut patch, 5, BASE + TEXT as u64 + target as u64 * 4);
            code[at..at + 4].copy_from_slice(&patch);
        } else {
            code[at] = 0x94000000 | ((target as i64 - at as i64) as u32 & 0x03ffffff);
        }
    }
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
#[test]
fn nested_generated_v2_exclusive_sequences_match_qemu() {
    let p = folder("qemu");
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            p.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\"}",
        )
        .unwrap();
        return;
    }
    let mut cases = Vec::new();
    for interference in [false, true] {
        for indirect in [false, true] {
            for initial in [0, 1, 0xff, 0x8000, 0x123456789abcdef0, u64::MAX] {
                for desired in [0, 1, 0xabcdef0123456789, u64::MAX] {
                    cases.push(QCase {
                        initial,
                        desired,
                        interference,
                        indirect,
                    });
                }
            }
        }
    }
    let elf = p.join("synthetic-aarch64");
    std::fs::write(&elf, qemu_image(&cases)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o700)).unwrap();
    let reference = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(elf)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(reference.stdout.len(), cases.len() * 24);
    for interference in [false, true] {
        for indirect in [false, true] {
            let folder = p.join(format!("{interference}-{indirect}"));
            std::fs::create_dir_all(&folder).unwrap();
            for opt in ["-O0", "-O2"] {
                let lib = build(&folder, opt, interference, indirect);
                let router = router(&lib, true, 8);
                let mem = memory();
                let mut bridge = Bridge::new(mem.clone()).unwrap();
                for (i, c) in cases
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.interference == interference && c.indirect == indirect)
                {
                    mem.lock()
                        .unwrap()
                        .write(GuestAddr(0x1000), &c.initial.to_le_bytes())
                        .unwrap();
                    let mut frame = cpu();
                    frame.base.x[3] = c.desired;
                    frame.base.x[4] = c.initial ^ 1;
                    let mut guard = bridge.attach(&mut frame).unwrap();
                    let mut routed = Routed::attach(&mut guard, &router).unwrap();
                    let mut budget = 20;
                    assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(0));
                    let state = &routed.cpu_mut().base;
                    let values = [
                        state.x[1],
                        state.x[2],
                        mem.lock()
                            .unwrap()
                            .read(GuestAddr(0x1000), Width::Double)
                            .unwrap() as u64,
                    ];
                    assert_eq!(
                        values,
                        [
                            c.initial,
                            u64::from(interference),
                            if interference {
                                c.initial ^ 1
                            } else {
                                c.desired
                            }
                        ]
                    );
                    for (k, value) in values.iter().enumerate() {
                        assert_eq!(
                            *value,
                            u64::from_le_bytes(
                                reference.stdout[i * 24 + k * 8..i * 24 + k * 8 + 8]
                                    .try_into()
                                    .unwrap()
                            )
                        );
                    }
                }
            }
        }
    }
    std::fs::write(p.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"qemu_cases":cases.len(),"native_cases_per_optimization":cases.len(),"optimizations":["O0","O2"],"fatal_ubsan":true,"direct_and_indirect_nested_calls":true,"same_value_interference_compared_to_qemu":false,"game_code_executed":false})).unwrap()).unwrap();
}
fn import_memory(got: Option<u64>) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(GuestAddr(0x1000), vec![0; 64], Permissions::RW, "data")
        .unwrap();
    if let Some(target) = got {
        let mut bytes = vec![0; 32];
        bytes[24..].copy_from_slice(&target.to_le_bytes());
        m.map(GuestAddr(0xa000), bytes, Permissions::R, "synthetic-GOT")
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
#[test]
fn synthetic_import_thunk_preserves_register_effects_coherence_and_tail_budgets() {
    let p = folder("import-thunk");
    for interference in [false, true] {
        for opt in ["-O0", "-O2"] {
            let lib = build(&p, opt, interference, false);
            let router = Router::new(
                vec![
                    unsafe { Target::load(&lib, b"caller\0", GuestAddr(0x4000), 0x4000) }.unwrap(),
                    unsafe { Target::load(&lib, b"import_stub\0", GuestAddr(0x8000), 0x8000) }
                        .unwrap(),
                    unsafe { Target::load(&lib, b"callee\0", GuestAddr(0x9000), 0x9000) }.unwrap(),
                ],
                2,
            )
            .unwrap();
            for initial_budget in 0u64..14 {
                let mem = import_memory(Some(0x9000));
                mem.lock()
                    .unwrap()
                    .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                    .unwrap();
                let mut bridge = Bridge::new(mem.clone()).unwrap();
                let mut c = cpu();
                c.base.x[30] = 0xb000;
                let mut guard = bridge.attach(&mut c).unwrap();
                let mut routed = Routed::attach(&mut guard, &router).unwrap();
                let mut budget = initial_budget;
                let total = if interference { 12 } else { 11 };
                assert_eq!(
                    routed.run(GuestAddr(0x4000), &mut budget),
                    Ok(if initial_budget < total { 2 } else { 0 })
                );
                let c = &routed.cpu_mut().base;
                assert_eq!(budget, initial_budget.saturating_sub(total));
                assert_eq!(c.module_base, 0xdead);
                assert_eq!(
                    c.x[16],
                    if initial_budget < 4 {
                        0
                    } else if initial_budget < 6 {
                        0xa000
                    } else {
                        0xa018
                    }
                );
                assert_eq!(c.x[17], if initial_budget < 5 { 0 } else { 0x9000 });
                let pcs = if interference {
                    vec![
                        0x4000, 0x4004, 0x4008, 0x8000, 0x8004, 0x8008, 0x800c, 0x9000, 0x9004,
                        0x400c, 0x4010, 0x4014,
                    ]
                } else {
                    vec![
                        0x4000, 0x4004, 0x4008, 0x8000, 0x8004, 0x8008, 0x800c, 0x9000, 0x400c,
                        0x4010, 0x4014,
                    ]
                };
                assert_eq!(
                    c.pc,
                    if initial_budget < total {
                        pcs[initial_budget as usize]
                    } else {
                        0xb000
                    }
                );
                let store_at = if interference { 10 } else { 9 };
                assert_eq!(
                    c.x[2],
                    if initial_budget < store_at {
                        88
                    } else {
                        u64::from(interference)
                    }
                );
                assert_eq!(
                    mem.lock()
                        .unwrap()
                        .read(GuestAddr(0x1000), Width::Double)
                        .unwrap(),
                    if !interference && initial_budget >= store_at {
                        9
                    } else {
                        7
                    }
                );
            }
            for (target, status, pc, address, budget_expected) in [
                (None, 5, 0x8004, 0xa018, 15),
                (Some(0x9999), 3, 0x9999, 0x9999, 13),
            ] {
                let mut bridge = Bridge::new(import_memory(target)).unwrap();
                let mut c = cpu();
                c.base.x[17] = 77;
                let mut guard = bridge.attach(&mut c).unwrap();
                let mut routed = Routed::attach(&mut guard, &router).unwrap();
                let mut budget = 20;
                assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(status));
                let c = &routed.cpu_mut().base;
                assert_eq!(
                    (
                        c.module_base,
                        c.pc,
                        c.fault_address,
                        c.fault_status,
                        c.x[2],
                        budget
                    ),
                    (0xdead, pc, address, status, 88, budget_expected)
                );
                assert_eq!(c.x[17], if target.is_none() { 77 } else { 0x9999 });
            }
        }
    }
}
#[test]
fn native_v2_depth_limits_reset_and_progressing_tail_cycles_are_bounded() {
    let p = folder("depth");
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt, false, false);
        for depth in [1, 2, 8, 128] {
            let router = Router::new(
                vec![
                    unsafe { Target::load(&lib, b"recurse\0", GuestAddr(0x4000), 0x4000) }.unwrap(),
                ],
                depth,
            )
            .unwrap();
            let mut bridge = Bridge::new(memory()).unwrap();
            let mut c = cpu();
            let mut guard = bridge.attach(&mut c).unwrap();
            let mut routed = Routed::attach(&mut guard, &router).unwrap();
            for _ in 0..2 {
                let mut budget = 256;
                assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(6));
                let c = &routed.cpu_mut().base;
                assert_eq!(
                    (budget, c.pc, c.module_base, c.fault_status),
                    (256 - u64::from(depth), 0x4000, 0xdead, 6)
                );
            }
            for initial in 0..u64::from(depth) {
                let mut budget = initial;
                assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(2));
                assert_eq!(budget, 0);
            }
        }
        let router = Router::new(
            vec![
                unsafe { Target::load(&lib, b"tail_cycle\0", GuestAddr(0x4000), 0x4000) }.unwrap(),
            ],
            1,
        )
        .unwrap();
        let mut bridge = Bridge::new(memory()).unwrap();
        let mut c = cpu();
        let mut guard = bridge.attach(&mut c).unwrap();
        let mut routed = Routed::attach(&mut guard, &router).unwrap();
        let mut budget = 1024;
        assert_eq!(routed.run(GuestAddr(0x4000), &mut budget), Ok(2));
        assert_eq!(
            (
                budget,
                routed.cpu_mut().base.pc,
                routed.cpu_mut().base.module_base
            ),
            (0, 0x4000, 0xdead)
        );
    }
}
