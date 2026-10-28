use mkw_formats::{Mod0, Relocation};
use mkw_runtime::{module_pages::Plan, GuestAddr, LoadedModule, Memory, Permissions};
use std::collections::BTreeMap;
fn fixture() -> LoadedModule {
    let mut raw = vec![0u8; 320];
    raw[..4].copy_from_slice(b"NSO0");
    for (i, size) in [9u32, 7, 17].iter().enumerate() {
        for (at, value) in [
            (16 + i * 16, 256 + i as u32 * 20),
            (20 + i * 16, i as u32 * 0x2000),
            (24 + i * 16, *size),
        ] {
            raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    raw[0x3c..0x40].copy_from_slice(&4090u32.to_le_bytes());
    let mut nso = mkw_formats::Nso::parse(&raw).unwrap();
    for (i, s) in nso.segments.iter_mut().enumerate() {
        s.bytes.fill(0x21 + i as u8);
    }
    LoadedModule {
        name: "synthetic".into(),
        base: GuestAddr(0x10000),
        nso,
    }
}
#[test]
fn pristine_checks_are_read_only_and_allow_unrelated_stack_and_sealed_images() {
    let mut modules = [fixture()];
    modules[0].nso.segments[0].permissions = "--X".into();
    let plan = Plan::new(&modules).unwrap();
    let mut memory = plan.materialize().unwrap();
    memory
        .map(
            GuestAddr(0x90000),
            vec![0x55; 4096],
            Permissions::RW,
            "stack",
        )
        .unwrap();
    assert!(memory.read(GuestAddr(0x10000), 1).is_err());
    plan.verify_pristine(&memory).unwrap();
    memory.seal();
    plan.verify_pristine(&memory).unwrap();
    assert!(memory.read(GuestAddr(0x10000), 1).is_err());
    assert_eq!(
        memory.read(GuestAddr(0x90000), 4096).unwrap(),
        &[0x55; 4096]
    );
    // Equal independent images pass: this API is deliberately not a namespace token.
    plan.verify_pristine(&plan.materialize().unwrap()).unwrap();
}
#[test]
fn payload_bss_and_padding_changes_refuse_without_repairing_memory() {
    let modules = [fixture()];
    let plan = Plan::new(&modules).unwrap();
    for offset in [0usize, 16, 17, 4096, 4106, 4107, 8191] {
        let mut memory = plan.materialize().unwrap();
        let at = GuestAddr(0x14000 + offset as u64);
        let old = memory.read(at, 1).unwrap()[0];
        memory.write(at, &[old ^ 0xff]).unwrap();
        assert!(plan.verify_pristine(&memory).is_err());
        assert_eq!(memory.read(at, 1).unwrap(), &[old ^ 0xff]);
        memory.write(at, &[old]).unwrap();
        plan.verify_pristine(&memory).unwrap();
    }
    for segment in 0..3 {
        let mut altered = [fixture()];
        altered[0].nso.segments[segment].bytes[0] ^= 1;
        let memory = Plan::new(&altered).unwrap().materialize().unwrap();
        assert!(plan.verify_pristine(&memory).is_err());
    }
}
#[test]
fn missing_split_shifted_and_wrong_permission_mappings_refuse() {
    let modules = [fixture()];
    let plan = Plan::new(&modules).unwrap();
    for mode in 0..5 {
        let mut memory = Memory::default();
        for i in 0..3 {
            let s = &modules[0].nso.segments[i];
            let size = if i == 2 { 8192 } else { 4096 };
            let mut bytes = vec![0; size];
            bytes[..s.size].copy_from_slice(&s.bytes);
            let permissions = [Permissions::RX, Permissions::R, Permissions::RW][i];
            let base = 0x10000 + s.memory_offset as u64;
            if i == 0 {
                match mode {
                    0 => continue,
                    1 => {
                        memory
                            .map(
                                GuestAddr(base),
                                bytes[..2048].to_vec(),
                                permissions,
                                "split1",
                            )
                            .unwrap();
                        memory
                            .map(
                                GuestAddr(base + 2048),
                                bytes[2048..].to_vec(),
                                permissions,
                                "split2",
                            )
                            .unwrap();
                        continue;
                    }
                    2 => {
                        memory
                            .map(GuestAddr(base), bytes, Permissions::RW, "wrong-permissions")
                            .unwrap();
                        continue;
                    }
                    3 => {
                        memory
                            .map(GuestAddr(base - 1), bytes, permissions, "shifted")
                            .unwrap();
                        continue;
                    }
                    _ => {
                        bytes.push(0);
                    }
                }
            }
            memory
                .map(GuestAddr(base), bytes, permissions, "mapping")
                .unwrap();
        }
        assert!(plan.verify_pristine(&memory).is_err());
    }
}
#[test]
fn relocation_changes_reject_and_replanning_relr_is_not_idempotent() {
    let mut modules = [fixture()];
    modules[0].nso.mod0 = Some(Mod0 {
        offset: 0,
        dynamic: 0,
        bss_start: 0,
        bss_end: 0,
        eh_start: 0,
        eh_end: 0,
        module_object: 0,
        tags: BTreeMap::new(),
        needed: vec![],
        symbols: vec![],
        relr_offsets: vec![0x4000],
        relocations: vec![Relocation {
            offset: 0x4008,
            kind: 1027,
            symbol: 0,
            addend: 0x77,
        }],
        relocation_counts: BTreeMap::new(),
    });
    let plan = Plan::new(&modules).unwrap();
    let mut memory = plan.materialize().unwrap();
    plan.verify_pristine(&memory).unwrap();
    let before = memory.read_u64(GuestAddr(0x14000)).unwrap();
    let relocation = mkw_runtime::plan_relocations(&modules, &memory).unwrap();
    relocation.apply(&mut memory).unwrap();
    relocation.verify_applied(&memory).unwrap();
    assert!(plan.verify_pristine(&memory).is_err());
    let after = memory.read_u64(GuestAddr(0x14000)).unwrap();
    assert_eq!(after, before.wrapping_add(0x10000));
    assert_eq!(memory.read_u64(GuestAddr(0x14008)).unwrap(), 0x10077);
    relocation.apply(&mut memory).unwrap();
    assert_eq!(memory.read_u64(GuestAddr(0x14000)).unwrap(), after);
    let second = mkw_runtime::plan_relocations(&modules, &memory).unwrap();
    second.apply(&mut memory).unwrap();
    assert_eq!(
        memory.read_u64(GuestAddr(0x14000)).unwrap(),
        after.wrapping_add(0x10000)
    );
    assert_eq!(memory.read_u64(GuestAddr(0x14008)).unwrap(), 0x10077);
    assert!(plan.verify_pristine(&memory).is_err());
}
