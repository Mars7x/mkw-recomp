use mkw_runtime::abi::Cpu;
use std::process::Command;
#[path = "support/fp_native.rs"]
mod support;
use support::*;
#[path = "../../mkw-codegen/tests/support/a64_reference.rs"]
mod reference;
fn words(full: bool) -> Vec<u32> {
    let regs = if full {
        vec![(0, 1), (0, 0), (31, 31), (31, 1)]
    } else {
        vec![(0, 1), (0, 0)]
    };
    regs.into_iter()
        .map(|(d, n)| 0x2ea04800 | (n << 5) | d)
        .collect()
}
fn code(words: &[u32]) -> String {
    words
        .iter()
        .map(|w| generate(&[*w, 0xd65f03c0], &format!("vector_clz_{w:x}")))
        .collect()
}
fn frame(word: u32, a: u32, b: u32) -> [[u64; 2]; 32] {
    let mut f = std::array::from_fn(|n| [n as u64 + 17, 0xdeadbeef00000000 | n as u64]);
    f[((word >> 5) & 31) as usize] = [u64::from(a) | (u64::from(b) << 32), u64::MAX];
    f
}
#[test]
fn two_s_clz_matches_independent_leading_zero_oracle() {
    let path = folder("vector-clz-known");
    let words = words(true);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        for &w in &words {
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_clz_{w:x}\0").as_bytes())
                    .unwrap()
            };
            for i in 0..65536u32 {
                let a = i;
                let b = i << 16;
                let before = frame(w, a, b);
                let mut cpu = Cpu {
                    sp: 99,
                    n: 1,
                    c: 1,
                    v: 1,
                    fpcr: u32::MAX,
                    fpsr: 0x0800009f,
                    fault_status: 77,
                    fault_address: 0x1234,
                    vregs: before,
                    ..Cpu::default()
                };
                cpu.x = std::array::from_fn(|n| n as u64 + 123);
                let x = cpu.x;
                let mut expected = before;
                expected[(w & 31) as usize] = [
                    u64::from(a.leading_zeros()) | (u64::from(b.leading_zeros()) << 32),
                    0,
                ];
                assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
                assert_eq!(cpu.vregs, expected, "{opt} {w:x} {i:x}");
                assert_eq!(cpu.x, x);
                assert_eq!(
                    (
                        cpu.sp,
                        cpu.n,
                        cpu.z,
                        cpu.c,
                        cpu.v,
                        cpu.fpcr,
                        cpu.fpsr,
                        cpu.fault_status,
                        cpu.fault_address
                    ),
                    (99, 1, 0, 1, 1, u32::MAX, 0x0800009f, 77, 0x1234)
                );
            }
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"native_cases_per_optimization":words.len()*65536,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false})).unwrap()).unwrap();
}
#[test]
fn two_s_clz_matches_synthetic_qemu() {
    let path = folder("vector-clz-reference");
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
    let words = words(false);
    let mut inputs = vec![0, u32::MAX];
    for bit in 0..32 {
        inputs.push(1 << bit);
        inputs.push(!(1 << bit));
    }
    let mut state = 0x12345678u32;
    for _ in 0..256 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        inputs.push(state);
    }
    let mut cases = Vec::new();
    for &w in &words {
        for &a in &inputs {
            for &b in &[0, u32::MAX, a.rotate_left(13)] {
                for qc in 0..2 {
                    cases.push(reference::VectorCase {
                        word: w,
                        vectors: frame(w, a, b)[..4].try_into().unwrap(),
                        fpcr: 0x02c00000,
                        fpsr: 0x9f | (qc << 27),
                        nzcv: 0xb0000000,
                    });
                }
            }
        }
    }
    let mut oracle = Vec::new();
    use std::os::unix::fs::PermissionsExt;
    for (i, chunk) in cases.chunks(1500).enumerate() {
        let image = path.join(format!("synthetic-aarch64-{i}"));
        std::fs::write(&image, reference::vector_elf(chunk)).unwrap();
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
        oracle.extend(result.stdout);
    }
    assert_eq!(oracle.len(), cases.len() * 32);
    let source = code(&words);
    for opt in ["-O0", "-O2"] {
        let library = build(&source, &path, opt);
        for (i, c) in cases.iter().enumerate() {
            let native = unsafe {
                library
                    .get::<Entry>(format!("vector_clz_{:x}\0", c.word).as_bytes())
                    .unwrap()
            };
            let mut cpu = Cpu {
                n: 1,
                c: 1,
                v: 1,
                fpcr: c.fpcr,
                fpsr: c.fpsr,
                ..Cpu::default()
            };
            cpu.vregs[..4].copy_from_slice(&c.vectors);
            let before = cpu.vregs;
            assert_eq!(unsafe { native(&mut cpu, 10) }, 0);
            let actual: Vec<u8> = [
                cpu.vregs[0][0],
                cpu.vregs[0][1],
                u64::from(cpu.fpsr),
                u64::from((cpu.n << 31) | (cpu.z << 30) | (cpu.c << 29) | (cpu.v << 28)),
            ]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
            assert_eq!(actual, &oracle[i * 32..(i + 1) * 32], "{opt} {i}");
            assert_eq!(cpu.vregs[1..], before[1..]);
            assert_eq!(cpu.fpcr, c.fpcr);
        }
    }
    std::fs::write(path.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"native_optimizations":["O0","O2"],"fatal_ubsan":true,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
