//! Explicit v10 diagnostic compiler profile. Never loads or executes guest objects.
use mkw_formats::{unwind, Nso};
use std::{fs, path::Path, process::Command};
pub fn run(root: &Path) -> Result<(), String> {
    run_profile(root, false)
}
pub fn run_v2(root: &Path) -> Result<(), String> {
    run_profile(root, true)
}
fn run_profile(root: &Path, calls_v2: bool) -> Result<(), String> {
    super::analysis::verify(root, Path::new("private/analysis"))?;
    let output = Path::new(if calls_v2 {
        "private/analysis/cpu-exclusive-v2"
    } else {
        "private/analysis/cpu-exclusive-v10"
    });
    fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut modules = Vec::new();
    let mut details = Vec::new();
    for path in super::analysis::walk(&root.join("exefs"))? {
        let name = path
            .strip_prefix(root.join("exefs"))
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if !raw.starts_with(b"NSO0") {
            continue;
        }
        let n = Nso::parse(&raw)?;
        let extents = unwind::functions(&n)?;
        let mut old = 0;
        let mut new = 0;
        let mut compiled = 0;
        let mut deferred = 0;
        for extent in &extents {
            if extent.size > 1024 * 1024 {
                continue;
            }
            let bytes = n.read(extent.start, extent.size)?;
            if mkw_arm64::lift_function(bytes, extent.start as u64, extent.start as u64).is_ok() {
                old += 1;
                continue;
            }
            if let Ok(ir) = mkw_arm64::lift_function_exclusive_v10(
                bytes,
                extent.start as u64,
                extent.start as u64,
            ) {
                new += 1;
                let mut row = serde_json::json!({"module":name,"extent":extent,"instructions":ir.len(),"executed":false});
                if ir.len() > 4096 {
                    deferred += 1;
                    row["compiled"] = false.into();
                } else {
                    let dir = output.join(&name).join(format!("{:x}", extent.start));
                    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let src = dir.join("guest.c");
                    fs::write(
                        &src,
                        if calls_v2 {
                            mkw_codegen::emit_exclusive_calls_v10(&ir, extent.start as u64)?
                        } else {
                            mkw_codegen::emit_exclusive_v10(&ir, extent.start as u64)?
                        },
                    )
                    .map_err(|e| e.to_string())?;
                    let result = Command::new("cc")
                        .args([
                            "-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-c", "-I",
                        ])
                        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
                        .arg(&src)
                        .arg("-o")
                        .arg(dir.join("guest.o"))
                        .output()
                        .map_err(|e| e.to_string())?;
                    fs::write(dir.join("compiler.stderr.log"), &result.stderr)
                        .map_err(|e| e.to_string())?;
                    if !result.status.success() {
                        return Err(format!(
                            "compile failed {}:{:x}: {}",
                            name,
                            extent.start,
                            String::from_utf8_lossy(&result.stderr)
                        ));
                    }
                    compiled += 1;
                    row["compiled"] = true.into();
                }
                details.push(row);
            }
        }
        modules.push(serde_json::json!({"module":name,"nso_sha256":n.sha256,"indexed_extents":extents.len(),"default_accepted":old,"exclusive_v10_conditional_additional":new,"additional_compiled_objects":compiled,"additional_deferred":deferred}));
    }
    let result = serde_json::json!({"modules":modules,"details_private":details,"cpu_frame_version":10,"exclusive_table_version":if calls_v2 {2}else{1},"game_code_executed":false,"default_acceptance_changed":false,"target_profile_verified":false,"limitations":"Explicit serialized owned normal-memory v10 compiler profile only; executable identity/configuration, aliases/MMIO, traps, weak ordering, PAC and call/dispatch backends remain gated. New objects are compilation evidence, never runtime coverage."});
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result["modules"]).unwrap()
    );
    Ok(())
}
