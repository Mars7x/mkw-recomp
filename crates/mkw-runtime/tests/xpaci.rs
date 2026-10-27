use mkw_runtime::abi::{Cpu, PAuth};
use std::{path::Path, process::Command};
#[path = "support/fp_native.rs"]
mod support;
use support::*;
type BudgetEntry = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
#[repr(C)]
struct Keys {
    high: [u64; 2],
    low: [u64; 2],
}
fn source() -> String {
    (0..32)
        .map(|n| generate(&[0xdac143e0 | n, 0xd65f03c0], &format!("xpaci_{n}")))
        .collect()
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
fn strip(pointer: u64) -> u64 {
    let low = pointer & ((1 << 39) - 1);
    if pointer & (1 << 55) == 0 {
        low
    } else {
        low | !((1 << 39) - 1)
    }
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
    c.x = std::array::from_fn(|n| 0xdeadbeef00000000 | n as u64);
    c.x[30] = 0x5000;
    c.vregs = std::array::from_fn(|n| [n as u64 + 17, n as u64 + 93]);
    c
}
#[repr(C)]
struct Probe {
    operation: u32,
    key: u32,
    pointer: u64,
    modifier: u64,
    calls: u32,
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
    let p = unsafe { &mut *context.cast::<Probe>() };
    p.calls += 1;
    p.operation = operation;
    p.key = key;
    p.pointer = pointer;
    p.modifier = modifier;
    unsafe {
        *result = 0x123456789abcdef0;
    }
    if p.fail {
        7
    } else {
        0
    }
}
#[test]
fn xpaci_native_configured_strip_state_budget_and_failure() {
    let path = folder("xpaci-known");
    let code = source();
    let mut pointers = vec![
        0,
        u64::MAX,
        (1 << 39) - 1,
        1 << 39,
        1 << 55,
        1 << 63,
        0xffffff8000002000,
    ];
    for bit in 0..64 {
        pointers.push(1 << bit);
        pointers.push(!(1 << bit));
    }
    let mut random = 0x8c4af101cab87365u64;
    for _ in 0..1024 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        pointers.push(random);
    }
    let mut count = 0;
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let pac_lib = pac_backend(&path, opt);
        let pac = *unsafe { pac_lib.get::<PAuth>(b"mkw_pac39\0") }.unwrap();
        let mut keys = Keys {
            high: [1, 3],
            low: [2, 4],
        };
        let mut observed = 0;
        for n in 0..32usize {
            let native = unsafe {
                library
                    .get::<BudgetEntry>(format!("xpaci_{n}_budget\0").as_bytes())
                    .unwrap()
            };
            for &pointer in &pointers {
                for initial in 0..3u64 {
                    let mut c = state();
                    if n < 31 {
                        c.x[n] = pointer;
                    }
                    let before = c.x;
                    let vectors = c.vregs;
                    c.pauth = Some(pac);
                    c.pac_context = std::ptr::addr_of_mut!(keys).cast();
                    let mut remaining = initial;
                    let rc = unsafe { native(&mut c, &mut remaining) };
                    assert_eq!(rc, if initial < 2 { 2 } else { 0 });
                    assert_eq!(remaining, 0);
                    let mut expected = before;
                    if initial != 0 && n < 31 {
                        expected[n] = strip(pointer);
                    }
                    assert_eq!(c.x, expected, "{opt} r{n} {pointer:x}/{initial}");
                    assert_eq!(
                        c.pc,
                        match initial {
                            0 => 0x2000,
                            1 => 0x2004,
                            _ => expected[30],
                        }
                    );
                    assert_eq!(c.vregs, vectors);
                    assert_eq!(
                        (
                            c.sp,
                            c.n,
                            c.z,
                            c.c,
                            c.v,
                            c.fpcr,
                            c.fpsr,
                            c.fault_status,
                            c.fault_address
                        ),
                        (0x7788, 1, 0, 1, 1, 0x02c00000, 0x0800009f, 77, 0xaabbccdd)
                    );
                    observed += 1;
                }
            }
            for fail in [false, true] {
                let mut p = Probe {
                    operation: 99,
                    key: 99,
                    pointer: 99,
                    modifier: 99,
                    calls: 0,
                    fail,
                };
                let mut c = state();
                let before = c.x;
                c.pauth = Some(probe);
                c.pac_context = std::ptr::addr_of_mut!(p).cast();
                let mut budget = 1;
                assert_eq!(
                    unsafe { native(&mut c, &mut budget) },
                    if fail { 7 } else { 2 }
                );
                assert_eq!(
                    (p.calls, p.operation, p.key, p.pointer, p.modifier),
                    (1, 2, 0, if n < 31 { before[n] } else { 0 }, 0)
                );
                let mut expected = before;
                if !fail && n < 31 {
                    expected[n] = 0x123456789abcdef0;
                }
                assert_eq!(c.x, expected);
                assert_eq!(c.pc, if fail { 0x2000 } else { 0x2004 });
                assert_eq!(budget, 0);
            }
            for missing in [true, false] {
                let mut c = state();
                if !missing {
                    c.pauth = Some(pac);
                }
                let before = c.x;
                let mut budget = 2;
                assert_eq!(unsafe { native(&mut c, &mut budget) }, 3);
                assert_eq!(budget, 1);
                assert_eq!(c.x, before);
                assert_eq!(c.pc, 0x2000);
            }
        }
        if count == 0 {
            count = observed;
        } else {
            assert_eq!(observed, count);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":count,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"profile":"explicit diagnostic legacy instruction-PAC 39-bit/no-TBI; actual game profile unverified","game_code_executed":false})).unwrap()).unwrap();
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
fn xpaci_qemu_transport_on_shared_canonical_addresses() {
    let path = folder("xpaci-reference");
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
            for i in 0..128u64 {
                let low = (i * 0x1234567) & ((1 << 39) - 1);
                let canonical = if upper == 0 {
                    low
                } else {
                    low | !((1 << 39) - 1)
                };
                let mask = 127 << 48;
                let pointer = (canonical & !mask) | (i << 48);
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
    let code = source();
    for opt in ["-O0", "-O2"] {
        let library = build(&code, &path, opt);
        let pac_lib = pac_backend(&path, opt);
        let pac = *unsafe { pac_lib.get::<PAuth>(b"mkw_pac39\0") }.unwrap();
        let mut keys = Keys {
            high: [1, 3],
            low: [2, 4],
        };
        for (i, &(reg, pointer)) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("xpaci_{reg}\0").as_bytes())
                    .unwrap()
            };
            let mut c = state();
            c.sp = 0x1000;
            if reg < 31 {
                c.x[reg as usize] = pointer;
            }
            c.pauth = Some(pac);
            c.pac_context = std::ptr::addr_of_mut!(keys).cast();
            assert_eq!(unsafe { native(&mut c, 1) }, 2);
            let value = if reg < 31 { c.x[reg as usize] } else { 0 };
            assert_eq!(value, if reg < 31 { strip(pointer) } else { 0 });
            let actual: Vec<u8> = [
                value,
                u64::from((c.n << 31) | (c.z << 30) | (c.c << 29) | (c.v << 28)),
                c.sp,
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(
                actual,
                &result.stdout[i * 24..(i + 1) * 24],
                "{opt} r{reg} {pointer:x}"
            );
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"shared_39_and_48_bit_canonical_addresses_only":true,"top_byte_kept_canonical_for_qemu_tbi_profile":true,"hardware_or_game_pac_profile_verified":false,"game_code_executed":false})).unwrap()).unwrap();
}

