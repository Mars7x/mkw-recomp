//! Metadata consistency only: no service activation or target authorization.
use mkw_formats::{capabilities::KernelCapabilityFacts, Npdm};
use serde_json::json;
use std::{fs, path::Path};
pub fn run(root: &Path) -> Result<(), String> {
    let out = Path::new("private/analysis");
    super::analysis::verify(root, out)?;
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(out.join("exefs-manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let path = root.join("exefs/main.npdm");
    if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("NPDM exceeds diagnostic bound".into());
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("NPDM exceeds diagnostic bound".into());
    }
    let npdm = Npdm::parse(&bytes)?;
    let expected = manifest["files"]
        .as_array()
        .ok_or("missing manifest files")?
        .iter()
        .find(|f| f["path"] == "main.npdm")
        .ok_or("main.npdm absent from manifest")?;
    if expected["sha256"].as_str() != Some(npdm.sha256.as_str()) {
        return Err("NPDM changed after identity verification".into());
    }
    let facts = KernelCapabilityFacts::inspect(&npdm.kernel_capabilities)
        .map_err(|e| format!("unsupported/malformed capability facts: {e:?}"))?;
    let threads = facts.threads.ok_or("missing declared thread constraints")?;
    if !(threads.minimum_priority..=threads.maximum_priority).contains(&npdm.main_thread_priority)
        || !(threads.minimum_virtual_core..=threads.maximum_virtual_core)
            .contains(&npdm.main_thread_cpu)
        || npdm.stack_size % 4096 != 0
    {
        return Err("declared main thread exceeds metadata constraints".into());
    }
    fs::write(out.join("runtime-metadata-facts.json"), serde_json::to_vec_pretty(&json!({
        "classification":"declared metadata facts and consistency only",
        "source_sha256":npdm.sha256,"facts":facts,
        "main_thread":{"priority":npdm.main_thread_priority,"virtual_core":npdm.main_thread_cpu,"stack_bytes":npdm.stack_size},
        "signature_verified":false,"target_configuration_verified":false,
        "game_code_executed":false,"services_activated":false,
        "limitations":"Selected reference descriptors only; no ACID policy/signature validation, physical mapping, target core/firmware/PAC/TLS/time configuration or execution authorization. Shared handle entries are not an event object quota."
    })).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("Declared runtime metadata is internally consistent; private facts recorded. No target authorization or original execution.");
    Ok(())
}
