//! Same-image conditional source theorems. Premises are NEVER runtime-attested.
use mkw_arm64::terminal::{
    conditional::{model_fingerprint, Effect},
    import::ImportThunk,
    Image,
};
use mkw_formats::unwind;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
struct Body {
    entry: u64,
    bytes: usize,
    targets: BTreeSet<u64>,
}
struct Alias {
    thunk: ImportThunk,
    provider: u64,
}
pub fn run(root: &Path) -> Result<(), String> {
    run_mode(root, false)
}
pub fn run_guarded(root: &Path) -> Result<(), String> {
    run_mode(root, true)
}
fn run_mode(root: &Path, guarded: bool) -> Result<(), String> {
    let mut guarded_report = guarded.then(super::guarded_survey::Report::new);
    super::terminal_proofs::run(root)?;
    let seeds: serde_json::Value = serde_json::from_slice(
        &fs::read("private/analysis/terminal-source-proof-details.json")
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let (modules, memory, plan) = super::pipeline::prepare_link(root)?;
    let mut reports = Vec::new();
    let mut details = Vec::new();
    let mut fact_details = Vec::new();
    for (index, module) in modules.iter().enumerate() {
        let text = &module.nso.segments[0];
        let base = text.memory_offset as u64;
        let image =
            Image::new(base, text.bytes.clone()).map_err(|e| format!("conditional image:{e:?}"))?;
        let mut facts = BTreeMap::<u64, Effect>::new();
        for seed in seeds
            .as_array()
            .ok_or("source proof details must be an array")?
            .iter()
            .filter(|s| s["module_private"] == module.name)
        {
            if seed["source_sha256_private"] != module.nso.sha256 {
                return Err("conditional seed/source identity differs".into());
            }
            let entry = seed["extent_private"]["start"]
                .as_u64()
                .ok_or("invalid seed entry")?;
            let bytes = usize::try_from(
                seed["extent_private"]["size"]
                    .as_u64()
                    .ok_or("invalid seed size")?,
            )
            .map_err(|e| e.to_string())?;
            let local = image
                .analyze(entry, bytes, &[])
                .map_err(|e| format!("seed:{e:?}"))?
                .no_return_fact()
                .ok_or("conditional seed lacks unconditional evidence")?;
            if seed["proof_digest_private"] != mkw_formats::hex(&local.proof_digest()) {
                return Err("conditional seed derivation differs".into());
            }
            facts.insert(entry, Effect::from_local(&local));
        }
        let seed_count = facts.len();
        let extents = unwind::functions(&module.nso)?;
        let mut rejected = Vec::new();
        let mut starts = BTreeSet::new();
        let mut oversized = 0;
        for e in &extents {
            if e.size > 1024 * 1024 {
                oversized += 1;
                continue;
            }
            let b = module.nso.read(e.start, e.size)?;
            if mkw_arm64::lift_function(b, e.start as u64, e.start as u64).is_ok() {
                continue;
            }
            let mut targets = BTreeSet::new();
            for (n, b) in b.chunks_exact(4).enumerate() {
                let w = u32::from_le_bytes(b.try_into().unwrap());
                if matches!(w & 0xfc000000, 0x14000000 | 0x94000000) {
                    let delta = ((w & 0x3ffffff) as i64) << 38 >> 36;
                    if let Some(target) = (e.start as u64 + n as u64 * 4).checked_add_signed(delta)
                    {
                        targets.insert(target);
                        starts.insert(target);
                    }
                }
            }
            rejected.push(Body {
                entry: e.start as u64,
                bytes: e.size,
                targets,
            });
        }
        let metadata = module
            .nso
            .mod0
            .as_ref()
            .ok_or("missing conditional import metadata")?;
        let relocs = metadata
            .relocations
            .iter()
            .filter(|r| r.kind == 1026 && r.addend == 0)
            .map(|r| (r.offset, r))
            .collect::<BTreeMap<_, _>>();
        let bindings = plan
            .bindings
            .iter()
            .filter(|b| b.consumer == index && b.provider == Some(index))
            .map(|b| (b.destination, b))
            .collect::<BTreeMap<_, _>>();
        let mut aliases = Vec::new();
        for entry in starts {
            let Ok(thunk) = image.import_thunk(entry, 8) else {
                continue;
            };
            let Some(relocation) = relocs.get(&thunk.slot()) else {
                continue;
            };
            let destination = module
                .base
                .0
                .checked_add(thunk.slot())
                .ok_or("conditional import overflow")?;
            let Some(binding) = bindings.get(&destination) else {
                continue;
            };
            let symbol = metadata
                .symbols
                .get(relocation.symbol as usize)
                .ok_or("conditional symbol out of range")?;
            if binding.symbol != symbol.name
                || memory.read_u64(mkw_runtime::GuestAddr(destination))? != binding.value
            {
                return Err("conditional import selection/readback disagrees".into());
            }
            let provider = binding
                .value
                .checked_sub(module.base.0)
                .ok_or("conditional provider base")?;
            aliases.push(Alias { thunk, provider });
        }
        let mut final_closed = BTreeMap::new();
        let mut passes = 0;
        loop {
            passes += 1;
            if passes > 64 {
                return Err("conditional source fixed point limit".into());
            }
            let before = facts.len();
            let additions = aliases
                .iter()
                .filter(|a| !facts.contains_key(&a.thunk.entry()))
                .filter_map(|a| facts.get(&a.provider).map(|p| (a, p)))
                .map(|(a, p)| {
                    image
                        .conditional_import(&a.thunk, p)
                        .map(|f| (a.thunk.entry(), f))
                        .map_err(|e| format!("conditional alias:{e:?}"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            for (entry, fact) in additions {
                facts.insert(entry, fact);
            }
            let mut additions = Vec::new();
            final_closed.clear();
            for body in &rejected {
                let needed = body
                    .targets
                    .iter()
                    .filter(|t| **t != body.entry)
                    .filter_map(|t| facts.get(t))
                    .cloned()
                    .collect::<Vec<_>>();
                let a = image
                    .analyze_conditionally(body.entry, body.bytes, &needed)
                    .map_err(|e| format!("conditional control:{e:?}"))?;
                if a.is_closed_conditionally() {
                    let no_return = a
                        .no_return_effect()
                        .map_err(|e| format!("conditional derivation:{e:?}"))?;
                    final_closed.insert(body.entry,json!({"module_private":module.name,"entry_private":body.entry,"bytes":body.bytes,"conditional_no_return":no_return.is_some(),"undischarged_requirements_private":a.requirements().iter().map(|r|json!({"thunk_sources":r.thunk().sources(),"slot":r.thunk().slot(),"provider_source":r.provider()})).collect::<Vec<_>>()}));
                    if let Some(f) = no_return {
                        if !facts.contains_key(&body.entry) {
                            additions.push((body.entry, f));
                        }
                    }
                }
            }
            facts.extend(additions);
            if facts.len() == before {
                break;
            }
        }
        if let Some(report) = &mut guarded_report {
            report.module(module, &image, &facts, &final_closed)?;
        }
        let aliases_count = facts
            .values()
            .filter(|f| f.kind() == mkw_arm64::terminal::conditional::Kind::Import)
            .count();
        for fact in facts.values() {
            fact_details.push(json!({"module_private":module.name,"source_private":fact.source(),"digest_private":mkw_formats::hex(&fact.digest()),"kind":format!("{:?}",fact.kind()),"dependencies_private":fact.dependencies().iter().map(|d|mkw_formats::hex(&d.digest())).collect::<Vec<_>>(),"undischarged_requirements":fact.requirements().len()}));
        }
        reports.push(json!({"module":module.name,"indexed_extents":extents.len(),"bounded_strict_rejects":rejected.len(),"oversized_unanalyzed":oversized,"unconditional_seed_facts":seed_count,"conditional_aliases":aliases_count,"conditional_control_facts":facts.len()-aliases_count-seed_count,"conditionally_closed_rejected_graphs":final_closed.len(),"passes":passes}));
        details.extend(final_closed.into_values());
    }
    let out = Path::new("private/analysis");
    if !guarded {
        for (name, value) in [
            ("conditional-source-details.json", json!(details)),
            ("conditional-source-facts.json", json!(fact_details)),
            (
                "conditional-source-summary.json",
                json!({"classification":"same-image source theorems under explicit UNDISCHARGED runtime premises, not compiler admission or runnable coverage","model_fingerprint":mkw_formats::hex(&model_fingerprint()),"modules":reports,"compiler_acceptance_changed":false,"game_code_generated":false,"game_code_executed":false,"runtime_guards_installed":false,"target_profile_verified":false,"runtime_premises_discharged":false,"cross_image_imports_supported":false}),
            ),
        ] {
            fs::write(
                out.join(name),
                serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
    }
    if let Some(report) = guarded_report {
        report.finish()?;
    }
    if !guarded {
        println!("Same-image conditional source evidence checked; every runtime premise remains undischarged; no admission change or original execution.");
    }
    Ok(())
}
