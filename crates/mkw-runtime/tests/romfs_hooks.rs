#![cfg(target_os = "linux")]
use mkw_formats::{Mod0, Nso, Relocation, Symbol};
use mkw_runtime::{
    map_modules, plan_relocations, romfs_hle::SYMBOLS, romfs_hooks::checked_hooks, GuestAddr,
    LoadedModule, Memory, RelocationPlan,
};
use std::collections::BTreeMap;
fn modules() -> Vec<LoadedModule> {
    let mut raw = vec![0; 640];
    raw[..4].copy_from_slice(b"NSO0");
    for (i, (offset, size)) in [(256u32, 256u32), (512, 64), (576, 64)]
        .into_iter()
        .enumerate()
    {
        for (at, value) in [
            (16 + i * 16, offset),
            (20 + i * 16, (i as u32) * 0x1000),
            (24 + i * 16, size),
        ] {
            raw[at..at + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    let mut nso = Nso::parse(&raw).unwrap();
    let mut symbols = vec![Symbol {
        name: String::new(),
        value: 0,
        size: 0,
        info: 0,
        other: 0,
        section: 0,
    }];
    let mut relocations = Vec::new();
    for (i, name) in SYMBOLS.iter().enumerate() {
        symbols.push(Symbol {
            name: (*name).into(),
            value: 16 * (i as u64 + 1),
            size: 16,
            info: 0x12,
            other: 0,
            section: 1,
        });
        relocations.push(Relocation {
            offset: 0x2000 + i as u64 * 8,
            kind: 1026,
            symbol: i as u32 + 1,
            addend: 0,
        });
    }
    nso.mod0 = Some(Mod0 {
        offset: 0,
        dynamic: 0,
        bss_start: 0,
        bss_end: 0,
        eh_start: 0,
        eh_end: 0,
        module_object: 0,
        tags: BTreeMap::new(),
        needed: vec![],
        symbols,
        relocations,
        relocation_counts: BTreeMap::new(),
        relr_offsets: vec![],
    });
    vec![LoadedModule {
        name: "original-synthetic".into(),
        base: GuestAddr(0x100000),
        nso,
    }]
}
fn link(modules: &[LoadedModule]) -> (Memory, RelocationPlan) {
    let mut memory = map_modules(modules).unwrap();
    let plan = plan_relocations(modules, &memory).unwrap();
    plan.apply(&mut memory).unwrap();
    (memory, plan)
}
#[test]
fn selected_hle_entries_require_exact_symbols_executable_ranges_and_readback() {
    let modules = modules();
    let (mut memory, mut plan) = link(&modules);
    let hooks = checked_hooks(&modules, &plan, &memory).unwrap();
    assert_eq!(hooks.len(), 5);
    for (i, hook) in hooks.iter().enumerate() {
        assert_eq!(hook.symbol(), SYMBOLS[i]);
        assert_eq!(hook.address(), GuestAddr(0x100010 + i as u64 * 16));
        assert_eq!(hook.binding_sites(), 1);
    }
    memory
        .write(GuestAddr(0x102000), &0u64.to_le_bytes())
        .unwrap();
    assert!(checked_hooks(&modules, &plan, &memory).is_err());
    plan.apply(&mut memory).unwrap();
    plan.bindings[0].provider = None;
    assert!(checked_hooks(&modules, &plan, &memory).is_err());
}
#[test]
fn malformed_hle_profile_metadata_addends_aliases_and_missing_hooks_refuse() {
    for problem in 0..8 {
        let mut modules = modules();
        match problem {
            0 => modules[0].nso.sha256 = "not-a-sha256".into(),
            1 => modules[0].nso.segments[0].permissions = "R--".into(),
            2 => modules[0].nso.mod0.as_mut().unwrap().relocations[0].addend = 4,
            3 => modules[0].nso.mod0.as_mut().unwrap().symbols[1].info = 0x11,
            4 => modules[0].nso.mod0.as_mut().unwrap().symbols[2].value = 16,
            5 => {
                modules[0].nso.mod0.as_mut().unwrap().relocations.pop();
            }
            6 => modules[0].base = GuestAddr(0x100001),
            _ => modules[0].nso.mod0.as_mut().unwrap().symbols[1].value = 0x1000,
        };
        let (memory, mut plan) = link(&modules);
        assert!(
            checked_hooks(&modules, &plan, &memory).is_err(),
            "problem {problem}"
        );
        plan.bindings.clear();
        assert!(checked_hooks(&modules, &plan, &memory).is_err());
    }
    let modules = modules();
    let (memory, mut plan) = link(&modules);
    let b = &plan.bindings[0];
    let duplicate = mkw_runtime::SymbolBinding {
        consumer: b.consumer,
        provider: b.provider,
        symbol: b.symbol.clone(),
        destination: b.destination,
        value: b.value,
    };
    plan.bindings.push(duplicate);
    assert!(checked_hooks(&modules, &plan, &memory).is_err());
}
