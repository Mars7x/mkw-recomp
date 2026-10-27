use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_entry::Entry,
    exclusive_native::{Bridge, CpuV10, Ops},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
fn folder(label: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-entry")
        .join(label);
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn build(path: &Path, opt: &str, version: u32) -> libloading::Library {
    std::fs::create_dir_all(path).unwrap();
    let c = path.join("entry.c");
    let code = format!("#define VERSION {version}u\n")
        + r#"
#include "exclusive_v10.h"
size_t frame_size(void){return sizeof(MkwCpuV10);}
size_t ops_size(void){return sizeof(MkwExclusiveV1);}
static unsigned calls;
unsigned entry_calls(void){return calls;}
unsigned step_abi_version(void){return VERSION;}
int step(MkwCpuV10 *cpu,uint64_t *budget){
 ++calls;if(!mkw_exclusive_v10_valid(cpu))return 3;MkwCpu *base=&cpu->base;
 base->pc=base->module_base+0x20;if(!*budget){return 2;}--*budget;
 uint64_t data[2]={0,0};base->fault_address=base->x[0];
 int status=cpu->exclusive->load(cpu->exclusive_context,base->x[0],8,1,data,2);
 if(status){base->fault_status=status;return status;}base->x[1]=data[0];data[0]++;
 base->pc=base->module_base+0x24;if(!*budget){return 2;}--*budget;
 uint32_t stored=99;status=cpu->exclusive->store(cpu->exclusive_context,base->x[0],8,1,data,2,&stored);
 if(status){base->fault_status=status;return status;}base->x[2]=1-stored;
 base->pc=base->module_base+0x28;if(!*budget){return 2;}--*budget;
 base->pc=base->x[30];return 0;
}
unsigned tail_abi_version(void){return VERSION;}
int tail(MkwCpuV10 *cpu,uint64_t *budget){(void)budget;cpu->base.pc=0x8800;return MKW_EXIT_TAIL;}
unsigned chain_abi_version(void){return VERSION;}
int chain(MkwCpuV10 *cpu,uint64_t *budget){
 ++calls;if(!mkw_exclusive_v10_valid(cpu))return 3;MkwCpu *c=&cpu->base;
 if(!*budget){return 2;}--*budget;uint64_t data[2]={0,0};c->fault_address=c->x[0];
 int status=cpu->exclusive->load(cpu->exclusive_context,c->x[0],8,1,data,2);
 if(status){c->fault_status=status;return status;}c->x[1]=data[0];c->pc=0x8040;return MKW_EXIT_TAIL;
}
unsigned finish_abi_version(void){return VERSION;}
int finish(MkwCpuV10 *cpu,uint64_t *budget){
 ++calls;if(!mkw_exclusive_v10_valid(cpu))return 3;MkwCpu *c=&cpu->base;
 if(!*budget){return 2;}--*budget;uint64_t data[2]={c->x[1]+1,0};uint32_t stored=99;c->fault_address=c->x[0];
 int status=cpu->exclusive->store(cpu->exclusive_context,c->x[0],8,1,data,2,&stored);
 if(status){c->fault_status=status;return status;}c->x[2]=1-stored;c->pc=c->x[30];return 0;
}
unsigned missing_tail_abi_version(void){return VERSION;}
int missing_tail(MkwCpuV10 *cpu,uint64_t *budget){if(!*budget){return 2;}--*budget;cpu->base.pc=0x8800;return MKW_EXIT_TAIL;}
int missing_marker(MkwCpuV10 *cpu,uint64_t *budget){(void)cpu;(void)budget;return 3;}
"#;
    std::fs::write(&c, code).unwrap();
    let output = path.join(format!("native{}.so", &opt[1..]));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native");
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
        .arg(root)
        .arg(c)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // SAFETY: original synthetic source compiled above, with C/Rust layout checks.
    unsafe {
        let library = libloading::Library::new(output).unwrap();
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"frame_size\0")
                .unwrap()(),
            std::mem::size_of::<CpuV10>()
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"ops_size\0")
                .unwrap()(),
            std::mem::size_of::<Ops>()
        );
        library
    }
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
    c.base.x[30] = 0x9000;
    c.base.module_base = 0xdead;
    c.base.pc = 0xbeef;
    c
}
#[test]
fn checked_v10_entry_budgets_pc_faults_and_tail_status() {
    let p = folder("semantics");
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt, 10);
        let entry = unsafe { Entry::load(&lib, b"step\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        for budget_value in 0..5 {
            let mem = memory();
            mem.lock()
                .unwrap()
                .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                .unwrap();
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut frame = cpu();
            let mut guard = bridge.attach(&mut frame).unwrap();
            let mut budget = budget_value;
            assert_eq!(
                entry.run(&mut guard, &mut budget),
                Ok(if budget_value < 3 { 2 } else { 0 })
            );
            let c = guard.cpu_mut();
            assert_eq!(budget, budget_value.saturating_sub(3));
            assert_eq!(c.base.module_base, 0x4000);
            assert_eq!(
                c.base.pc,
                match budget_value {
                    0 => 0x4020,
                    1 => 0x4024,
                    2 => 0x4028,
                    _ => 0x9000,
                }
            );
            assert_eq!(c.base.x[1], if budget_value == 0 { 99 } else { 7 });
            assert_eq!(c.base.x[2], if budget_value < 2 { 88 } else { 0 });
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x1000), Width::Double)
                    .unwrap(),
                if budget_value < 2 { 7 } else { 8 }
            );
        }
        for address in [0x2000, 0x3000, 0x1001] {
            let mem = memory();
            let mut bridge = Bridge::new(mem).unwrap();
            let mut frame = cpu();
            frame.base.x[0] = address;
            let mut guard = bridge.attach(&mut frame).unwrap();
            let mut budget = 3;
            assert_eq!(entry.run(&mut guard, &mut budget), Ok(1));
            let c = guard.cpu_mut();
            assert_eq!(c.base.fault_address, address);
            assert_eq!(c.base.fault_status, 1);
            assert_eq!(c.base.pc, if address == 0x2000 { 0x4024 } else { 0x4020 });
            assert_eq!(budget, if address == 0x2000 { 1 } else { 2 });
            assert_eq!(c.base.x[2], 88);
        }
        let tail = unsafe { Entry::load(&lib, b"tail\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        let mut bridge = Bridge::new(memory()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let mut budget = 4;
        assert_eq!(
            tail.run(&mut guard, &mut budget),
            Ok(mkw_runtime::abi::EXIT_TAIL)
        );
        assert_eq!(budget, 4);
        assert_eq!(guard.cpu_mut().base.pc, 0x8800);
    }
}
#[test]
fn checked_v10_entry_refuses_wrong_versions_names_and_addresses() {
    for version in [0, 9, 11] {
        let p = folder(&format!("version{version}"));
        let lib = build(&p, "-O2", version);
        assert!(
            matches!(unsafe{Entry::load(&lib,b"step\0",GuestAddr(0x4020),0x4000)},Err(e)if e.contains("ABI version mismatch"))
        );
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> u32>(b"entry_calls\0")
                    .unwrap()()
            },
            0
        );
    }
    let p = folder("names");
    let lib = build(&p, "-O2", 10);
    for name in [
        b"".as_slice(),
        b"step\0bad",
        b"missing_marker\0",
        b"nonexistent\0",
    ] {
        assert!(unsafe { Entry::load(&lib, name, GuestAddr(0x4020), 0x4000) }.is_err());
    }
    for (address, base) in [(0x4021, 0x4000), (0x4020, 0x4001), (0x3000, 0x4000)] {
        assert!(unsafe { Entry::load(&lib, b"step\0", GuestAddr(address), base) }.is_err());
    }
    assert!(unsafe { Entry::load(&lib, b"step", GuestAddr(0x4020), 0x4000) }.is_ok());
}
unsafe extern "C" fn forbidden_call(
    _: *mut std::ffi::c_void,
    _: *mut mkw_runtime::abi::Cpu,
    _: u64,
    _: *mut u64,
) -> i32 {
    99
}
#[test]
fn checked_v10_entry_rejects_tampering_before_any_state_or_budget_commit() {
    let p = folder("refusal");
    for opt in ["-O0", "-O2"] {
        let lib = build(&p, opt, 10);
        let entry = unsafe { Entry::load(&lib, b"step\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        let mem = memory();
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        for problem in 0..10 {
            let c = guard.cpu_mut();
            let ops = c.exclusive;
            let ctx = c.exclusive_context;
            let memory = c.base.memory;
            let ordered = c.base.ordered_memory;
            let atomic = c.base.atomic_memory;
            let barrier = c.base.barrier;
            let old_pc = c.base.pc;
            let old_base = c.base.module_base;
            match problem {
                0 => c.exclusive = std::ptr::null(),
                1 => c.exclusive = std::ptr::NonNull::<Ops>::dangling().as_ptr(),
                2 => c.exclusive_context = std::ptr::null_mut(),
                3 => c.base.memory = None,
                4 => c.base.memory_context = std::ptr::null_mut(),
                5 => c.base.ordered_memory = None,
                6 => c.base.atomic_memory = None,
                7 => c.base.barrier = None,
                8 => c.base.call_context = std::ptr::NonNull::<u8>::dangling().as_ptr().cast(),
                _ => c.base.call = Some(forbidden_call),
            };
            let mut budget = 3;
            assert!(entry.run(&mut guard, &mut budget).is_err());
            let c = guard.cpu_mut();
            assert_eq!(
                (c.base.pc, c.base.module_base, budget),
                (old_pc, old_base, 3)
            );
            c.exclusive = ops;
            c.exclusive_context = ctx;
            c.base.memory = memory;
            c.base.ordered_memory = ordered;
            c.base.atomic_memory = atomic;
            c.base.barrier = barrier;
            c.base.call = None;
            c.base.call_context = std::ptr::null_mut();
            c.base.memory_context = ctx;
        }
        let replacement = memory();
        std::mem::swap(&mut *mem.lock().unwrap(), &mut *replacement.lock().unwrap());
        let mut budget = 3;
        assert!(entry.run(&mut guard, &mut budget).is_err());
        assert_eq!(budget, 3);
        drop(guard);
        drop(bridge);
        let poisoned = memory();
        let mut bridge = Bridge::new(poisoned.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        assert!(std::thread::spawn(move || {
            let _lock = poisoned.lock().unwrap();
            panic!("intentional authority poisoning");
        })
        .join()
        .is_err());
        assert!(entry.run(&mut guard, &mut budget).is_err());
        assert_eq!(
            (
                budget,
                guard.cpu_mut().base.pc,
                guard.cpu_mut().base.module_base
            ),
            (3, 0xbeef, 0xdead)
        );
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> u32>(b"entry_calls\0")
                    .unwrap()()
            },
            0
        );
    }
}
#[test]
fn v10_tail_router_rebases_preserves_reservations_and_refuses_cycles_or_unknowns() {
    use mkw_runtime::exclusive_entry::TailRouter;
    let p = folder("routing");
    for opt in ["-O0", "-O2"] {
        let first = build(&p.join("first"), opt, 10);
        let second = build(&p.join("second"), opt, 10);
        let chain = unsafe { Entry::load(&first, b"chain\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        let finish =
            unsafe { Entry::load(&second, b"finish\0", GuestAddr(0x8040), 0x8000) }.unwrap();
        let router = TailRouter::new(vec![chain, finish]).unwrap();
        for initial_budget in 0..4 {
            let mem = memory();
            mem.lock()
                .unwrap()
                .write(GuestAddr(0x1000), &7u64.to_le_bytes())
                .unwrap();
            let mut bridge = Bridge::new(mem.clone()).unwrap();
            let mut frame = cpu();
            let mut guard = bridge.attach(&mut frame).unwrap();
            let mut budget = initial_budget;
            assert_eq!(
                router.run(&mut guard, GuestAddr(0x4020), &mut budget),
                Ok(if initial_budget < 2 { 2 } else { 0 })
            );
            let c = &guard.cpu_mut().base;
            assert_eq!(budget, initial_budget.saturating_sub(2));
            assert_eq!(
                c.pc,
                if initial_budget == 0 {
                    0x4020
                } else if initial_budget == 1 {
                    0x8040
                } else {
                    0x9000
                }
            );
            assert_eq!(
                c.module_base,
                if initial_budget < 2 { 0x4000 } else { 0x8000 }
            );
            assert_eq!(c.x[2], if initial_budget < 2 { 88 } else { 0 });
            assert_eq!(
                mem.lock()
                    .unwrap()
                    .read(GuestAddr(0x1000), Width::Double)
                    .unwrap(),
                if initial_budget < 2 { 7 } else { 8 }
            );
        }
        for (symbol, expected) in [(b"tail\0".as_slice(), 6), (b"missing_tail\0", 3)] {
            let entry = unsafe { Entry::load(&first, symbol, GuestAddr(0x4020), 0x4000) }.unwrap();
            let router = TailRouter::new(vec![entry]).unwrap();
            let mut bridge = Bridge::new(memory()).unwrap();
            let mut frame = cpu();
            let mut guard = bridge.attach(&mut frame).unwrap();
            let mut budget = 3;
            assert_eq!(
                router.run(&mut guard, GuestAddr(0x4020), &mut budget),
                Ok(expected)
            );
            assert_eq!(guard.cpu_mut().base.fault_status, expected);
            assert_eq!(guard.cpu_mut().base.pc, 0x8800);
            assert_eq!(budget, if expected == 6 { 3 } else { 2 });
            if expected == 3 {
                assert_eq!(guard.cpu_mut().base.fault_address, 0x8800);
            }
        }
        let a = unsafe { Entry::load(&first, b"chain\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        let b = unsafe { Entry::load(&second, b"finish\0", GuestAddr(0x4020), 0x4000) }.unwrap();
        assert!(TailRouter::new(vec![a, b]).is_err());
        let mut bridge = Bridge::new(memory()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let context = guard.cpu_mut().exclusive_context;
        guard.cpu_mut().exclusive_context = std::ptr::null_mut();
        let mut budget = 4;
        assert!(router
            .run(&mut guard, GuestAddr(0x4020), &mut budget)
            .is_err());
        assert_eq!((guard.cpu_mut().base.pc, budget), (0xbeef, 4));
        guard.cpu_mut().exclusive_context = context;
        assert_eq!(
            router.run(&mut guard, GuestAddr(0x9999), &mut budget),
            Ok(3)
        );
        assert_eq!(
            (
                guard.cpu_mut().base.pc,
                guard.cpu_mut().base.fault_address,
                budget
            ),
            (0xbeef, 0x9999, 4)
        );
    }
}
