//! Diagnostic continuation only: never changes the strict lifting/acceptance path.
use mkw_arm64::Op;
use mkw_formats::{sha256, unwind, Nso, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{BufWriter, Write},
    path::Path,
};

fn issue(pc: u64, word: Option<u32>, error: impl Into<String>) -> Value {
    json!({"pc":pc,"word":word,"error":error.into()})
}
fn diagnose(bytes: &[u8], base: u64) -> Value {
    let mut linear = Vec::new();
    for (n, chunk) in bytes.chunks_exact(4).enumerate() {
        let pc = base + n as u64 * 4;
        let word = u32::from_le_bytes(chunk.try_into().unwrap());
        if let Err(error) = mkw_arm64::decode(word, pc) {
            linear.push(issue(pc, Some(word), error));
        }
    }
    let mut decode = BTreeMap::new();
    let mut edges = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut pending = VecDeque::from([base]);
    let end = base.checked_add(bytes.len() as u64);
    if base % 4 != 0 || bytes.len() % 4 != 0 || end.is_none() {
        return json!({"cfg_decode_issues":[],"cfg_edge_issues":["unaligned/overflowing program"],"cfg_visited":0,"linear_decode_issues":linear});
    }
    let end = end.unwrap();
    while let Some(pc) = pending.pop_front() {
        if !visited.insert(pc) {
            continue;
        }
        if pc < base || pc >= end || pc % 4 != 0 {
            edges.insert(pc);
            continue;
        }
        let at = (pc - base) as usize;
        let word = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        match mkw_arm64::decode(word, pc) {
            Err(error) => {
                decode.insert(pc, issue(pc, Some(word), error));
                pending.push_back(pc + 4);
            }
            Ok(instruction) => match instruction.op {
                Op::Ret { .. }
                | Op::AuthReturn { .. }
                | Op::IndirectBranch { .. }
                | Op::TailBranch { .. } => {}
                Op::Branch { target } => {
                    if target >= base && target < end {
                        pending.push_back(target);
                    }
                }
                Op::CompareBranch { target, .. }
                | Op::BranchCond { target, .. }
                | Op::TestBit { target, .. } => {
                    pending.push_back(target);
                    pending.push_back(pc + 4);
                }
                _ => pending.push_back(pc + 4),
            },
        }
    }
    json!({"cfg_decode_issues":decode.into_values().collect::<Vec<_>>(),"cfg_edge_issues":edges,"cfg_visited":visited.len(),"linear_decode_issues":linear})
}

pub fn run(root: &Path) -> Result<()> {
    super::analysis::verify(root, Path::new("private/analysis"))?;
    let mut output = BufWriter::new(
        fs::File::create("private/analysis/cpu-all-blockers.jsonl").map_err(|e| e.to_string())?,
    );
    let decoder_hash =
        sha256(&serde_json::to_vec(mkw_arm64::SOURCE_INPUTS).map_err(|e| e.to_string())?);
    let mut modules = Vec::new();
    for path in super::analysis::walk(&root.join("exefs"))? {
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if !raw.starts_with(b"NSO0") {
            continue;
        }
        let nso = Nso::parse(&raw)?;
        let name = path
            .strip_prefix(root.join("exefs"))
            .unwrap()
            .to_string_lossy();
        let extents = unwind::functions(&nso)?;
        let mut accepted = 0;
        let mut rejected = 0;
        let mut bytes_scanned = 0u64;
        let mut omitted = 0;
        for extent in &extents {
            let bytes = nso.read(extent.start, extent.size)?;
            let baseline = if extent.size > 1024 * 1024 {
                Err("function exceeds 1 MiB survey limit".into())
            } else {
                mkw_arm64::lift_function(bytes, extent.start as u64, extent.start as u64)
            };
            let error = match baseline {
                Ok(_) => {
                    accepted += 1;
                    continue;
                }
                Err(error) => error,
            };
            rejected += 1;
            let mut row = if extent.size > 1024 * 1024 {
                omitted += 1;
                json!({"cfg_decode_issues":[],"cfg_edge_issues":["survey_size_limit"],"linear_decode_issues":[],"omitted":true})
            } else {
                bytes_scanned += extent.size as u64;
                diagnose(bytes, extent.start as u64)
            };
            row["module"] = json!(name);
            row["extent"] = json!(extent);
            row["baseline_failure"] = json!(error);
            row["source_sha256"] = json!(nso.sha256);
            writeln!(output, "{row}").map_err(|e| e.to_string())?;
        }
        modules.push(json!({"module":name,"extents":extents.len(),"accepted":accepted,"rejected":rejected,"rejected_bytes_scanned":bytes_scanned,"omitted_extents":omitted}));
        eprintln!("{name}: {accepted} accepted unchanged; {rejected} rejected extents diagnosed ({bytes_scanned} bytes; {omitted} omitted)");
    }
    output.flush().map_err(|e| e.to_string())?;
    fs::write("private/analysis/cpu-all-blockers-summary.json",serde_json::to_vec_pretty(&json!({"modules":modules,"decoder_source_sha256":decoder_hash,"acceptance_rules_changed":false,"guest_code_executed":false,"method":"Strict baseline lift unchanged. Separate optimistic CFG continues after decode failures assuming fallthrough; supported edges/returns/tail policy mirror strict lift. Full linear span inventory includes unreachable code/data and is not an acceptance requirement. Unsupported control-flow recovery is incomplete; estimates require real re-survey."})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }
    #[test]
    fn continues_after_every_decode_failure_without_accepting() {
        let b = bytes(&[0xffffffff, 0x91000400, 0xffffffff, 0xd65f03c0]);
        assert!(mkw_arm64::lift_function(&b, 0x1000, 0x1000).is_err());
        let d = diagnose(&b, 0x1000);
        assert_eq!(d["cfg_decode_issues"].as_array().unwrap().len(), 2);
        assert_eq!(d["linear_decode_issues"].as_array().unwrap().len(), 2);
        assert!(d["cfg_edge_issues"].as_array().unwrap().is_empty());
    }
    #[test]
    fn conditional_boundary_remains_blocked_and_direct_tail_remains_allowed() {
        let b = bytes(&[0x54000080, 0xd65f03c0]);
        assert!(mkw_arm64::lift_function(&b, 0x1000, 0x1000).is_err());
        assert_eq!(diagnose(&b, 0x1000)["cfg_edge_issues"], json!([0x1010]));
        let b = bytes(&[0x14000004, 0xd65f03c0]);
        assert!(mkw_arm64::lift_function(&b, 0x1000, 0x1000).is_ok());
        assert_eq!(diagnose(&b, 0x1000)["cfg_edge_issues"], json!([]));
    }
    #[test]
    fn linear_inventory_does_not_turn_unreachable_data_into_cfg_requirements() {
        let b = bytes(&[0x14000002, 0xffffffff, 0xd65f03c0]);
        assert!(mkw_arm64::lift_function(&b, 0x1000, 0x1000).is_ok());
        let d = diagnose(&b, 0x1000);
        assert_eq!(d["linear_decode_issues"].as_array().unwrap().len(), 1);
        assert_eq!(d["cfg_decode_issues"], json!([]));
    }
}