#[test]
fn xpaci_qemu_tag_byte_demonstrates_distinct_profile() {
    let path = folder("xpaci-profile-difference");
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
    let pointer = 0x4901000001234567;
    let image = path.join("synthetic-aarch64");
    std::fs::write(&image, elf(&[(0, pointer)])).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(&image)
        .current_dir(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout.len(), 24);
    let qemu = u64::from_le_bytes(result.stdout[..8].try_into().unwrap());
    assert_eq!(qemu, 0x4900000001234567);
    for opt in ["-O0", "-O2"] {
        let library = pac_backend(&path, opt);
        let callback = *unsafe { library.get::<PAuth>(b"mkw_pac39\0") }.unwrap();
        let mut keys = Keys {
            high: [1, 3],
            low: [2, 4],
        };
        let mut native = 0;
        assert_eq!(
            unsafe {
                callback(
                    std::ptr::addr_of_mut!(keys).cast(),
                    2,
                    0,
                    pointer,
                    0,
                    &mut native,
                )
            },
            0
        );
        assert_eq!(native, 0x1234567);
        assert_ne!(native, qemu);
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"qemu_preserves_top_tag_byte":true,"diagnostic_no_tbi_strips_top_byte":true,"profiles_intentionally_distinct":true,"game_profile_verified":false,"implementation_changed_to_match_qemu":false,"native_optimizations":["O0","O2"],"game_code_executed":false})).unwrap()).unwrap();
}
