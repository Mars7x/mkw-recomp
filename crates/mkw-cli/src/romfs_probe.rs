//! Execute only original synthetic AOT code; read one bounded supplied RomFS file.
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_calls::{Routed, Router, Target},
    exclusive_native::{Bridge, CpuV10},
    romfs_hle::{self as hle, Limits, ReadOnlyFs},
    GuestAddr, Permissions,
};
use std::{
    fs,
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
fn call(
    router: &Router<'_>,
    bridge: &mut Bridge,
    target: u64,
    args: [u64; 8],
) -> Result<CpuV10, String> {
    let mut cpu = CpuV10::default();
    cpu.base.x[..8].copy_from_slice(&args);
    cpu.base.x[8] = target;
    cpu.base.x[30] = 0x9000;
    let mut budget = 16;
    {
        let mut guard = bridge.attach(&mut cpu)?;
        let mut routed = Routed::attach(&mut guard, router)?;
        let status = routed.run(GuestAddr(0x4000), &mut budget)?;
        if status != 0 || budget != 12 || routed.cpu_mut().base.pc != 0x9000 {
            return Err(format!(
                "synthetic HLE probe status/budget/PC mismatch: {status}"
            ));
        }
    }
    Ok(cpu)
}
fn word(memory: &Arc<Mutex<ExclusiveMemory>>, at: u64) -> Result<u64, String> {
    memory
        .lock()
        .map_err(|_| "poisoned probe memory")?
        .read(GuestAddr(at), Width::Double)
        .map(|v| v as u64)
        .map_err(|e| format!("probe output: {e:?}"))
}
pub fn run(root: &Path) -> Result<(), String> {
    let (modules, mut m, plan) = super::pipeline::prepare_link(root)?;
    let descriptors = mkw_runtime::romfs_hooks::checked_hooks(&modules, &plan, &m)?;
    let find = |symbol: &str| -> Result<u64, String> {
        descriptors
            .iter()
            .find(|d| d.symbol() == symbol)
            .map(|d| d.address().0)
            .ok_or_else(|| "missing checked filesystem hook".into())
    };
    let romfs = root.join("romfs");
    let selected = super::analysis::walk(&romfs)?
        .into_iter()
        .find(|path| {
            path.metadata()
                .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= 4096)
                && path
                    .strip_prefix(&romfs)
                    .is_ok_and(|p| p.as_os_str().len() + 5 <= 0x300)
        })
        .ok_or("no bounded regular RomFS probe candidate")?;
    let relative = selected.strip_prefix(&romfs).map_err(|e| e.to_string())?;
    use std::os::unix::ffi::OsStrExt;
    let mut guest_path = b"rom:/".to_vec();
    guest_path.extend_from_slice(relative.as_os_str().as_bytes());
    guest_path.push(0);
    let expected = fs::read(&selected).map_err(|e| e.to_string())?;
    let dir = Path::new("private/testing/romfs-hle-supplied");
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let instructions = [0xaa1e03f3u32, 0xd63f0100, 0xaa1303fe, 0xd65f03c0]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>();
    let ir = mkw_arm64::lift(&instructions, 0, 0)?;
    let source = mkw_codegen::emit_exclusive_calls_v10(&ir, 0)?;
    let src = dir.join("synthetic.c");
    let library = dir.join("synthetic.so");
    fs::write(&src, source).map_err(|e| e.to_string())?;
    let build = Command::new("cc")
        .args([
            "-std=c11",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=undefined",
            "-I",
            "native",
        ])
        .arg(&src)
        .arg("-o")
        .arg(&library)
        .output()
        .map_err(|e| e.to_string())?;
    fs::write(dir.join("compiler.log"), &build.stderr).map_err(|e| e.to_string())?;
    if !build.status.success() {
        return Err("synthetic HLE probe compilation failed; private compiler log retained".into());
    }
    // SAFETY: source is the fixed author-created four-instruction wrapper above.
    let code = unsafe { libloading::Library::new(&library) }.map_err(|e| e.to_string())?;
    let backend = ReadOnlyFs::new(
        &romfs,
        "rom",
        Limits {
            handles: 2,
            read_bytes: 4096,
        },
    )
    .map_err(|e| e.to_string())?;
    let mut targets =
        vec![unsafe { Target::load(&code, b"mkw_guest_v10_budget\0", GuestAddr(0x4000), 0x4000) }?];
    for descriptor in &descriptors {
        targets.push(descriptor.bind(&backend)?);
    }
    let router = Router::new(targets, 8)?;
    m.map(
        GuestAddr(0x1000),
        vec![0; 0x4000],
        Permissions::RW,
        "synthetic-probe-memory",
    )?;
    let memory = Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 1,
            },
        )
        .map_err(|e| format!("probe config: {e:?}"))?,
    ));
    memory
        .lock()
        .map_err(|_| "poisoned probe memory")?
        .write(GuestAddr(0x1400), &guest_path)
        .map_err(|e| format!("probe path: {e:?}"))?;
    let mut bridge = Bridge::new(memory.clone())?;
    if call(
        &router,
        &mut bridge,
        find(hle::OPEN)?,
        [0x1200, 0x1400, 1, 0, 0, 0, 0, 0],
    )?
    .base
    .x[0]
        != 0
    {
        return Err("supplied RomFS open refused".into());
    }
    let handle = word(&memory, 0x1200)?;
    if call(
        &router,
        &mut bridge,
        find(hle::SIZE)?,
        [0x1210, handle, 0, 0, 0, 0, 0, 0],
    )?
    .base
    .x[0]
        != 0
        || word(&memory, 0x1210)? != expected.len() as u64
    {
        return Err("supplied RomFS size differs".into());
    }
    if call(
        &router,
        &mut bridge,
        find(hle::READ)?,
        [0x1220, handle, 0, 0x2000, expected.len() as u64, 0, 0, 0],
    )?
    .base
    .x[0]
        != 0
        || word(&memory, 0x1220)? != expected.len() as u64
    {
        return Err("supplied RomFS read differs".into());
    }
    let actual = {
        let memory = memory.lock().map_err(|_| "poisoned probe memory")?;
        (0..expected.len())
            .map(|i| {
                memory
                    .read(GuestAddr(0x2000 + i as u64), Width::Byte)
                    .map(|v| v as u8)
                    .map_err(|e| format!("probe data: {e:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    if actual != expected || fs::read(&selected).map_err(|e| e.to_string())? != expected {
        return Err("supplied RomFS content mismatch/changed input".into());
    }
    call(
        &router,
        &mut bridge,
        find(hle::CLOSE)?,
        [handle, 0, 0, 0, 0, 0, 0, 0],
    )?;
    let receipt = serde_json::json!({"original_guest_execution":false,"synthetic_native_execution":true,"path_private":relative,"bytes_read":actual.len(),"actual_sha256_private":mkw_formats::sha256(&actual),"host_stream_comparison":true,"input_read_only":true,"operations":["OpenFile","GetFileSize","ReadFile","CloseFile"],"checked_hooks_private":descriptors,"source_layout":"existing diagnostic rtld-first mapping, not retail startup","cpu_frame_version":10,"exclusive_table_version":2});
    fs::write(
        dir.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "synthetic native RomFS HLE probe: {} bytes match; no original code executed",
        actual.len()
    );
    Ok(())
}
