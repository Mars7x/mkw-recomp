use mkw_runtime::{
    abi::Cpu,
    dispatch::NativeTarget,
    exclusive::{Config, ExclusiveMemory},
    exclusive_native::{Bridge, CpuV10, Ops, CPU_V10_ABI_VERSION, EXCLUSIVE_V1_VERSION},
    GuestAddr, Memory, Permissions,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
type Operation =
    unsafe extern "C" fn(*mut CpuV10, u32, u64, u32, u32, *mut u64, u32, *mut u32) -> i32;
fn folder() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/exclusive-native");
    std::fs::create_dir_all(&p).unwrap();
    p
}
fn library(path: &Path, opt: &str) -> libloading::Library {
    let source = path.join("contract.c");
    std::fs::write(&source,r#"
#include "exclusive_v10.h"
size_t frame_size(void){return sizeof(MkwCpuV10);}
size_t prefix_size(void){return sizeof(MkwCpu);}
size_t ops_size(void){return sizeof(MkwExclusiveV1);}
size_t ops_offset(void){return offsetof(MkwCpuV10,exclusive);}
size_t context_offset(void){return offsetof(MkwCpuV10,exclusive_context);}
unsigned native_v10_version(void){return MKW_CPU_V10_ABI_VERSION;}
int operate(MkwCpuV10 *cpu,uint32_t action,uint64_t address,uint32_t bytes,uint32_t order,uint64_t *data,uint32_t words,uint32_t *stored){
 if(!mkw_exclusive_v10_valid(cpu))return 3;
 if(action==0)return cpu->exclusive->load(cpu->exclusive_context,address,bytes,order,data,words);
 if(action==1)return cpu->exclusive->store(cpu->exclusive_context,address,bytes,order,data,words,stored);
 if(action==2)return cpu->exclusive->clear(cpu->exclusive_context);
 return 3;
}
int prefix(MkwCpuV10 *cpu,uint32_t kind,uint32_t order,uint32_t op,uint64_t address,uint64_t input,uint64_t replacement,uint64_t *data,uint32_t bytes){
 if(!mkw_exclusive_v10_valid(cpu))return 3;
 if(kind==0)return cpu->base.memory(cpu->base.memory_context,(int)op,address,data,bytes);
 if(kind==1)return cpu->base.ordered_memory(cpu->base.memory_context,order,(int)op,address,data,bytes);
 if(kind==2)return cpu->base.atomic_memory(cpu->base.memory_context,order,op,address,input,replacement,data,bytes);
 if(kind==3)return cpu->base.barrier(cpu->base.memory_context,op,order);
 return 3;
}
static unsigned calls;
unsigned entry_calls(void){return calls;}
unsigned mkw_guest_v10_budget_abi_version(void){return MKW_CPU_V10_ABI_VERSION;}
int mkw_guest_v10_budget(MkwCpuV10 *cpu,uint64_t *budget){
 ++calls;if(!mkw_exclusive_v10_valid(cpu))return 3;if(!*budget)return 2;--*budget;
 return cpu->exclusive->clear(cpu->exclusive_context);
}
"#).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native");
    let out = path.join(format!("native{}.so", &opt[1..]));
    let c = Command::new("cc")
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
        .arg(&source)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
    // SAFETY: only this original synthetic C helper is loaded; compare ABI before calls.
    unsafe {
        let lib = libloading::Library::new(out).unwrap();
        for (name, expected) in [
            (b"frame_size\0".as_slice(), std::mem::size_of::<CpuV10>()),
            (b"prefix_size\0", std::mem::size_of::<Cpu>()),
            (b"ops_size\0", std::mem::size_of::<Ops>()),
            (b"ops_offset\0", std::mem::offset_of!(CpuV10, exclusive)),
            (
                b"context_offset\0",
                std::mem::offset_of!(CpuV10, exclusive_context),
            ),
        ] {
            assert_eq!(
                lib.get::<unsafe extern "C" fn() -> usize>(name).unwrap()(),
                expected
            );
        }
        assert_eq!(
            lib.get::<unsafe extern "C" fn() -> u32>(b"native_v10_version\0")
                .unwrap()(),
            CPU_V10_ABI_VERSION
        );
        lib
    }
}
fn memory(owners: usize) -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(GuestAddr(0x1000), vec![0; 128], Permissions::RW, "rw")
        .unwrap();
    m.map(GuestAddr(0x2000), vec![0; 16], Permissions::R, "ro")
        .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: owners,
            },
        )
        .unwrap(),
    ))
}
fn cpu() -> CpuV10 {
    let mut frame = CpuV10::default();
    frame.base.x = std::array::from_fn(|n| n as u64 + 123);
    frame.base.sp = 77;
    frame.base.n = 1;
    frame.base.c = 1;
    frame.base.v = 1;
    frame.base.fpsr = 0x08000091;
    frame.base.fpcr = 0x02c00000;
    frame.base.fault_status = 77;
    frame.base.fault_address = 91;
    frame
}
unsafe extern "C" fn unrelated_memory(
    _: *mut std::ffi::c_void,
    _: i32,
    _: u64,
    _: *mut u64,
    _: u32,
) -> i32 {
    3
}
#[test]
fn native_v10_contract_layout_values_and_shared_invalidation() {
    let path = folder();
    let mut cases = 0;
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let f = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        for width in [1, 2, 4, 8, 16] {
            for order in 0..2 {
                let mem = memory(2);
                let mut a = Bridge::new(mem.clone()).unwrap();
                let mut b = Bridge::new(mem.clone()).unwrap();
                let (mut ac, mut bc) = (cpu(), cpu());
                let x = ac.base.x;
                {
                    let (mut a, mut b) = (a.attach(&mut ac).unwrap(), b.attach(&mut bc).unwrap());
                    let mut data = [0x123456789abcdef0, 0xfedcba9876543210, 0xdead, 0xbeef];
                    let original = data;
                    let mut stored = 0xaaaa;
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                0,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(&data[..2], &[0, 0]);
                    assert_eq!(&data[2..], &original[2..]);
                    assert_eq!(stored, 0xaaaa);
                    assert_eq!(
                        unsafe {
                            f(
                                b.cpu_mut(),
                                0,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    data = original;
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                1,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(stored, 1);
                    let bytes = mem
                        .lock()
                        .unwrap()
                        .read(
                            GuestAddr(0x1000),
                            mkw_runtime::exclusive::Width::try_from(width as usize).unwrap(),
                        )
                        .unwrap();
                    let mask = if width == 16 {
                        u128::MAX
                    } else {
                        (1u128 << (width * 8)) - 1
                    };
                    assert_eq!(
                        bytes,
                        (u128::from(original[0]) | (u128::from(original[1]) << 64)) & mask
                    );
                    stored = 0xaaaa;
                    assert_eq!(
                        unsafe {
                            f(
                                b.cpu_mut(),
                                1,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(stored, 0);
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                0,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    mem.lock().unwrap().write(GuestAddr(0x100f), &[0]).unwrap();
                    stored = 0xaaaa;
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                1,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(stored, 0);
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                0,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                2,
                                0,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    stored = 0xaaaa;
                    assert_eq!(
                        unsafe {
                            f(
                                a.cpu_mut(),
                                1,
                                0x1000,
                                width,
                                order,
                                data.as_mut_ptr(),
                                2,
                                &mut stored,
                            )
                        },
                        0
                    );
                    assert_eq!(stored, 0);
                }
                assert!(ac.exclusive.is_null() && ac.exclusive_context.is_null());
                assert!(bc.exclusive.is_null() && bc.exclusive_context.is_null());
                assert_eq!(ac.base.x, x);
                assert_eq!(
                    (
                        ac.base.sp,
                        ac.base.n,
                        ac.base.z,
                        ac.base.c,
                        ac.base.v,
                        ac.base.fpsr,
                        ac.base.fpcr,
                        ac.base.fault_status,
                        ac.base.fault_address
                    ),
                    (77, 1, 0, 1, 1, 0x08000091, 0x02c00000, 77, 91)
                );
                cases += 1;
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"width_order_cases_both_optimizations":cases,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"cpu_v10_frame_exclusive_v1_only":true,"legacy_cpu_v9_unchanged":true,"coherent_prefix_memory_routing_pending":false,"llsc_isa_acceptance_enabled":false,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn native_v10_invalid_versions_arguments_faults_and_legacy_refusal() {
    let path = folder().join("faults");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let f = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        // SAFETY: trusted synthetic helper advertises ABI10. The ABI9 loader must
        // reject before fetching/invoking the differently typed entry function.
        let rejected =
            unsafe { NativeTarget::load(&lib, b"mkw_guest_v10_budget\0", GuestAddr(0), 0) };
        assert!(matches!(rejected,Err(e)if e.contains("ABI version mismatch")));
        assert_eq!(
            unsafe {
                lib.get::<unsafe extern "C" fn() -> u32>(b"entry_calls\0")
                    .unwrap()()
            },
            0
        );
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        assert!(Bridge::new(mem.clone()).is_err());
        let mut frame = cpu();
        frame.base.memory = Some(unrelated_memory);
        assert!(bridge.attach(&mut frame).is_err());
        assert!(frame.exclusive.is_null());
        frame.base.memory = None;
        {
            let mut attached = bridge.attach(&mut frame).unwrap();
            let cpu = attached.cpu_mut();
            let ops = cpu.exclusive;
            let ctx = cpu.exclusive_context;
            let mut data = [0xdead, 0xbeef];
            let mut stored = 0xaaaa;
            let mut bad = unsafe { *ops };
            assert_eq!(bad.version, EXCLUSIVE_V1_VERSION);
            for problem in 0..5 {
                bad = unsafe { *ops };
                match problem {
                    0 => bad.version = 0,
                    1 => bad.version = 2,
                    2 => bad.size = 0,
                    3 => bad.load = None,
                    _ => bad.store = None,
                };
                cpu.exclusive = std::ptr::addr_of!(bad);
                assert_eq!(
                    unsafe { f(cpu, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                    3
                );
                assert_eq!(data, [0xdead, 0xbeef]);
            }
            cpu.exclusive = ops;
            cpu.exclusive_context = std::ptr::null_mut();
            assert_eq!(
                unsafe { f(cpu, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                3
            );
            cpu.exclusive_context = ctx;
            cpu.base.memory = Some(unrelated_memory);
            assert_eq!(
                unsafe { f(cpu, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                3
            );
            cpu.base.memory = unsafe { (*ops).memory };
            for (width, order, words) in [
                (0, 0, 2),
                (3, 0, 2),
                (32, 0, 2),
                (4, 2, 2),
                (4, 0, 1),
                (4, 0, 3),
            ] {
                assert_eq!(
                    unsafe {
                        f(
                            cpu,
                            0,
                            0x1000,
                            width,
                            order,
                            data.as_mut_ptr(),
                            words,
                            &mut stored,
                        )
                    },
                    3
                );
                assert_eq!(data, [0xdead, 0xbeef]);
            }
            assert_eq!(
                unsafe { f(cpu, 0, 0x1000, 4, 0, std::ptr::null_mut(), 2, &mut stored) },
                3
            );
            assert_eq!(
                unsafe {
                    f(
                        cpu,
                        1,
                        0x1000,
                        4,
                        0,
                        data.as_mut_ptr(),
                        2,
                        std::ptr::null_mut(),
                    )
                },
                3
            );
            assert_eq!(stored, 0xaaaa);
            assert_eq!(
                unsafe { f(cpu, 0, 0x1001, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                1
            );
            assert_eq!(data, [0xdead, 0xbeef]);
            assert_eq!(
                unsafe { f(cpu, 0, 0x3000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                1
            );
            assert_eq!(data, [0xdead, 0xbeef]);
            assert_eq!(
                unsafe { f(cpu, 0, 0x2000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            stored = 0xaaaa;
            assert_eq!(
                unsafe { f(cpu, 1, 0x2000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                1
            );
            assert_eq!(stored, 0xaaaa);
            assert_eq!(
                unsafe { f(cpu, 1, 0x2000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            assert_eq!(stored, 0);
            assert_eq!(
                unsafe { f(cpu, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            let mut budget = 0;
            let entry = unsafe {
                lib.get::<unsafe extern "C" fn(*mut CpuV10, *mut u64) -> i32>(
                    b"mkw_guest_v10_budget\0",
                )
                .unwrap()
            };
            assert_eq!(unsafe { entry(cpu, &mut budget) }, 2);
            budget = 1;
            assert_eq!(unsafe { entry(cpu, &mut budget) }, 0);
            assert_eq!(budget, 0);
            stored = 0xaaaa;
            assert_eq!(
                unsafe { f(cpu, 1, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            assert_eq!(stored, 0);
        }
        drop(bridge);
        assert!(Bridge::new(mem).is_ok());
    }
}
#[test]
fn native_v10_poisoned_authority_refuses_without_output_commit() {
    let path = folder().join("poison");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let f = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = CpuV10::default();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let poisoned = mem.clone();
        assert!(std::thread::spawn(move || {
            let _lock = poisoned.lock().unwrap();
            panic!("synthetic authority poison");
        })
        .join()
        .is_err());
        let mut data = [0xdead, 0xbeef];
        let mut stored = 0xaaaa;
        for action in 0..3 {
            assert_eq!(
                unsafe {
                    f(
                        guard.cpu_mut(),
                        action,
                        0x1000,
                        4,
                        0,
                        data.as_mut_ptr(),
                        2,
                        &mut stored,
                    )
                },
                3
            );
            assert_eq!(data, [0xdead, 0xbeef]);
            assert_eq!(stored, 0xaaaa);
        }
    }
}

type Prefix = unsafe extern "C" fn(*mut CpuV10, u32, u32, u32, u64, u64, u64, *mut u64, u32) -> i32;
#[test]
fn coherent_prefix_transfer_widths_and_interference() {
    let path = folder().join("prefix");
    std::fs::create_dir_all(&path).unwrap();
    let mut cases = 0;
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let access = unsafe { lib.get::<Prefix>(b"prefix\0").unwrap() };
        let exclusive = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let c = guard.cpu_mut();
        for bytes in [1u32, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64] {
            for unaligned in [0u64, 1] {
                let address = 0x1000 + unaligned;
                let data = std::array::from_fn::<_, 8, _>(|i| {
                    0xfedcba9876543210u64.wrapping_add(i as u64 * 0x102030405060708)
                });
                let mut buffer = data;
                assert_eq!(
                    unsafe { access(c, 0, 0, 1, address, 0, 0, buffer.as_mut_ptr(), bytes) },
                    0
                );
                buffer.fill(0xdeadbeefdeadbeef);
                assert_eq!(
                    unsafe { access(c, 0, 0, 0, address, 0, 0, buffer.as_mut_ptr(), bytes) },
                    0
                );
                let flat: Vec<u8> = data.into_iter().flat_map(u64::to_le_bytes).collect();
                let mut expected = [0xdeadbeefdeadbeef; 8];
                for (i, chunk) in flat[..bytes as usize].chunks(8).enumerate() {
                    let mut b = [0; 8];
                    b[..chunk.len()].copy_from_slice(chunk);
                    expected[i] = u64::from_le_bytes(b);
                }
                assert_eq!(buffer, expected);
                cases += 1;
            }
        }
        for kind in 0..2 {
            for order in if kind == 0 { vec![0] } else { vec![3] } {
                for bytes in if kind == 0 {
                    vec![1u32, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64]
                } else {
                    vec![1, 2, 4, 8]
                } {
                    mem.lock()
                        .unwrap()
                        .write(GuestAddr(0x1000), &[0; 64])
                        .unwrap();
                    let mut data = [0u64; 8];
                    let mut stored = 99;
                    assert_eq!(
                        unsafe { exclusive(c, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                        0
                    );
                    // Even an unchanged-value write must invalidate the monitor.
                    assert_eq!(
                        unsafe {
                            access(c, kind, order, 1, 0x1000, 0, 0, data.as_mut_ptr(), bytes)
                        },
                        0
                    );
                    assert_eq!(
                        unsafe { exclusive(c, 1, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                        0
                    );
                    assert_eq!(stored, 0);
                    cases += 1;
                }
            }
        }
        let mut data = [0u64; 8];
        let mut stored = 99;
        assert_eq!(
            unsafe { exclusive(c, 0, 0x1070, 16, 0, data.as_mut_ptr(), 2, &mut stored) },
            0
        );
        let saved = mem
            .lock()
            .unwrap()
            .read(GuestAddr(0x1070), mkw_runtime::exclusive::Width::Pair)
            .unwrap();
        assert_eq!(
            unsafe { access(c, 0, 0, 1, 0x107f, 0, 0, data.as_mut_ptr(), 2) },
            5
        );
        assert_eq!(
            mem.lock()
                .unwrap()
                .read(GuestAddr(0x1070), mkw_runtime::exclusive::Width::Pair)
                .unwrap(),
            saved
        );
        assert_eq!(
            unsafe { exclusive(c, 1, 0x1070, 16, 0, data.as_mut_ptr(), 2, &mut stored) },
            0
        );
        assert_eq!(stored, 1);
        for (kind, order, op, bytes, expected) in [
            (0, 0, 2, 4, 3),
            (0, 0, 0, 0, 3),
            (0, 0, 0, 5, 3),
            (1, 0, 0, 4, 3),
            (1, 1, 1, 4, 5),
            (1, 3, 0, 4, 5),
            (1, 1, 0, 16, 3),
            (1, 4, 0, 4, 3),
        ] {
            data.fill(0xdead);
            assert_eq!(
                unsafe { access(c, kind, order, op, 0x1000, 0, 0, data.as_mut_ptr(), bytes) },
                expected
            );
            assert_eq!(data, [0xdead; 8]);
        }
        for order in [1, 2] {
            data.fill(0xdead);
            assert_eq!(
                unsafe { access(c, 1, order, 0, 0x1001, 0, 0, data.as_mut_ptr(), 4) },
                5
            );
            assert_eq!(data, [0xdead; 8]);
            assert_eq!(
                unsafe { access(c, 1, order, 0, 0x1000, 0, 0, data.as_mut_ptr(), 4) },
                0
            );
        }
        data.fill(0xdead);
        assert_eq!(
            unsafe { access(c, 0, 0, 0, 0x3000, 0, 0, data.as_mut_ptr(), 4) },
            5
        );
        assert_eq!(data, [0xdead; 8]);
        assert_eq!(
            unsafe { access(c, 0, 0, 0, 0x1000, 0, 0, std::ptr::null_mut(), 4) },
            7
        );
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_transfer_and_interference_cases_both_optimizations":cases,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
fn atomic_expected(op: u32, old: u64, input: u64, replacement: u64, bytes: u32) -> u64 {
    let mask = if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    };
    let input = input & mask;
    let signed = |v: u64| {
        let v = i128::from(v);
        if v & (1i128 << (bytes * 8 - 1)) != 0 {
            v - (1i128 << (bytes * 8))
        } else {
            v
        }
    };
    let value = match op {
        0 => (u128::from(old) + u128::from(input)) as u64,
        1 => old & !input,
        2 => old ^ input,
        3 => old | input,
        4 => {
            if signed(old) >= signed(input) {
                old
            } else {
                input
            }
        }
        5 => {
            if signed(old) <= signed(input) {
                old
            } else {
                input
            }
        }
        6 => old.max(input),
        7 => old.min(input),
        8 => input,
        9 => {
            if old == input {
                replacement
            } else {
                old
            }
        }
        _ => panic!(),
    };
    value & mask
}
#[test]
fn coherent_atomic_and_barrier_callbacks_preserve_monitor_rules() {
    let path = folder().join("atomic");
    std::fs::create_dir_all(&path).unwrap();
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let access = unsafe { lib.get::<Prefix>(b"prefix\0").unwrap() };
        let exclusive = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let c = guard.cpu_mut();
        for bytes in [1u32, 2, 4, 8] {
            let mask = if bytes == 8 {
                u64::MAX
            } else {
                (1u64 << (bytes * 8)) - 1
            };
            for order in 0..4 {
                for op in 0..10 {
                    for initial in [0, 1, mask, mask >> 1, (mask >> 1) + 1] {
                        for input in [0, 1, mask, initial] {
                            let replacement = 0xfedcba9876543210;
                            mem.lock()
                                .unwrap()
                                .write(GuestAddr(0x1000), &initial.to_le_bytes()[..bytes as usize])
                                .unwrap();
                            let mut data = [0xdead; 8];
                            let mut stored = 99;
                            assert_eq!(
                                unsafe {
                                    exclusive(
                                        c,
                                        0,
                                        0x1000,
                                        bytes,
                                        0,
                                        data.as_mut_ptr(),
                                        2,
                                        &mut stored,
                                    )
                                },
                                0
                            );
                            data.fill(0xdead);
                            assert_eq!(
                                unsafe {
                                    access(
                                        c,
                                        2,
                                        order,
                                        op,
                                        0x1000,
                                        input,
                                        replacement,
                                        data.as_mut_ptr(),
                                        bytes,
                                    )
                                },
                                0
                            );
                            assert_eq!(data[0], initial);
                            assert_eq!(&data[1..], &[0xdead; 7]);
                            let value = mem
                                .lock()
                                .unwrap()
                                .read(
                                    GuestAddr(0x1000),
                                    mkw_runtime::exclusive::Width::try_from(bytes as usize)
                                        .unwrap(),
                                )
                                .unwrap() as u64;
                            assert_eq!(
                                value,
                                atomic_expected(op, initial, input, replacement, bytes)
                            );
                            let should_survive = op == 9 && initial != (input & mask);
                            assert_eq!(
                                unsafe {
                                    exclusive(
                                        c,
                                        1,
                                        0x1000,
                                        bytes,
                                        0,
                                        data.as_mut_ptr(),
                                        2,
                                        &mut stored,
                                    )
                                },
                                0
                            );
                            assert_eq!(stored, u32::from(should_survive));
                            count += 1;
                        }
                    }
                }
            }
        }
        // Matched CAS and zero-add are genuine stores even when the bytes stay equal.
        for op in [0u32, 9] {
            let mut data = [0u64; 8];
            let mut stored = 99;
            mem.lock()
                .unwrap()
                .write(GuestAddr(0x1000), &[0; 8])
                .unwrap();
            assert_eq!(
                unsafe { exclusive(c, 0, 0x1000, 8, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            assert_eq!(
                unsafe { access(c, 2, 0, op, 0x1000, 0, 0, data.as_mut_ptr(), 8) },
                0
            );
            assert_eq!(
                unsafe { exclusive(c, 1, 0x1000, 8, 0, data.as_mut_ptr(), 2, &mut stored) },
                0
            );
            assert_eq!(stored, 0);
        }
        for (kind, order, op, address, bytes, status) in [
            (2, 4, 0, 0x1000, 4, 3),
            (2, 0, 10, 0x1000, 4, 3),
            (2, 0, 0, 0x1000, 16, 5),
            (2, 0, 0, 0x1001, 4, 5),
            (2, 0, 9, 0x2000, 4, 5),
        ] {
            let mut data = [0xdead; 8];
            assert_eq!(
                unsafe { access(c, kind, order, op, address, 0, 0, data.as_mut_ptr(), bytes) },
                status
            );
            assert_eq!(data, [0xdead; 8]);
        }
        for kind in 0..3 {
            for option in 0..17 {
                let mut data = [0xdead; 8];
                let mut stored = 99;
                assert_eq!(
                    unsafe { exclusive(c, 0, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                    0
                );
                let expected = if kind < 2 && option < 16 && option & 3 != 0 {
                    0
                } else {
                    3
                };
                assert_eq!(
                    unsafe { access(c, 3, option, kind, 0, 0, 0, data.as_mut_ptr(), 0) },
                    expected
                );
                assert_eq!(
                    unsafe { exclusive(c, 1, 0x1000, 4, 0, data.as_mut_ptr(), 2, &mut stored) },
                    0
                );
                assert_eq!(stored, 1);
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"atomic_native_cases_both_optimizations":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"cas_mismatch_preserves_reservation":true,"same_value_stores_invalidate":true,"game_code_executed":false})).unwrap()).unwrap();
}

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

#[test]
fn native_v10_callback_sequences_match_synthetic_qemu() {
    let path = folder().join("qemu");
    std::fs::create_dir_all(&path).unwrap();
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            path.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\"}",
        )
        .unwrap();
        return;
    }
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
    let elf = path.join("synthetic-aarch64");
    std::fs::write(&elf, image(&cases)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o700)).unwrap();
    let reference = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(&elf)
        .current_dir(&path)
        .output()
        .unwrap();
    assert!(
        reference.status.success(),
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    assert_eq!(reference.stdout.len(), cases.len() * 24);
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let prefix = unsafe { lib.get::<Prefix>(b"prefix\0").unwrap() };
        let exclusive = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(1);
        let mut bridge = Bridge::new(mem).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let cpu = guard.cpu_mut();
        for (i, c) in cases.iter().enumerate() {
            let bytes = 1u32 << c.size;
            let order = u32::from(c.ordered);
            let mut data = [0u64; 8];
            data[0] = c.initial;
            let mut stored = 99;
            assert_eq!(
                unsafe { prefix(cpu, 0, 0, 1, 0x1000, 0, 0, data.as_mut_ptr(), bytes) },
                0
            );
            assert_eq!(
                unsafe {
                    exclusive(
                        cpu,
                        0,
                        0x1000,
                        bytes,
                        order,
                        data.as_mut_ptr(),
                        2,
                        &mut stored,
                    )
                },
                0
            );
            let loaded = data[0];
            match c.interference {
                0 => {}
                1 => assert_eq!(
                    unsafe { exclusive(cpu, 2, 0, 0, 0, data.as_mut_ptr(), 2, &mut stored) },
                    0
                ),
                2 => {
                    data[0] = c.initial ^ 1;
                    assert_eq!(
                        unsafe { prefix(cpu, 0, 0, 1, 0x1000, 0, 0, data.as_mut_ptr(), bytes) },
                        0
                    );
                }
                _ => panic!(),
            };
            data[0] = c.desired;
            assert_eq!(
                unsafe {
                    exclusive(
                        cpu,
                        1,
                        0x1000,
                        bytes,
                        order,
                        data.as_mut_ptr(),
                        2,
                        &mut stored,
                    )
                },
                0
            );
            assert_eq!(
                unsafe { prefix(cpu, 0, 0, 0, 0x1000, 0, 0, data.as_mut_ptr(), bytes) },
                0
            );
            let actual: Vec<u8> = [loaded, u64::from(1 - stored), data[0]]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect();
            assert_eq!(
                actual,
                &reference.stdout[i * 24..(i + 1) * 24],
                "{opt} case{i}"
            );
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_native_qemu_cases_per_optimization":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"uses_actual_v10_callbacks_and_coherent_plain_write_path":true,"hardware_reference":false,"target_configuration_verified":false,"llsc_isa_acceptance_enabled":false,"game_code_executed":false})).unwrap()).unwrap();
}

#[test]
fn stale_authority_context_refuses_every_route() {
    let path = folder().join("stale");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let prefix = unsafe { lib.get::<Prefix>(b"prefix\0").unwrap() };
        let exclusive = unsafe { lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        let replacement = memory(1);
        std::mem::swap(&mut *mem.lock().unwrap(), &mut *replacement.lock().unwrap());
        let mut data = [0xdead; 8];
        let mut stored = 0xaaaa;
        for action in 0..3 {
            assert_eq!(
                unsafe {
                    exclusive(
                        guard.cpu_mut(),
                        action,
                        0x1000,
                        4,
                        0,
                        data.as_mut_ptr(),
                        2,
                        &mut stored,
                    )
                },
                3
            );
            assert_eq!(data, [0xdead; 8]);
            assert_eq!(stored, 0xaaaa);
        }
        for (kind, order, op) in [
            (0, 0, 0),
            (0, 0, 1),
            (1, 1, 0),
            (1, 3, 1),
            (2, 0, 0),
            (3, 15, 0),
        ] {
            assert_eq!(
                unsafe {
                    prefix(
                        guard.cpu_mut(),
                        kind,
                        order,
                        op,
                        0x1000,
                        0,
                        0,
                        data.as_mut_ptr(),
                        4,
                    )
                },
                3
            );
            assert_eq!(data, [0xdead; 8]);
        }
        assert_eq!(
            mem.lock()
                .unwrap()
                .read(GuestAddr(0x1000), mkw_runtime::exclusive::Width::Word)
                .unwrap(),
            0
        );
    }
}
#[allow(dead_code)]
#[path = "support/fp_native.rs"]
mod native_support;
fn lse_word(size: u32, order: u32, op: u32) -> u32 {
    if op == 9 {
        0x08a07c00 | (size << 30) | ((order & 1) << 22) | ((order >> 1) << 15) | (2 << 5) | 1
    } else {
        0x38200000
            | (size << 30)
            | ((order & 1) << 23)
            | ((order >> 1) << 22)
            | (op << 12)
            | (2 << 5)
            | 1
    }
}
#[test]
fn unchanged_v9_aot_leaves_use_coherent_v10_prefix_callbacks() {
    let path = folder().join("aot-prefix");
    std::fs::create_dir_all(&path).unwrap();
    let mut source = String::new();
    for size in 0..4 {
        for order in 0..4 {
            for op in 0..10 {
                source += &native_support::generate(
                    &[lse_word(size, order, op), 0xd65f03c0],
                    &format!("coherent_lse_{size}_{order}_{op}"),
                );
            }
        }
    }
    source += &native_support::generate(&[0x4c002020, 0x4c402024, 0xd65f03c0], "coherent_wide");
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = native_support::build(&source, &path, opt);
        let mem = memory(1);
        let mut bridge = Bridge::new(mem.clone()).unwrap();
        let mut frame = cpu();
        let mut guard = bridge.attach(&mut frame).unwrap();
        for size in 0..4 {
            let bytes = 1u32 << size;
            let mask = if bytes == 8 {
                u64::MAX
            } else {
                (1u64 << (bytes * 8)) - 1
            };
            for order in 0..4 {
                for op in 0..10 {
                    let f = unsafe {
                        lib.get::<native_support::Entry>(
                            format!("coherent_lse_{size}_{order}_{op}\0").as_bytes(),
                        )
                        .unwrap()
                    };
                    for initial in [0, 1, mask, (mask >> 1) + 1] {
                        for input in [0, 1, mask, initial] {
                            let c = guard.cpu_mut();
                            mem.lock()
                                .unwrap()
                                .write(GuestAddr(0x1000), &initial.to_le_bytes()[..bytes as usize])
                                .unwrap();
                            let mut data = [0; 2];
                            let mut stored = 99;
                            let ops = unsafe { &*c.exclusive };
                            assert_eq!(
                                unsafe {
                                    ops.load.unwrap()(
                                        c.exclusive_context,
                                        0x1000,
                                        bytes,
                                        0,
                                        data.as_mut_ptr(),
                                        2,
                                    )
                                },
                                0
                            );
                            c.base.x[0] = input;
                            c.base.x[1] = 0x0123456789abcdef;
                            c.base.x[2] = 0x1000;
                            assert_eq!(unsafe { f(&mut c.base, 8) }, 0);
                            assert_eq!(c.base.x[if op == 9 { 0 } else { 1 }], initial);
                            assert_eq!(
                                mem.lock()
                                    .unwrap()
                                    .read(
                                        GuestAddr(0x1000),
                                        mkw_runtime::exclusive::Width::try_from(bytes as usize)
                                            .unwrap()
                                    )
                                    .unwrap() as u64,
                                atomic_expected(op, initial, input, 0x0123456789abcdef, bytes)
                            );
                            assert_eq!(
                                unsafe {
                                    ops.store.unwrap()(
                                        c.exclusive_context,
                                        0x1000,
                                        bytes,
                                        0,
                                        data.as_ptr(),
                                        2,
                                        &mut stored,
                                    )
                                },
                                0
                            );
                            assert_eq!(stored, u32::from(op == 9 && initial != (input & mask)));
                            count += 1;
                        }
                    }
                }
            }
        }
        let c = guard.cpu_mut();
        let vectors = std::array::from_fn::<_, 4, _>(|r| {
            [0x0123456789abcdef + r as u64, 0xfedcba9876543210 + r as u64]
        });
        c.base.vregs[..4].copy_from_slice(&vectors);
        c.base.x[1] = 0x1000;
        let mut data = [0; 2];
        let mut stored = 99;
        let ops = unsafe { &*c.exclusive };
        assert_eq!(
            unsafe { ops.load.unwrap()(c.exclusive_context, 0x1030, 8, 0, data.as_mut_ptr(), 2) },
            0
        );
        let wide = unsafe {
            lib.get::<native_support::Entry>(b"coherent_wide\0")
                .unwrap()
        };
        assert_eq!(unsafe { wide(&mut c.base, 8) }, 0);
        assert_eq!(&c.base.vregs[4..8], &vectors);
        assert_eq!(
            unsafe {
                ops.store.unwrap()(
                    c.exclusive_context,
                    0x1030,
                    8,
                    0,
                    data.as_ptr(),
                    2,
                    &mut stored,
                )
            },
            0
        );
        assert_eq!(stored, 0);
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_v9_aot_lse_cases_both_optimizations":count,"native_v9_aot_64_byte_store_load_cases_per_optimization":1,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"v9_layout_and_callback_contracts_unchanged":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn contended_native_v10_atomic_and_exclusive_updates_are_coherent() {
    let path = folder().join("contention");
    std::fs::create_dir_all(&path).unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = library(&path, opt);
        let prefix = unsafe { *lib.get::<Prefix>(b"prefix\0").unwrap() };
        let exclusive = unsafe { *lib.get::<Operation>(b"operate\0").unwrap() };
        let mem = memory(4);
        let old_values = std::thread::scope(|scope| {
            let mut threads = Vec::new();
            for _ in 0..4 {
                let mem = mem.clone();
                threads.push(scope.spawn(move || {
                    let mut bridge = Bridge::new(mem).unwrap();
                    let mut frame = cpu();
                    let mut guard = bridge.attach(&mut frame).unwrap();
                    let mut values = Vec::new();
                    for _ in 0..1000 {
                        let mut data = [0; 8];
                        assert_eq!(
                            unsafe {
                                prefix(guard.cpu_mut(), 2, 3, 0, 0x1000, 1, 0, data.as_mut_ptr(), 8)
                            },
                            0
                        );
                        values.push(data[0]);
                    }
                    values
                }));
            }
            threads
                .into_iter()
                .flat_map(|t| t.join().unwrap())
                .collect::<Vec<_>>()
        });
        let mut sorted = old_values;
        sorted.sort_unstable();
        assert_eq!(sorted, (0..4000).collect::<Vec<_>>());
        assert_eq!(
            mem.lock()
                .unwrap()
                .read(GuestAddr(0x1000), mkw_runtime::exclusive::Width::Double)
                .unwrap(),
            4000
        );
        mem.lock()
            .unwrap()
            .write(GuestAddr(0x1000), &[0; 8])
            .unwrap();
        std::thread::scope(|scope| {
            let mut threads = Vec::new();
            for _ in 0..4 {
                let mem = mem.clone();
                threads.push(scope.spawn(move || {
                    let mut bridge = Bridge::new(mem).unwrap();
                    let mut frame = cpu();
                    let mut guard = bridge.attach(&mut frame).unwrap();
                    let mut successes = 0;
                    let mut attempts = 0;
                    while successes < 500 {
                        attempts += 1;
                        assert!(attempts < 100000, "exclusive progress failed");
                        let mut data = [0u64; 2];
                        let mut stored = 99;
                        assert_eq!(
                            unsafe {
                                exclusive(
                                    guard.cpu_mut(),
                                    0,
                                    0x1000,
                                    8,
                                    1,
                                    data.as_mut_ptr(),
                                    2,
                                    &mut stored,
                                )
                            },
                            0
                        );
                        data[0] += 1;
                        assert_eq!(
                            unsafe {
                                exclusive(
                                    guard.cpu_mut(),
                                    1,
                                    0x1000,
                                    8,
                                    1,
                                    data.as_mut_ptr(),
                                    2,
                                    &mut stored,
                                )
                            },
                            0
                        );
                        if stored == 1 {
                            successes += 1;
                        } else {
                            std::thread::yield_now();
                        }
                    }
                }));
            }
            for t in threads {
                t.join().unwrap();
            }
        });
        assert_eq!(
            mem.lock()
                .unwrap()
                .read(GuestAddr(0x1000), mkw_runtime::exclusive::Width::Double)
                .unwrap(),
            2000
        );
        std::fs::write(path.join(format!("receipt{}.json",&opt[1..])),"{\"threads\":4,\"atomic_increments\":4000,\"exclusive_successful_increments\":2000,\"profile\":\"serialized-owned-normal-memory\",\"fatal_ubsan\":true,\"game_code_executed\":false}").unwrap();
    }
}
