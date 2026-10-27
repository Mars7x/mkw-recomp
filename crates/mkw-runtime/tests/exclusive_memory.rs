use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    GuestAddr, Memory, Permissions,
};
use std::{path::Path, process::Command};
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
fn exclusive_memory_sequences_match_synthetic_qemu() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/exclusive-memory-reference");
    std::fs::create_dir_all(&root).unwrap();
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            root.join("receipt.json"),
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
    let elf = root.join("synthetic-aarch64");
    std::fs::write(&elf, image(&cases)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .arg(&elf)
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.len(), cases.len() * 24);
    let mut memory = Memory::default();
    memory
        .map(GuestAddr(0x1000), vec![0; 64], Permissions::RW, "synthetic")
        .unwrap();
    let mut memory = ExclusiveMemory::new(
        memory,
        Config {
            granule: 16,
            max_owners: 1,
        },
    )
    .unwrap();
    let owner = memory.new_owner().unwrap();
    for (i, c) in cases.iter().enumerate() {
        let bytes = 1usize << c.size;
        let width = Width::try_from(bytes).unwrap();
        memory
            .write(GuestAddr(0x1000), &c.initial.to_le_bytes()[..bytes])
            .unwrap();
        let loaded = memory.load(&owner, GuestAddr(0x1000), width).unwrap();
        match c.interference {
            0 => {}
            1 => memory.clear(&owner).unwrap(),
            2 => memory
                .write(GuestAddr(0x1000), &(c.initial ^ 1).to_le_bytes()[..bytes])
                .unwrap(),
            _ => panic!(),
        };
        let success = memory
            .store_conditional(&owner, GuestAddr(0x1000), width, u128::from(c.desired))
            .unwrap();
        let value = memory.read(GuestAddr(0x1000), width).unwrap();
        let actual: Vec<u8> = [loaded as u64, u64::from(!success), value as u64]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect();
        assert_eq!(
            actual,
            &result.stdout[i * 24..(i + 1) * 24],
            "case{i} size{} order{} interference{}",
            c.size,
            c.ordered,
            c.interference
        );
    }
    std::fs::write(root.join("receipt.json"),serde_json::to_vec_pretty(&serde_json::json!({"synthetic_cases":cases.len(),"widths":[1,2,4,8],"orders":["plain","acquire-release"],"events":["success","explicit-clear","changed-value-interference"],"diagnostic_rust_memory_model_only":true,"cpu_abi_binding":false,"target_configuration_verified":false,"hardware_reference":false,"game_code_executed":false})).unwrap()).unwrap();
}
