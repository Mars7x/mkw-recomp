//! Explicit guarded-v4 compiler survey and bounded objects; no original entry runs.
use mkw_arm64::terminal::{conditional::Effect, contract::Contract, Image};
use mkw_codegen::guarded::Artifact;
use mkw_runtime::LoadedModule;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    time::Instant,
};
pub(crate) struct Report {
    modules: Vec<Value>,
    samples: Vec<Value>,
    began: Instant,
}
impl Report {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            samples: Vec::new(),
            began: Instant::now(),
        }
    }
    pub fn module(
        &mut self,
        module: &LoadedModule,
        image: &Image,
        facts: &BTreeMap<u64, Effect>,
        closed: &BTreeMap<u64, Value>,
    ) -> Result<(), String> {
        let mut proofs = BTreeMap::new();
        for effect in facts.values() {
            prove(image, effect, &mut proofs)?;
        }
        let extents = mkw_formats::unwind::functions(&module.nso)?;
        let mut plain = 0;
        let mut added = 0;
        let mut rejects = 0;
        let mut root_contract = None;
        let root = module
            .nso
            .mod0
            .as_ref()
            .and_then(|m| {
                m.symbols
                    .iter()
                    .find(|s| s.section != 0 && s.name == "_ZN2nn2os6detail10RngManagerC1Ev")
            })
            .map(|s| s.value);
        for extent in &extents {
            let at = extent.start as u64;
            let contract = match Contract::plain_services(image, at, extent.size) {
                Ok(c) => {
                    plain += 1;
                    c
                }
                Err(_) => {
                    if !closed.contains_key(&at) {
                        rejects += 1;
                        continue;
                    }
                    let c = if let Some(effect) = facts.get(&at) {
                        Contract::effect(image, effect)?
                    } else {
                        let targets =
                            direct_targets(module.nso.read(extent.start, extent.size)?, at);
                        let effects = targets
                            .iter()
                            .filter(|t| **t != at)
                            .filter_map(|t| facts.get(t))
                            .cloned()
                            .collect::<Vec<_>>();
                        Contract::analyze(image, at, extent.size, &effects)?
                    };
                    added += 1;
                    c
                }
            };
            for effect in contract.effects().iter().chain(contract.alias_next()) {
                prove(image, effect, &mut proofs)?;
            }
            // Every counted entry passes the new emitter/graph/profile checks. C/object
            // compilation is bounded below; these are capability-scoped translations.
            let artifact = Artifact::new(contract.clone(), "mkw_gv4_survey")?;
            if root == Some(at) {
                root_contract = Some(contract);
            }
            drop(artifact);
        }
        if let Some(root) = root_contract {
            let mut selected = BTreeMap::<u64, Contract>::new();
            collect(&root, &proofs, &mut selected)?;
            for (entry, c) in selected {
                let a = Artifact::new(c, &format!("mkw_gv4_sample_{entry:x}"))?;
                let dir = Path::new("private/analysis/guarded-v4/objects").join(&module.name);
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let src = dir.join(format!("{entry:x}.c"));
                let obj = dir.join(format!("{entry:x}.o"));
                fs::write(&src, a.code()).map_err(|e| e.to_string())?;
                let output = Command::new("cc")
                    .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-c", "-I"])
                    .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../../native"))
                    .arg(&src)
                    .arg("-o")
                    .arg(&obj)
                    .output()
                    .map_err(|e| e.to_string())?;
                if !output.status.success() {
                    return Err(format!(
                        "guarded native sample failed:{}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                self.samples.push(json!({"module_private":module.name,"source_private":a.contract().source(),"claim":a.contract().claim().is_some(),"alias":a.contract().alias_next().is_some(),"contract_digest_private":mkw_formats::hex(&a.contract().digest()),"origin_private":mkw_formats::hex(&a.origin()),"object_sha256_private":mkw_formats::sha256(&fs::read(obj).map_err(|e|e.to_string())?)}));
            }
        }
        self.modules.push(json!({"module":module.name,"indexed_extents":extents.len(),"plain_services_eligible":plain,"additional_guarded_eligible":added,"guarded_eligible_total":plain+added,"rejected":rejects,"proof_contracts":proofs.len()}));
        Ok(())
    }
    pub fn finish(self) -> Result<(), String> {
        let out = Path::new("private/analysis/guarded-v4");
        fs::create_dir_all(out).map_err(|e| e.to_string())?;
        fs::write(
            out.join("samples.json"),
            serde_json::to_vec_pretty(&self.samples).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        fs::write(out.join("summary.json"),serde_json::to_vec_pretty(&json!({"modules":self.modules,"native_sample_objects":self.samples.len(),"native_sample_source_bytes":self.samples.iter().map(|s|s["source_private"]["bytes"].as_u64().unwrap()).sum::<u64>(),"elapsed_seconds":self.began.elapsed().as_secs_f64(),"classification":"explicit CPU-v10/table-v4 guarded source/compiler capability; objects only, not runtime registration/readiness","existing_acceptance_changed":false,"game_code_generated":true,"original_code_executed":false,"runtime_routes_registered":false,"target_configuration_verified":false,"cross_image_no_return_supported":false,"guarded_veneers_supported":false})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        println!("Explicit guarded-v4 compiler survey/objects checked; no original entry executed or target configuration authorized.");
        Ok(())
    }
}
fn prove(
    image: &Image,
    effect: &Effect,
    cache: &mut BTreeMap<[u8; 32], Contract>,
) -> Result<(), String> {
    if cache.contains_key(&effect.digest()) {
        return Ok(());
    }
    let contract = Contract::effect(image, effect)?;
    for dep in contract.effects().iter().chain(contract.alias_next()) {
        prove(image, dep, cache)?;
    }
    cache.insert(effect.digest(), contract);
    Ok(())
}
fn collect(
    c: &Contract,
    proofs: &BTreeMap<[u8; 32], Contract>,
    out: &mut BTreeMap<u64, Contract>,
) -> Result<(), String> {
    if let Some(old) = out.get(&c.source().entry) {
        if old.digest() != c.digest() {
            return Err("conflicting selected guarded contracts".into());
        }
        return Ok(());
    }
    out.insert(c.source().entry, c.clone());
    for e in c.effects().iter().chain(c.alias_next()) {
        collect(
            proofs
                .get(&e.digest())
                .ok_or("missing selected guarded dependency")?,
            proofs,
            out,
        )?;
    }
    Ok(())
}
fn direct_targets(bytes: &[u8], entry: u64) -> BTreeSet<u64> {
    bytes
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(n, b)| {
            let w = u32::from_le_bytes(b.try_into().unwrap());
            matches!(w & 0xfc000000, 0x14000000 | 0x94000000).then(|| {
                (entry + n as u64 * 4).wrapping_add_signed(((w & 0x3ffffff) as i64) << 38 >> 36)
            })
        })
        .collect()
}
