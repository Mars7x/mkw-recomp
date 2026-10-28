//! Analysis evidence only. Existing lift admission and all runtime gates stay intact.
use mkw_arm64::{terminal::Image, Op};
use mkw_formats::{unwind, Nso};
use serde_json::json;
use std::{fs, path::Path};
pub fn run(root: &Path) -> Result<(), String> {
    let out = Path::new("private/analysis");
    super::analysis::verify(root, out)?;
    let mut reports = Vec::new();
    let mut details = Vec::new();
    for path in super::analysis::walk(&root.join("exefs"))? {
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if !raw.starts_with(b"NSO0") {
            continue;
        }
        let nso = Nso::parse(&raw)?;
        let module = path
            .strip_prefix(root.join("exefs"))
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();
        let extents = unwind::functions(&nso)?;
        let text = &nso.segments[0];
        let image = Image::new(text.memory_offset as u64, text.bytes.clone())
            .map_err(|e| format!("proof image:{e:?}"))?;
        let mut accepted = 0;
        let mut checked = 0;
        let mut certified = 0;
        for e in &extents {
            if e.size > 1024 * 1024 {
                continue;
            }
            let ir = match mkw_arm64::lift_function(
                nso.read(e.start, e.size)?,
                e.start as u64,
                e.start as u64,
            ) {
                Ok(ir) => ir,
                Err(_) => continue,
            };
            accepted += 1;
            if ir.iter().any(|i| {
                matches!(
                    i.op,
                    Op::Ret { .. }
                        | Op::AuthReturn { .. }
                        | Op::IndirectBranch { .. }
                        | Op::TailBranch { .. }
                )
            }) {
                continue;
            }
            checked += 1;
            let analysis = image
                .analyze(e.start as u64, e.size, &[])
                .map_err(|x| format!("proof span:{x:?}"))?;
            if !analysis.is_closed() {
                return Err(
                    "existing accepted closed-control candidate failed proof analysis".into(),
                );
            }
            let fact = analysis
                .no_return_fact()
                .ok_or("return-free accepted candidate did not establish evidence")?;
            certified += 1;
            details.push(json!({"module_private":module,"source_sha256_private":nso.sha256,"extent_private":e,"source_stamp_private":fact.source(),"proof_digest_private":mkw_formats::hex(&fact.proof_digest()),"dependencies":fact.dependencies().count()}));
        }
        reports.push(json!({"module":module,"source_sha256":nso.sha256,"indexed_extents":extents.len(),"strict_accepted":accepted,"checked_return_free_candidates":checked,"certified_local_closed_control":certified}));
    }
    fs::write(
        out.join("terminal-source-proof-details.json"),
        serde_json::to_vec_pretty(&details).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write(out.join("terminal-source-proof-summary.json"),serde_json::to_vec_pretty(&json!({"modules":reports,"compiler_acceptance_changed":false,"game_code_generated":false,"game_code_executed":false,"runtime_guards_installed":false,"target_profile_verified":false,"imports_or_cross_image_facts_supported":false,"classification":"local immutable control evidence only; not accepted coverage gain or execution authorization"})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    println!("Immutable local terminal-source evidence checked; existing acceptance unchanged; no original generation/execution or runtime guard installed.");
    Ok(())
}
