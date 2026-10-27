use mkw_formats::{sha256, unwind, Nso, Result};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufWriter, Write},
    path::Path,
    process::Command,
};
/// Survey only. Native sample objects are compiled but never executed or linked as a game.
pub fn run(root: &Path) -> Result<()> {
    super::analysis::verify(root, Path::new("private/analysis"))?;
    let mut details = BufWriter::new(
        fs::File::create("private/analysis/cpu-survey.jsonl").map_err(|e| e.to_string())?,
    );
    let mut summaries = Vec::new();
    for path in super::analysis::walk(&root.join("exefs"))? {
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        if !raw.starts_with(b"NSO0") {
            continue;
        }
        let n = Nso::parse(&raw)?;
        let name = path
            .strip_prefix(root.join("exefs"))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let extents = unwind::functions(&n)?;
        let mut complete = 0;
        let mut pac_complete = 0;
        let mut compiled = 0;
        let mut compiled_pac = 0;
        let mut sampled = BTreeSet::new();
        let mut failures = BTreeMap::<String, usize>::new();
        for extent in &extents {
            let mut row = json!({"module":name,"extent":extent});
            let result = if extent.size > 1024 * 1024 {
                Err("function exceeds 1 MiB survey limit".into())
            } else {
                mkw_arm64::lift_function(
                    n.read(extent.start, extent.size)?,
                    extent.start as u64,
                    extent.start as u64,
                )
            };
            match result {
                Err(error) => {
                    let category = if let Some(t) = error.split(" (").nth(1) {
                        t.split(')').next().unwrap_or("unknown").to_string()
                    } else {
                        error.split(" at ").next().unwrap_or("unknown").to_string()
                    };
                    *failures.entry(category).or_default() += 1;
                    row["failure"] = json!(error);
                }
                Ok(ir) => {
                    complete += 1;
                    let pac = ir.iter().any(|i| {
                        matches!(
                            i.op,
                            mkw_arm64::Op::PointerAuth { .. } | mkw_arm64::Op::AuthReturn { .. }
                        )
                    });
                    if pac {
                        pac_complete += 1;
                    }
                    row["subset_instructions"] = json!(ir.len());
                    row["requires_atomic_memory_backend"] =
                        json!(ir.iter().any(|i| matches!(i.op, mkw_arm64::Op::Atomic(_))));
                    row["requires_thread_register_configuration"] =
                        json!(ir.iter().any(|i| matches!(
                            i.op,
                            mkw_arm64::Op::SystemRegister {
                                register: mkw_arm64::floating::SystemReg::TpidrEl0
                                    | mkw_arm64::floating::SystemReg::TpidrroEl0,
                                ..
                            }
                        )));
                    row["requires_floating_backend"] = json!(ir.iter().any(|i| matches!(
                        i.op,
                        mkw_arm64::Op::Floating(_)
                            | mkw_arm64::Op::VectorFp(_)
                            | mkw_arm64::Op::Convert(_)
                            | mkw_arm64::Op::ConditionalFloat { .. }
                    )));
                    row["requires_barrier_backend"] = json!(ir
                        .iter()
                        .any(|i| matches!(i.op, mkw_arm64::Op::Barrier { .. })));
                    row["requires_pac_backend"] = json!(pac);
                    row["requires_call_backend"] = json!(ir.iter().any(|i| matches!(
                        i.op,
                        mkw_arm64::Op::Call { .. }
                            | mkw_arm64::Op::IndirectCall { .. }
                            | mkw_arm64::Op::IndirectBranch { .. }
                            | mkw_arm64::Op::TailBranch { .. }
                    )));
                    row["requires_ordered_memory_backend"] = json!(ir
                        .iter()
                        .any(|i| matches!(i.op, mkw_arm64::Op::OrderedMemory { .. })));
                    row["requires_memory_backend"] = json!(ir.iter().any(|i| matches!(
                        i.op,
                        mkw_arm64::Op::OrderedMemory { .. } | mkw_arm64::Op::Memory(_)
                    )));
                    let mut features = Vec::new();
                    for instruction in &ir {
                        use mkw_arm64::{
                            memory::{AddressMode, ValueReg},
                            Op,
                        };
                        match instruction.op {
                            Op::FloatSelect { .. } => features.push("fp_conditional_select"),
                            Op::Convert(_) => features.push("fp_integer_conversion"),
                            Op::ConditionalFloat { .. } => features.push("conditional_fp_compare"),
                            Op::VectorFp(v) => {
                                features.push("vector_fp");
                                if v.rhs_lane.is_some() {
                                    features.push("indexed_vector_fp");
                                }
                            }
                            Op::Floating(f) => {
                                features.push("scalar_fp");
                                if f.operation == 14 {
                                    features.push("fp_format_conversion");
                                }
                                if (11..=13).contains(&f.operation) {
                                    features.push("fused_sign_fp");
                                }
                            }
                            Op::SystemRegister {
                                register:
                                    mkw_arm64::floating::SystemReg::TpidrEl0
                                    | mkw_arm64::floating::SystemReg::TpidrroEl0,
                                ..
                            } => features.push("thread_register_state"),
                            Op::SystemRegister { .. } => features.push("fp_nzcv_register"),
                            Op::Barrier { .. } => features.push("data_barrier"),
                            Op::Atomic(_) => features.push("lse_atomic"),
                            Op::Simd(_) => features.push("simd_register_bits"),
                            Op::Crc32 { .. } => features.push("crc32"),
                            Op::IntegerUnary { .. } | Op::CarryArithmetic { .. } => {
                                features.push("integer_bits_carry")
                            }
                            Op::Multiply { .. } | Op::Divide { .. } => {
                                features.push("multiply_divide")
                            }
                            Op::IndirectBranch { .. } => features.push("indirect_branch"),
                            Op::TailBranch { .. } => features.push("extent_tail"),
                            Op::ConditionalCompare { .. } => features.push("conditional_compare"),
                            Op::Memory(transfer) => {
                                if matches!(transfer.first, ValueReg::Vector(_)) {
                                    features.push("simd_memory");
                                }
                                if !matches!(transfer.address, AddressMode::Offset(_))
                                    || transfer.signed
                                {
                                    features.push("extended_addressing");
                                }
                            }
                            Op::Logical { .. }
                            | Op::Bitfield { .. }
                            | Op::Extract { .. }
                            | Op::VariableShift { .. }
                            | Op::AddSubExtended { .. } => {
                                features.push("logical_bitfield_extended_arithmetic")
                            }
                            _ => {}
                        }
                    }
                    features.sort_unstable();
                    features.dedup();
                    row["instruction_features"] = json!(features);
                    if (pac && compiled_pac < 4)
                        || features.iter().any(|feature| !sampled.contains(feature))
                    {
                        let dir = Path::new("private/generated/survey")
                            .join(&n.sha256)
                            .join(format!("{:x}", extent.start));
                        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                        let source = dir.join("guest.c");
                        let object = dir.join("guest.o");
                        let code = mkw_codegen::emit(&ir, extent.start as u64)?;
                        fs::write(&source, &code).map_err(|e| e.to_string())?;
                        let build = Command::new("cc")
                            .args(["-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-c"])
                            .arg(&source)
                            .arg("-o")
                            .arg(&object)
                            .output()
                            .map_err(|e| e.to_string())?;
                        fs::write(dir.join("compiler.stderr.log"), &build.stderr)
                            .map_err(|e| e.to_string())?;
                        if !build.status.success() {
                            return Err(format!(
                                "native compilation failed for {name}:{:#x}",
                                extent.start
                            ));
                        }
                        row["compiled_object"] = json!(object);
                        row["generated_source_sha256"] = json!(sha256(code.as_bytes()));
                        compiled += 1;
                        if pac {
                            compiled_pac += 1;
                        }
                        sampled.extend(features);
                        fs::write(dir.join("source-map.json"),serde_json::to_vec_pretty(&json!({"module":name,"source_sha256":n.sha256,"build_id":n.build_id,"extent":extent,"ir":ir,"compiled":true,"executed":false,"game_pac_profile_verified":false})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
                    }
                }
            }
            writeln!(details, "{row}").map_err(|e| e.to_string())?;
        }
        summaries.push(json!({"module":name,"fde_extents":extents.len(),"subset_liftable_functions":complete,"subset_liftable_pac_functions":pac_complete,"native_pac_sample_objects":compiled_pac,"native_sample_objects":compiled,"compiled_instruction_features":sampled,"failure_categories":failures}));
        eprintln!("{name}: {} FDE extents, {complete} subset-liftable functions, {pac_complete} PAC, {compiled} native sample objects",extents.len());
    }
    details.flush().map_err(|e| e.to_string())?;
    fs::write("private/analysis/cpu-survey-summary.json",serde_json::to_vec_pretty(&json!({"modules":summaries,"guest_code_executed":false,"game_pac_profile_verified":false,"limitations":"unwind-index extents only; indirect and out-of-extent B targets may exit through unbound routing requests; other unsupported instructions/edges retained; no whole-module coverage or reference execution"})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(())
}
