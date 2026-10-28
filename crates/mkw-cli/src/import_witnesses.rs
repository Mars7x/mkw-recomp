//! Read-only diagnostic relocation correlation, never a runtime terminal fact.
use mkw_arm64::terminal::Image;
use mkw_runtime::GuestAddr;
use serde_json::json;
use std::{collections::BTreeMap, fs, path::Path};
pub fn run(root: &Path) -> Result<(), String> {
    let (modules, memory, plan) = super::pipeline::prepare_link(root)?;
    let mut reports = Vec::new();
    let mut details = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let nso = &module.nso;
        let text = &nso.segments[0];
        let base = text.memory_offset as u64;
        let image = Image::new(base, text.bytes.clone())
            .map_err(|e| format!("import witness image:{e:?}"))?;
        let mod0 = nso.mod0.as_ref().ok_or("missing import metadata")?;
        let mut relocs = BTreeMap::new();
        for relocation in &mod0.relocations {
            if relocation.kind == 1026
                && relocation.addend == 0
                && relocs.insert(relocation.offset, relocation).is_some()
            {
                return Err("duplicate zero-addend JUMP_SLOT".into());
            }
        }
        let mut bindings = BTreeMap::new();
        for binding in plan.bindings.iter().filter(|b| b.consumer == index) {
            if bindings.insert(binding.destination, binding).is_some() {
                return Err("duplicate diagnostic binding".into());
            }
        }
        let mut canonical = 0;
        let mut matched = 0;
        let mut providers = 0;
        for (offset, bytes) in text.bytes.chunks_exact(4).enumerate() {
            let word = u32::from_le_bytes(bytes.try_into().unwrap());
            if word & 0x9f00001f != 0x90000010 {
                continue;
            }
            let Ok(witness) = image.import_thunk(base + offset as u64 * 4, 0) else {
                continue;
            };
            canonical += 1;
            let Some(relocation) = relocs.get(&witness.slot()) else {
                continue;
            };
            let symbol = mod0
                .symbols
                .get(relocation.symbol as usize)
                .ok_or("invalid import symbol")?;
            let destination = module
                .base
                .0
                .checked_add(witness.slot())
                .ok_or("import slot overflow")?;
            let binding = bindings
                .get(&destination)
                .ok_or("canonical relocation lacks diagnostic binding")?;
            if binding.symbol != symbol.name
                || memory.read_u64(GuestAddr(destination))? != binding.value
            {
                return Err("import witness/relocation/binding/readback disagreement".into());
            }
            matched += 1;
            providers += usize::from(binding.provider.is_some());
            details.push(json!({"module_private":module.name,"source_sha256_private":nso.sha256,
                "sources_private":witness.sources(),"slot_private":witness.slot(),"symbol_private":symbol.name,
                "diagnostic_binding_private":binding,"runtime_target_attested":false}));
        }
        reports.push(json!({"module":module.name,"canonical_thunks":canonical,
            "zero_addend_jump_slot_correlations":matched,"diagnostic_selected_providers":providers}));
    }
    let out = Path::new("private/analysis");
    fs::write(
        out.join("import-witness-details.json"),
        serde_json::to_vec_pretty(&details).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(out.join("import-witness-summary.json"),serde_json::to_vec_pretty(&json!({"modules":reports,
        "compiler_acceptance_changed":false,"game_code_generated":false,"game_code_executed":false,
        "runtime_guards_installed":false,"target_profile_verified":false,"terminal_import_facts_supported":false,
        "classification":"exact immutable source plus diagnostic initial relocation correlations; no immutable binding or terminal call effect"})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("Import thunk source witnesses and diagnostic relocation readback checked; no terminal facts, admission change or original execution.");
    Ok(())
}
