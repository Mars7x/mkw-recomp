use mkw_formats::{sha256, Nso, Result};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, bytes).map_err(|e| e.to_string())
}
fn json_write(path: &Path, value: serde_json::Value) -> Result<()> {
    write(
        path,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
    )
}

pub fn demo() -> Result<()> {
    let dir = Path::new("private/builds/synthetic");
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let words = [
        0xd2800500u32,
        0xd2800041,
        0x91000400,
        0xf1000421,
        0xb5ffffc1,
        0xd65f03c0,
    ];
    let bytes: Vec<_> = words.iter().flat_map(|x| x.to_le_bytes()).collect();
    let ir = mkw_arm64::lift(&bytes, 0x1000, 0x1000)?;
    write(&dir.join("program.arm64.bin"), &bytes)?;
    json_write(&dir.join("ir.json"), json!(ir))?;
    let code = mkw_codegen::emit(&ir, 0x1000)?;
    write(&dir.join("guest.c"), code)?;
    write(&dir.join("guest.h"), mkw_codegen::HEADER)?;
    write(&dir.join("harness.c"),br#"#include "guest.h"
#include <stdio.h>
int main(void) {MkwCpu c={0}; c.x[30]=0xdead;
 int status=mkw_guest(&c,100);
 if(status || c.x[0]!=42 || c.x[1] || c.pc!=0xdead || !c.z || !c.c || c.n || c.v) return 1;
 printf("host-native synthetic result=%llu; NZCV=%u%u%u%u\n",(unsigned long long)c.x[0],c.n,c.z,c.c,c.v); return 0;}
"#)?;
    let exe = dir.join("aot-demo");
    let output = Command::new("cc")
        .args(["-std=c11", "-O2", "-g", "-Wall", "-Wextra", "-Werror"])
        .arg(dir.join("guest.c"))
        .arg(dir.join("harness.c"))
        .arg("-o")
        .arg(&exe)
        .output()
        .map_err(|e| format!("cc: {e}"))?;
    write(&dir.join("compiler.stderr.log"), &output.stderr)?;
    if !output.status.success() {
        return Err(
            "host C compilation failed; see private/builds/synthetic/compiler.stderr.log".into(),
        );
    }
    let run = Command::new(&exe).output().map_err(|e| e.to_string())?;
    write(&dir.join("run.stdout.log"), &run.stdout)?;
    write(&dir.join("run.stderr.log"), &run.stderr)?;
    if !run.status.success() {
        return Err(format!(
            "native synthetic program failed: {:?}",
            run.status.code()
        ));
    }
    print!("{}", String::from_utf8_lossy(&run.stdout));
    json_write(
        &dir.join("receipt.json"),
        json!({"kind":"independently-authored-synthetic","source_sha256":sha256(&bytes),"native_executable_sha256":sha256(&fs::read(&exe).map_err(|e|e.to_string())?),"host_arch":std::env::consts::ARCH,"guest_instructions":ir.len(),"return_code":run.status.code(),"game_code_executed":false}),
    )?;
    Ok(())
}
/// Shared diagnostic linking only; never executes an original function.
pub(crate) fn prepare_link(
    root: &Path,
) -> Result<(
    Vec<mkw_runtime::LoadedModule>,
    mkw_runtime::Memory,
    mkw_runtime::RelocationPlan,
)> {
    super::analysis::verify(root, Path::new("private/analysis"))?;
    let mut modules = Vec::new();
    for path in super::analysis::walk(&root.join("exefs"))? {
        let b = fs::read(&path).map_err(|e| e.to_string())?;
        if !b.starts_with(b"NSO0") {
            continue;
        }
        let nso = Nso::parse(&b)?;
        let name = path
            .strip_prefix(root.join("exefs"))
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let index = modules.len() as u64;
        modules.push(mkw_runtime::LoadedModule {
            name,
            base: mkw_runtime::GuestAddr(0x7100000000 + index * 0x10000000),
            nso,
        });
    }
    let mut memory = mkw_runtime::map_modules(&modules)?;
    let mut order: Vec<_> = (0..modules.len()).collect();
    // Observed rtld: append itself, then enumerate other modules by guest mapping address.
    let rtld = modules
        .iter()
        .position(|m| m.nso.module_name == "nnrtld")
        .ok_or("cannot infer process order without observed nnrtld module")?;
    order.sort_by_key(|&i| (i != rtld, modules[i].base.0));
    let scopes = mkw_runtime::LinkScopes::process_order(&modules, &order)?;
    let plan = match mkw_runtime::plan_relocations_scoped(&modules, &memory, &scopes) {
        Ok(plan) => plan,
        Err(error) => {
            json_write(
                Path::new("private/analysis/link-check.json"),
                json!({"status":"blocked","error":error,"mapped_modules":modules.len(),"mapped_bytes":memory.mapped_bytes(),"relocations_applied":0,"guest_code_executed":false}),
            )?;
            return Err(error);
        }
    };
    json_write(
        Path::new("private/analysis/symbol-bindings.json"),
        json!({"policy":scopes.policy,"process_order":order.iter().map(|&i|&modules[i].name).collect::<Vec<_>>(),"dependencies":scopes.dependencies,"search_scopes":scopes.search,"bindings":plan.bindings}),
    )?;
    plan.apply(&mut memory)?;
    plan.verify_applied(&memory)?;
    Ok((modules, memory, plan))
}
pub fn link_check(root: &Path) -> Result<()> {
    let (modules, mut memory, plan) = prepare_link(root)?;
    memory.seal();
    let receipt = json!({"modules":modules.iter().map(|m|json!({"name":m.name,"base":m.base.0,"source_sha256":m.nso.sha256})).collect::<Vec<_>>(),"mapped_bytes":memory.mapped_bytes(),"regions":memory.region_count(),"relocations_applied":plan.len(),"relocation_readback_verified":true,"module_base_policy":"diagnostic alphabetical-file order, 0x7100000000 + i*0x10000000; not a proven game process layout","limitations":"explicit observed-order process lookup; guest address layout remains diagnostic; no final RelRo transition, initializers, TLS, service runtime, or guest execution","guest_code_executed":false});
    json_write(Path::new("private/analysis/link-check.json"), receipt)?;
    println!(
        "mapped {} modules, {} bytes; applied {} relocations; no guest code executed",
        modules.len(),
        memory.mapped_bytes(),
        plan.len()
    );
    Ok(())
}
pub fn generate_slice(root: &Path, module: &str, entry: u64, length: usize) -> Result<()> {
    super::analysis::verify(root, Path::new("private/analysis"))?;
    if Path::new(module)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("invalid module path".into());
    }
    let b = fs::read(root.join("exefs").join(module)).map_err(|e| e.to_string())?;
    let n = Nso::parse(&b)?;
    let text = &n.segments[0];
    let end = entry
        .checked_add(length as u64)
        .ok_or("slice range overflow")?;
    if entry < text.memory_offset as u64 || end > (text.memory_offset + text.size) as u64 {
        return Err("slice must be inside verified text segment".into());
    }
    let source_inputs = std::iter::once(env!("CARGO_PKG_VERSION"))
        .chain(mkw_arm64::SOURCE_INPUTS.iter().copied())
        .chain(mkw_codegen::SOURCE_INPUTS.iter().copied())
        .collect::<Vec<_>>()
        .join("\0");
    let translator = sha256(source_inputs.as_bytes());
    let dir: PathBuf = Path::new("private/generated")
        .join(&n.sha256)
        .join(translator)
        .join(format!("slice-{entry:x}-{length:x}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ir = match mkw_arm64::lift(n.read(entry as usize, length)?, entry, entry) {
        Ok(ir) => ir,
        Err(e) => {
            json_write(
                &dir.join("failure.json"),
                json!({"error":e,"build_id":n.build_id,"source_sha256":n.sha256,"module":module,"entry":entry,"length":length,"compiled":false}),
            )?;
            return Err(e);
        }
    };
    write(&dir.join("guest.c"), mkw_codegen::emit(&ir, entry)?)?;
    json_write(
        &dir.join("source-map.json"),
        json!({"module":module,"build_id":n.build_id,"source_sha256":n.sha256,"entry":entry,"native_symbol":"mkw_guest","ir":ir,"compiled":false,"limitations":"single closed CFG; returns exit to host wrapper; explicit PAC/memory callbacks required; no verified game PAC profile, calls, FP/SIMD, TLS or services"}),
    )?;
    println!(
        "generated {} instructions at {}; not compiled or executed",
        ir.len(),
        dir.display()
    );
    Ok(())
}
