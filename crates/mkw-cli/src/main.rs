mod analysis;
mod blockers;
mod exclusive_survey;
mod import_witnesses;
mod pipeline;
#[cfg(target_os = "linux")]
mod romfs_probe;
mod runtime_metadata;
mod survey;
mod terminal_proofs;
use mkw_formats::{Npdm, Nso};
use std::{error::Error, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const USAGE:&str="usage: mkw-recomp {analyze|verify|link-check|cpu-survey|cpu-survey-exclusive-v10|cpu-survey-exclusive-v2|cpu-survey-services-v3|romfs-hle-check|runtime-metadata-check|terminal-proof-check|import-witness-check|cpu-blockers} GAME_ROOT | inspect FILE | aot-demo | generate-slice GAME_ROOT MODULE ENTRY LENGTH";
fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1)
    }
}
fn number(text: &str) -> Result<u64> {
    Ok(if let Some(hex) = text.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)?
    } else {
        text.parse()?
    })
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let argument =
        |i: usize| -> Result<&str> { args.get(i).map(String::as_str).ok_or_else(|| USAGE.into()) };
    match argument(1)? {
        "decode-words" => {
            use std::io::BufRead;
            for line in std::io::stdin().lock().lines() {
                let input: serde_json::Value = serde_json::from_str(&line?)?;
                let word = u32::try_from(input["word"].as_u64().ok_or("word must be a u32")?)?;
                let pc = input["pc"].as_u64().unwrap_or(0);
                let output = match mkw_arm64::decode(word, pc) {
                    Ok(instruction) => serde_json::json!({"word":word,"instruction":instruction}),
                    Err(error) => serde_json::json!({"word":word,"error":error}),
                };
                println!("{output}");
            }
        }
        "aot-demo" => pipeline::demo()?,
        #[cfg(target_os = "linux")]
        "romfs-hle-check" => romfs_probe::run(Path::new(argument(2)?))?,
        "cpu-survey" => survey::run(Path::new(argument(2)?))?,
        "cpu-survey-exclusive-v10" => exclusive_survey::run(Path::new(argument(2)?))?,
        "cpu-survey-services-v3" => exclusive_survey::run_v3(Path::new(argument(2)?))?,
        "cpu-survey-exclusive-v2" => exclusive_survey::run_v2(Path::new(argument(2)?))?,
        "runtime-metadata-check" => runtime_metadata::run(Path::new(argument(2)?))?,
        "terminal-proof-check" => terminal_proofs::run(Path::new(argument(2)?))?,
        "import-witness-check" => import_witnesses::run(Path::new(argument(2)?))?,
        "cpu-blockers" => blockers::run(Path::new(argument(2)?))?,
        "link-check" => pipeline::link_check(Path::new(argument(2)?))?,
        "generate-slice" => {
            pipeline::generate_slice(
                Path::new(argument(2)?),
                argument(3)?,
                number(argument(4)?)?,
                usize::try_from(number(argument(5)?)?)?,
            )?;
        }
        "analyze" => analysis::analyze(Path::new(argument(2)?), Path::new("private/analysis"))?,
        "verify" => analysis::verify(Path::new(argument(2)?), Path::new("private/analysis"))?,
        "inspect" => {
            let bytes = std::fs::read(argument(2)?)?;
            let value = if bytes.starts_with(b"NSO0") {
                serde_json::to_value(Nso::parse(&bytes)?)?
            } else {
                serde_json::to_value(Npdm::parse(&bytes)?)?
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
