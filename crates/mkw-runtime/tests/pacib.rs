use mkw_runtime::abi::{Cpu, PAuth};
use std::{collections::BTreeSet, path::Path, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
#[repr(C)]
struct Keys {
    high: [u64; 2],
    low: [u64; 2],
}
fn words() -> Vec<u32> {
    let mut out = BTreeSet::new();
    for d in 0..32 {
        for n in [d, (d + 9) % 32, 31] {
            out.insert(0xdac10400 | (n << 5) | d);
        }
    }
    out.into_iter().collect()
}
fn source(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("pacib_{w:x}")))
        .collect()
}
fn state() -> Cpu {
    let mut c = Cpu {
        sp: 0x7788,
        module_base: 0x2000,
        n: 1,
        c: 1,
        v: 1,
        fpcr: 0x02c00000,
        fpsr: 0x0800009f,
        fault_status: 77,
        fault_address: 0xaabbccdd,
        ..Cpu::default()
    };
    c.x = std::array::from_fn(|n| 0x123456789abcdef0 + n as u64);
    c.vregs = std::array::from_fn(|n| [n as u64 + 1, n as u64 + 99]);
    c
}
#[derive(Default)]
struct Probe {
    calls: u32,
    operation: u32,
    key: u32,
    pointer: u64,
    modifier: u64,
    fail: bool,
}
unsafe extern "C" fn probe(
    context: *mut std::ffi::c_void,
    operation: u32,
    key: u32,
    pointer: u64,
    modifier: u64,
    result: *mut u64,
) -> i32 {
    if context.is_null() {
        return 3;
    }
    let p = unsafe { &mut *context.cast::<Probe>() };
    p.calls += 1;
    p.operation = operation;
    p.key = key;
    p.pointer = pointer;
    p.modifier = modifier;
    unsafe {
        *result = 0xabc0123456789def;
    }
    if p.fail {
        7
    } else {
        0
    }
}
fn pac_backend(path: &Path, opt: &str) -> libloading::Library {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = path.join(format!("pac-{}.so", &opt[1..]));
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
        .arg(root.join("native/pac.c"))
        .arg(root.join("third_party/gem5-qarma/qarma.c"))
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    unsafe { libloading::Library::new(out) }.unwrap()
}
fn immediate(words: &mut Vec<u32>, reg: u32, value: u64) {
    for slot in 0..4 {
        words.push(
            (if slot == 0 { 0xd2800000 } else { 0xf2800000 })
                | (slot << 21)
                | ((((value >> (slot * 16)) & 65535) as u32) << 5)
                | reg,
        );
    }
}
fn elf(cases: &[(u32, u64)]) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 28, 0);
    for &(reg, pointer) in cases {
        immediate(&mut code, 27, 0x1000);
        code.push(0x9100037f);
        immediate(&mut code, 27, 0xb0000000);
        code.push(0xd51b421b);
        immediate(&mut code, reg, pointer);
        immediate(&mut code, 29, pointer.rotate_left(9) ^ 0x12345678);
        code.push(0xdac10400 | (29 << 5) | reg);
        code.push(0xdac143e0 | reg);
        code.push(0xf9000380 | reg);
        code.push(0xd53b421b);
        code.push(0xf900079b);
        code.push(0x910003fb);
        code.push(0xf9000b9b);
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
    let output = (TEXT + code.len() * 4 + 15) & !15;
    let address = BASE + output as u64;
    let mut patch = Vec::new();
    immediate(&mut patch, 28, address);
    code[..4].copy_from_slice(&patch);
    patch.clear();
    immediate(&mut patch, 1, address);
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
fn pacib_transports_all_registers_sp_aliases_budgets_and_faults() {
    let path = folder("pacib-transport");
    let words = words();
    let code = source(&words);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        for &w in &words {
            let f = unsafe {
                lib.get::<BudgetEntry>(format!("pacib_{w:x}_budget\0").as_bytes())
                    .unwrap()
            };
            let d = (w & 31) as usize;
            let n = ((w >> 5) & 31) as usize;
            for initial in 0..3 {
                for fail in [false, true] {
                    let mut p = Probe {
                        fail,
                        ..Probe::default()
                    };
                    let mut cpu = state();
                    let before = cpu.x;
                    let vectors = cpu.vregs;
                    cpu.pauth = Some(probe);
                    cpu.pac_context = std::ptr::addr_of_mut!(p).cast();
                    let mut budget = initial;
                    let expected_status = if initial == 0 {
                        2
                    } else if fail {
                        7
                    } else if initial == 1 {
                        2
                    } else {
                        0
                    };
                    assert_eq!(unsafe { f(&mut cpu, &mut budget) }, expected_status);
                    let mut want = before;
                    if initial > 0 && !fail && d < 31 {
                        want[d] = 0xabc0123456789def;
                    }
                    assert_eq!(cpu.x, want);
                    assert_eq!(p.calls, u32::from(initial > 0));
                    if initial > 0 {
                        assert_eq!(
                            (p.operation, p.key, p.pointer, p.modifier),
                            (
                                0,
                                1,
                                if d < 31 { before[d] } else { 0 },
                                if n < 31 { before[n] } else { 0x7788 }
                            )
                        );
                    }
                    assert_eq!(budget, if initial == 2 && fail { 1 } else { 0 });
                    assert_eq!(cpu.vregs, vectors);
                    assert_eq!(
                        (
                            cpu.sp,
                            cpu.n,
                            cpu.z,
                            cpu.c,
                            cpu.v,
                            cpu.fpcr,
                            cpu.fpsr,
                            cpu.fault_address
                        ),
                        (0x7788, 1, 0, 1, 1, 0x02c00000, 0x0800009f, 0xaabbccdd)
                    );
                    assert_eq!(cpu.fault_status, if initial > 0 && fail { 7 } else { 77 });
                    if initial == 0 || fail {
                        assert_eq!(cpu.pc, 0x2000);
                    } else if initial == 1 {
                        assert_eq!(cpu.pc, 0x2004);
                    }
                    count += 1;
                }
            }
            for missing_context in [false, true] {
                let mut p = Probe::default();
                let mut cpu = state();
                let before = cpu.x;
                cpu.pauth = if missing_context { Some(probe) } else { None };
                cpu.pac_context = if missing_context {
                    std::ptr::null_mut()
                } else {
                    std::ptr::addr_of_mut!(p).cast()
                };
                let mut budget = 2;
                assert_eq!(unsafe { f(&mut cpu, &mut budget) }, 3);
                assert_eq!(budget, 1);
                assert_eq!(cpu.x, before);
                assert_eq!(p.calls, 0);
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"transport_native_cases_both_optimizations":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn pacib_routes_checked_sign_to_existing_diagnostic_backend() {
    let path = folder("pacib-known-profile");
    let words = words();
    let code = source(&words);
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let back = pac_backend(&path, opt);
        let pac = *unsafe { back.get::<PAuth>(b"mkw_pac39\0") }.unwrap();
        let mut keys = Keys {
            high: [1, 3],
            low: [2, 4],
        };
        for &w in &words {
            let f = unsafe {
                lib.get::<Entry>(format!("pacib_{w:x}\0").as_bytes())
                    .unwrap()
            };
            let d = (w & 31) as usize;
            let n = ((w >> 5) & 31) as usize;
            for pointer in [
                0,
                1,
                (1u64 << 38) - 1,
                (1u64 << 39) - 1,
                0xffffffc000000000,
                u64::MAX,
            ] {
                let mut cpu = state();
                if d < 31 {
                    cpu.x[d] = pointer;
                }
                let before = cpu.x;
                let modifier = if n < 31 { before[n] } else { cpu.sp };
                let pointer = if d < 31 { before[d] } else { 0 };
                let mut result = 0;
                let context = std::ptr::addr_of_mut!(keys).cast();
                assert_eq!(
                    unsafe { pac(context, 0, 1, pointer, modifier, &mut result) },
                    0
                );
                let mut stripped = 0;
                assert_eq!(
                    unsafe { pac(context, 2, 1, result, modifier, &mut stripped) },
                    0
                );
                let mut authenticated = 0;
                assert_eq!(
                    unsafe { pac(context, 1, 1, result, modifier, &mut authenticated) },
                    0
                );
                assert_eq!(authenticated, stripped);
                let mut want = before;
                if d < 31 {
                    want[d] = result;
                }
                cpu.pauth = Some(pac);
                cpu.pac_context = context;
                assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
                assert_eq!(cpu.x, want);
                count += 1;
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"checked_backend_native_cases_both_optimizations":count,"profile":"legacy-QARMA5-39-bit-no-TBI diagnostic; not game verified","signature_transport_expected_from_existing_independently_QARMA_checked_backend":true,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn pacib_strip_roundtrip_matches_qemu_on_shared_canonical_addresses() {
    let path = folder("pacib-roundtrip-reference");
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
    for reg in [0, 1, 2, 30, 31] {
        for upper in 0..2 {
            for n in 0..64u64 {
                let low = (n * 0x1234567) & ((1 << 39) - 1);
                let pointer = if upper == 0 {
                    low
                } else {
                    low | !((1 << 39) - 1)
                };
                cases.push((reg, pointer));
            }
        }
    }
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, elf(&cases)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(&image)
        .current_dir(&path)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.len(), cases.len() * 24);
    let code: String = [0, 1, 2, 30, 31]
        .iter()
        .map(|&reg| {
            generate(
                &[0xdac10400 | (29 << 5) | reg, 0xdac143e0 | reg, 0xd65f03c0],
                &format!("pacib_roundtrip_{reg}"),
            )
        })
        .collect();
    for opt in ["-O0", "-O2"] {
        let lib = build(&code, &path, opt);
        let back = pac_backend(&path, opt);
        let pac = *unsafe { back.get::<PAuth>(b"mkw_pac39\0") }.unwrap();
        let mut keys = Keys {
            high: [1, 3],
            low: [2, 4],
        };
        for (i, &(reg, pointer)) in cases.iter().enumerate() {
            let f = unsafe {
                lib.get::<Entry>(format!("pacib_roundtrip_{reg}\0").as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                sp: 0x1000,
                n: 1,
                c: 1,
                v: 1,
                pauth: Some(pac),
                pac_context: std::ptr::addr_of_mut!(keys).cast(),
                ..Cpu::default()
            };
            if reg < 31 {
                cpu.x[reg as usize] = pointer;
            }
            cpu.x[29] = pointer.rotate_left(9) ^ 0x12345678;
            assert_eq!(unsafe { f(&mut cpu, 10) }, 0);
            let value = if reg < 31 { cpu.x[reg as usize] } else { 0 };
            assert_eq!(value, if reg < 31 { pointer } else { 0 });
            let actual: Vec<u8> = [
                value,
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
                cpu.sp,
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(actual, &result.stdout[i * 24..(i + 1) * 24]);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_roundtrip_cases":cases.len(),"signed_pointer_bits_not_compared_across_different_profiles":true,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
