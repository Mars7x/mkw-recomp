use mkw_formats::{hex, sha256, Npdm, Nso, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

pub fn walk(root: &Path) -> Result<Vec<PathBuf>> {
    fn add(p: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for e in fs::read_dir(p).map_err(|e| format!("{}: {e}", p.display()))? {
            let e = e.map_err(|e| e.to_string())?;
            let t = e.file_type().map_err(|e| e.to_string())?;
            if t.is_symlink() {
                return Err(format!("symlink in input tree: {}", e.path().display()));
            }
            if t.is_dir() {
                add(&e.path(), out)?
            } else if t.is_file() {
                out.push(e.path())
            } else {
                return Err(format!("non-regular input {}", e.path().display()));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    add(root, &mut out)?;
    out.sort();
    Ok(out)
}
fn save(p: &Path, v: &Value) -> Result<()> {
    fs::write(p, serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn tally(map: &mut BTreeMap<String, usize>, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}
fn signature(b: &[u8]) -> String {
    if b.starts_with(b"\x28\xb5\x2f\xfd") {
        return "zstd-frame".into();
    }
    if b.starts_with(b"SARC") {
        return "SARC".into();
    }
    if b.starts_with(b"BNTX\0") {
        return "BNTX-candidate".into();
    }
    if b.starts_with(b"FRES") {
        return "BFRES-candidate".into();
    }
    if b.starts_with(b"FSHA") {
        return "BFSHA-candidate".into();
    }
    if b.starts_with(b"BNSH") {
        return "BNSH-candidate".into();
    }
    if b.starts_with(b"BWAV") {
        return "BWAV-candidate".into();
    }
    if b.starts_with(b"AIB ") {
        return "AINB-candidate".into();
    }
    if b.starts_with(b"BY") || b.starts_with(b"YB") {
        return "BYML-candidate".into();
    }
    if b.starts_with(b"\x1a\x45\xdf\xa3") {
        return "EBML-container".into();
    }
    if b.starts_with(b"NSO0") {
        return "NSO0".into();
    }
    if b.starts_with(b"NRO0") || b.get(16..20) == Some(b"NRO0") {
        return "NRO0-candidate".into();
    }
    if b.starts_with(b"Yaz0") {
        return "Yaz0-candidate".into();
    }
    if b.is_empty() {
        return "empty".into();
    }
    format!("unknown:{}", hex(&b[..b.len().min(4)]))
}

// The SARC inspector validates byte order, header, node and name bounds. It does
// not extract entries or infer file dependencies from incidental strings.
fn sarc_entries(b: &[u8]) -> Result<Vec<(String, usize, usize)>> {
    if b.get(..4) != Some(b"SARC") {
        return Err("not SARC".into());
    }
    let be = match b.get(6..8) {
        Some([0xfe, 0xff]) => true,
        Some([0xff, 0xfe]) => false,
        _ => return Err("invalid SARC BOM".into()),
    };
    let u16at = |o| -> Result<usize> {
        let a = mkw_formats::slice(b, o, 2)?.try_into().unwrap();
        Ok(if be {
            u16::from_be_bytes(a)
        } else {
            u16::from_le_bytes(a)
        } as usize)
    };
    let u32at = |o| -> Result<usize> {
        let a = mkw_formats::slice(b, o, 4)?.try_into().unwrap();
        Ok(if be {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        } as usize)
    };
    let h = u16at(4)?;
    if h != 20 {
        return Err("unsupported SARC header size".into());
    }
    let total = u32at(8)?;
    let data = u32at(12)?;
    if u16at(16)? != 0x100 {
        return Err("unsupported SARC version".into());
    }
    if total != b.len() || data > total {
        return Err("SARC length/data mismatch".into());
    }
    if mkw_formats::slice(b, h, 4)? != b"SFAT" {
        return Err("missing SFAT".into());
    }
    let sh = u16at(h + 4)?;
    if sh != 12 {
        return Err("invalid SFAT header".into());
    }
    let count = u16at(h + 6)?;
    let nodes = h + sh;
    let sfnt = nodes + count * 16;
    if mkw_formats::slice(b, sfnt, 4)? != b"SFNT" || u16at(sfnt + 4)? != 8 {
        return Err("missing SFNT".into());
    }
    let names = sfnt + 8;
    if names > data {
        return Err("SARC nodes overlap data".into());
    }
    let mut out = Vec::new();
    for i in 0..count {
        let node = nodes + i * 16;
        let attrs = u32at(node + 4)?;
        let start = u32at(node + 8)?;
        let end = u32at(node + 12)?;
        if end < start || data.checked_add(end).ok_or("SARC offset overflow")? > total {
            return Err("SARC entry outside data".into());
        }
        let name = if attrs >> 24 == 1 {
            let off = names + (attrs & 0xffffff) * 4;
            let tail = b.get(off..data).ok_or("SARC name outside name table")?;
            let n = tail
                .iter()
                .position(|c| *c == 0)
                .ok_or("unterminated SARC name")?;
            String::from_utf8_lossy(&tail[..n]).into_owned()
        } else {
            format!("hash:{:08x}", u32at(node)?)
        };
        out.push((name, data + start, end - start));
    }
    Ok(out)
}
fn inspect_payload(
    b: &[u8],
    label: &str,
    depth: usize,
    archive: &mut BufWriter<File>,
    counts: &mut BTreeMap<String, usize>,
) -> Result<Value> {
    let sig = signature(b);
    tally(counts, sig.clone());
    if depth >= 4 {
        return Ok(json!({"signature":sig,"inspection":"depth limit"}));
    }
    if sig == "zstd-frame" {
        match mkw_formats::zstd_frame(b, 64 * 1024 * 1024) {
            Ok(d) => {
                let child = inspect_payload(&d, label, depth + 1, archive, counts)?;
                Ok(json!({"signature":sig,"decoded_size":d.len(),"payload":child}))
            }
            Err(e) => Ok(json!({"signature":sig,"inspection_error":e})),
        }
    } else if sig == "SARC" {
        match sarc_entries(b) {
            Ok(entries) => {
                for (name, off, len) in &entries {
                    let child_label = format!("{label}!{name}");
                    let desc = inspect_payload(
                        &b[*off..off + len],
                        &child_label,
                        depth + 1,
                        archive,
                        counts,
                    )?;
                    writeln!(archive,"{}",json!({"container":label,"entry":name,"size":len,"depth":depth,"inspection":desc})).map_err(|e|e.to_string())?;
                }
                Ok(json!({"signature":sig,"validated":true,"entries":entries.len()}))
            }
            Err(e) => Ok(json!({"signature":sig,"inspection_error":e})),
        }
    } else {
        // Signature plus a header prefix is evidence for later full format parsers.
        Ok(json!({"signature":sig,"header_hex":hex(&b[..b.len().min(32)]),"validated":false}))
    }
}

pub fn analyze(root: &Path, out: &Path) -> Result<()> {
    let exefs = root.join("exefs");
    let romfs = root.join("romfs");
    if !exefs.is_dir() || !romfs.is_dir() {
        return Err("root must contain observed exefs/ and romfs/ directories".into());
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut files = Vec::new();
    let mut modules = Vec::new();
    let mut npdm = None;
    let mut functions = BufWriter::new(
        File::create(out.join("function-candidates.jsonl")).map_err(|e| e.to_string())?,
    );
    let mut dependencies = Vec::new();
    for path in walk(&exefs)? {
        let raw = fs::read(&path).map_err(|e| e.to_string())?;
        let name = path
            .strip_prefix(&exefs)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        files.push(json!({"path":name,"size":raw.len(),"sha256":sha256(&raw),"magic":hex(&raw[..raw.len().min(16)])}));
        if raw.starts_with(b"NSO0") {
            let n = Nso::parse(&raw).map_err(|e| format!("{name}: {e}"))?;
            let stats = module_analysis(&n, &name, &mut functions)?;
            let mut sdk_markers = Vec::new();
            let mut references = BufWriter::new(
                File::create(out.join(format!(
                    "{}.resource-references.jsonl",
                    name.replace('/', "_")
                )))
                .map_err(|e| e.to_string())?,
            );
            for segment in &n.segments {
                if segment.name != "rodata" {
                    continue;
                }
                let mut start = 0;
                for (at, &byte) in segment.bytes.iter().enumerate() {
                    if !(32..=126).contains(&byte) {
                        if at - start >= 6 && at - start <= 512 {
                            let text = String::from_utf8_lossy(&segment.bytes[start..at]);
                            if text.starts_with("SDK MW+") {
                                sdk_markers.push(json!({"module_offset":segment.memory_offset+start,"marker":text,"confidence":"embedded SDK marker; not a retail game version"}));
                            }
                            if text.contains('/') && !text.contains('\\') {
                                let clean = text.trim_start_matches("rom:/");
                                let exists = !clean.starts_with('/')
                                    && !Path::new(clean)
                                        .components()
                                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                                    && (romfs.join(clean).is_file()
                                        || romfs.join(format!("{clean}.zs")).is_file());
                                writeln!(references,"{}",json!({"module":name,"module_offset":segment.memory_offset+start,"string":text,"kind":"string-reference-candidate","romfs_match":exists,"executed":false})).map_err(|e|e.to_string())?;
                            }
                        }
                        start = at + 1;
                    }
                }
            }
            references.flush().map_err(|e| e.to_string())?;
            if let Some(m) = &n.mod0 {
                for s in m
                    .symbols
                    .iter()
                    .filter(|s| s.section == 0 && !s.name.is_empty())
                {
                    dependencies.push(json!({"consumer":name,"symbol":s.name,"binding":s.info>>4,"type":s.info&15}));
                }
            }
            let mut val = serde_json::to_value(&n).map_err(|e| e.to_string())?;
            val["path"] = json!(name);
            val["analysis"] = stats;
            val["sdk_version_markers"] = json!(sdk_markers);
            eprintln!("verified {}: {} bytes of text", name, n.segments[0].size);
            modules.push(val);
        } else if raw.starts_with(b"META") {
            npdm = Some(serde_json::to_value(Npdm::parse(&raw)?).map_err(|e| e.to_string())?)
        }
    }
    functions.flush().map_err(|e| e.to_string())?;
    save(
        &out.join("exefs-manifest.json"),
        &json!({"schema_version":1,"identity_policy":"exact full SHA-256 + 32-byte module ID; version label unverified","software_version":null,"files":files,"npdm":npdm,"modules":modules}),
    )?;
    save(&out.join("import-map.json"), &json!(dependencies))?;
    let mut inventory =
        BufWriter::new(File::create(out.join("romfs-inventory.jsonl")).map_err(|e| e.to_string())?);
    let mut archives =
        BufWriter::new(File::create(out.join("archive-entries.jsonl")).map_err(|e| e.to_string())?);
    let mut ext = BTreeMap::new();
    let mut chains = BTreeMap::new();
    let mut magic = BTreeMap::new();
    let mut payloads = BTreeMap::new();
    let mut sample_count = BTreeMap::new();
    let mut dirs = BTreeMap::new();
    let mut total = 0u64;
    let mut count = 0;
    let mut sampled = 0;
    // Every file gets a size and header. Full payload inspection is bounded: every
    // filename-declared SARC plus eight samples per top-directory/extension-chain.
    for path in walk(&romfs)? {
        let name = path
            .strip_prefix(&romfs)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let size = path.metadata().map_err(|e| e.to_string())?.len();
        total += size;
        count += 1;
        let mut file = File::open(&path).map_err(|e| e.to_string())?;
        let mut header = vec![0u8; size.min(64) as usize];
        file.read_exact(&mut header).map_err(|e| e.to_string())?;
        let extension = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let sig = signature(&header);
        tally(&mut ext, extension.clone());
        tally(&mut magic, sig.clone());
        let top = name.split('/').next().unwrap_or("");
        tally(&mut dirs, top.to_string());
        let chain = path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .split('.')
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .join(".");
        tally(
            &mut chains,
            chain.split('.').rev().collect::<Vec<_>>().join("."),
        );
        let key = format!("{top}/{chain}/{sig}");
        let seen = sample_count.entry(key).or_insert(0usize);
        let inspect =
            (name.ends_with(".sarc") || name.ends_with(".sarc.zs") || sig == "SARC" || *seen < 8)
                && name.rsplit('/').next() != Some(".listing");
        *seen += 1;
        let mut record = json!({"path":name,"size":size,"extension":extension,"header_hex":hex(&header[..header.len().min(16)]),"signature":sig,"payload_inspected":false});
        if inspect && size <= 64 * 1024 * 1024 {
            let mut full = header;
            file.read_to_end(&mut full).map_err(|e| e.to_string())?;
            record["inspection"] = inspect_payload(&full, &name, 0, &mut archives, &mut payloads)?;
            record["payload_inspected"] = json!(true);
            sampled += 1;
        }
        writeln!(inventory, "{record}").map_err(|e| e.to_string())?;
        if count % 25000 == 0 {
            eprintln!("inventory: {count} files")
        }
    }
    inventory.flush().map_err(|e| e.to_string())?;
    archives.flush().map_err(|e| e.to_string())?;
    let listing = check_listings(root)?;
    save(
        &out.join("romfs-summary.json"),
        &json!({"files":count,"bytes":total,"extensions":ext,"extension_chains":chains,"outer_signatures":magic,"inspected_payload_signatures":payloads,"top_directories":dirs,"payload_inspected_files":sampled,"payload_policy":"every filename-declared SARC and eight samples per top-directory/last-two-extensions/signature; <=64 MiB input/output; depth <4; no full asset validation or all-file hashing","completeness":"listing comparison detects missing/size-mismatched entries only; source listings cannot establish complete retail content","listing_comparison":listing}),
    )?;
    let identity = sha256(&serde_json::to_vec(&files).map_err(|e| e.to_string())?);
    let version_dir = out
        .parent()
        .ok_or("analysis output needs a parent directory")?
        .join("versions")
        .join(&identity);
    fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
    let inventory = fs::read(out.join("romfs-inventory.jsonl")).map_err(|e| e.to_string())?;
    save(
        &version_dir.join("manifest.json"),
        &json!({"schema_version":1,"exefs_identity":identity,"software_version":null,"runtime_supported":false,"modules":modules.iter().map(|m|json!({"path":m["path"],"sha256":m["sha256"],"build_id":m["build_id"],"module_name":m["module_name"],"sdk_markers":m["sdk_version_markers"]})).collect::<Vec<_>>(),"known_function_mappings":[],"function_candidates":"../../analysis/function-candidates.jsonl","patches":[],"runtime_requirements_status":"incomplete; inspect per-module PAC, ELF relocation and service metadata","asset_expectations":{"file_count":count,"bytes":total,"header_inventory_sha256":sha256(&inventory),"full_asset_hashes_verified":false},"version_differences":null,"layers":[{"kind":"supplied-tree","version":null,"precedence":0}],"overlay_support":"not implemented"}),
    )?;
    let mut report=format!("# Private dump analysis\n\nExeFS modules: {}. NPDM parsed: {}. Each flagged segment hash verified.\n\nRomFS: {count} files, {total} bytes. Payload inspection: {sampled} files. Complete header inventory in romfs-inventory.jsonl; nested entries in archive-entries.jsonl.\n\nVersion label remains unverified. Product_100 filenames are hints, not proof. Unknown instruction classes are retained; function candidates are not confirmed boundaries. See exefs-manifest.json and function-candidates.jsonl. No guest code executed.\n",modules.len(),npdm.is_some());
    report.push_str("\n## Executable identities and requirements\n\n");
    for module in &modules {
        report.push_str(&format!("### {} ({})\n\n- Full-file SHA-256: `{}`\n- Full module ID: `{}`\n- Dependencies: `{}`\n- RELA records: {}; RELR locations: {}\n- Unwind-index records: {}; function candidates: {}\n\n| Segment | Module offset | Decoded bytes | Stored bytes | Permissions | Hash checked |\n|---|---:|---:|---:|---|---|\n",module["path"].as_str().unwrap_or("?"),module["module_name"].as_str().unwrap_or("?"),module["sha256"].as_str().unwrap_or("?"),module["build_id"].as_str().unwrap_or("?"),module["mod0"]["needed"],module["mod0"]["relocations"].as_array().map_or(0,Vec::len),module["mod0"]["relr_offsets"].as_array().map_or(0,Vec::len),module["analysis"]["unwind_index"].as_array().map_or(0,Vec::len),module["analysis"]["function_candidates"]));
        if let Some(segments) = module["segments"].as_array() {
            for segment in segments {
                report.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} |\n",
                    segment["name"].as_str().unwrap_or("?"),
                    segment["memory_offset"],
                    segment["size"],
                    segment["stored_size"],
                    segment["permissions"].as_str().unwrap_or("?"),
                    segment["embedded_hash_checked"]
                ));
            }
        }
        report.push_str(&format!("\nBSS size: {} bytes. SDK-marker candidates: `{}`.\n\nInstruction-class observations: `{}`. Linear scan includes embedded data; this is not executed coverage.\n\n",module["bss_size"],module["sdk_version_markers"],module["analysis"]["diagnostic_classes"]));
    }
    report.push_str("## Resource and completeness limits\n\nThe inventory records every supplied file header. SARC entries establish containment only. Signature candidates are not full format validation. See romfs-summary.json for all outer/inspected signatures, extension chains, directory counts, inspection limits and listing comparisons. Payloads beyond the inspection limit remain unresolved; this is not evidence of corruption. No complete retail-content reference or original-hardware execution was available.\n\nThe observed bootstrap imports and NVN2 resource names guide graphics investigation, but shader ISA, command submission, resource layouts and actual rendering are unverified. No game code, service initialization or native graphics work has executed.\n");
    fs::write(out.join("REPORT.md"), report).map_err(|e| e.to_string())?;
    eprintln!(
        "analysis complete: {count} files, {total} bytes; output {}",
        out.display()
    );
    Ok(())
}

fn module_analysis(n: &Nso, name: &str, functions: &mut BufWriter<File>) -> Result<Value> {
    let text = &n.segments[0];
    let lo = text.memory_offset as u64;
    let hi = lo + text.size as u64;
    let mut classes = BTreeMap::new();
    let mut histogram: BTreeMap<String, usize> = BTreeMap::new();
    let mut supported = 0;
    let mut candidates: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
    let mut svc = BTreeMap::new();
    let mut indirect = Vec::new();
    let mut unwind_status = json!("absent");
    let mut unwind_entries = Vec::new();
    let mut candidate = |at: u64, reason: &'static str| {
        if at >= lo && at < hi && at % 4 == 0 {
            let v = candidates.entry(at).or_default();
            if !v.contains(&reason) {
                v.push(reason)
            }
        }
    };
    if let Some(m) = &n.mod0 {
        if m.eh_end > m.eh_start {
            let header = n.read(m.eh_start, m.eh_end - m.eh_start)?;
            if header.len() >= 12 && header[..4] == [1, 0x1b, 3, 0x3b] {
                let count = mkw_formats::u32le(header, 8)? as usize;
                mkw_formats::slice(
                    header,
                    12,
                    count.checked_mul(8).ok_or("unwind count overflow")?,
                )?;
                let mut previous = None;
                for i in 0..count {
                    let at = 12 + i * 8;
                    let start = m.eh_start as i64 + mkw_formats::u32le(header, at)? as i32 as i64;
                    let fde = m.eh_start as i64 + mkw_formats::u32le(header, at + 4)? as i32 as i64;
                    if start < 0 || fde < 0 || previous.is_some_and(|p| p > start) {
                        return Err("invalid unwind index ordering/address".into());
                    }
                    n.read(fde as usize, 8)?;
                    previous = Some(start);
                    candidate(start as u64, "eh_frame_hdr-index");
                    unwind_entries.push(json!({"start":start,"fde":fde}));
                }
                unwind_status = json!(
                    "index validated; CIE/FDE extent validation available through cpu-survey; unwind program execution not implemented"
                );
            } else {
                unwind_status = json!("unsupported header encoding; retained as unresolved");
            }
        }
        for s in &m.symbols {
            if s.section != 0 && s.info & 15 == 2 {
                candidate(s.value, "dynamic-FUNC-symbol")
            }
        }
        for r in &m.relocations {
            if r.kind == 1027 && r.addend >= 0 {
                candidate(r.addend as u64, "RELA-relative-addend")
            }
        }
        for &off in &m.relr_offsets {
            let value = mkw_formats::u64le(n.read(off as usize, 8)?, 0)?;
            candidate(value, "RELR-pointer-to-text")
        }
        for tag in [12, 13] {
            if let Some(v) = m.tags.get(&tag) {
                for &a in v {
                    candidate(a, "DT_INIT-or-FINI")
                }
            }
        }
    }
    for (i, b) in text.bytes.chunks_exact(4).enumerate() {
        let w = u32::from_le_bytes(b.try_into().unwrap());
        let pc = lo + (i * 4) as u64;
        let class = mkw_arm64::classify(w);
        tally(&mut classes, class);
        tally(&mut histogram, format!("{:03x}", w >> 21));
        if mkw_arm64::decode(w, pc).is_ok() {
            supported += 1
        }
        if class == "BL-immediate" {
            let imm = ((w & 0x3ffffff) as i64) << 38 >> 38;
            candidate(pc.wrapping_add((imm * 4) as u64), "BL-scan-candidate")
        }
        if class == "PACIASP" || class == "PACIBSP" {
            candidate(pc, "PAC-prologue-candidate")
        }
        if class == "SVC" {
            tally(&mut svc, format!("0x{:x}", (w >> 5) & 65535))
        }
        if class == "BR-register" || class == "BLR-register" {
            indirect
                .push(json!({"pc":pc,"kind":class,"register":(w>>5)&31,"resolution":"unresolved"}))
        }
    }
    for (pc, reasons) in &candidates {
        writeln!(functions,"{}",json!({"module":name,"module_offset":pc,"sources":reasons,"confidence":"candidate-only","extent":null})).map_err(|e|e.to_string())?
    }
    Ok(
        json!({"text_words":text.size/4,"semantic_subset_decodable_words":supported,"diagnostic_classes":classes,"top11_opcode_histogram":histogram,"unwind_status":unwind_status,"unwind_index":unwind_entries,"function_candidates":candidates.len(),"svc_immediates":svc,"unresolved_indirect_sites":indirect,"scan_limitations":"linear text scan includes embedded data; no proven reachability, extents, instruction coverage or complete PAC survey; full CFI execution and indirect-target recovery pending"}),
    )
}
fn check_listings(root: &Path) -> Result<Value> {
    let mut checked = 0;
    let mut malformed = Vec::new();
    let mut mismatches = Vec::new();
    for path in walk(root)?
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|s| s == ".listing"))
    {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for line in text.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 9 {
                malformed.push(format!("{}: {line}", path.display()));
                continue;
            }
            let name = fields[8..].join(" ");
            if name == "." || name == ".." {
                continue;
            }
            let p = path.parent().unwrap().join(&name);
            checked += 1;
            match fs::symlink_metadata(&p) {
                Ok(m) => {
                    let size = fields[4].parse::<u64>().ok();
                    if fields[0].starts_with('-') && (!m.is_file() || Some(m.len()) != size) {
                        mismatches
                            .push(json!({"path":p,"expected_size":size,"actual_size":m.len()}))
                    } else if fields[0].starts_with('d') && !m.is_dir() {
                        mismatches.push(json!({"path":p,"error":"expected directory"}))
                    }
                }
                Err(e) => mismatches.push(json!({"path":p,"error":e.to_string()})),
            }
        }
    }
    Ok(json!({"entries_checked":checked,"mismatches":mismatches,"unparsed_lines":malformed}))
}

pub fn verify(root: &Path, out: &Path) -> Result<()> {
    let manifest: Value = serde_json::from_slice(
        &fs::read(out.join("exefs-manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let expected = manifest["files"].as_array().ok_or("bad manifest")?;
    let actual = walk(&root.join("exefs"))?;
    if actual.len() != expected.len() {
        return Err("ExeFS file set changed".into());
    }
    for f in expected {
        let name = f["path"].as_str().ok_or("bad manifest path")?;
        if Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("unsafe manifest path".into());
        }
        let b = fs::read(root.join("exefs").join(name)).map_err(|e| e.to_string())?;
        if f["sha256"].as_str() != Some(&sha256(&b)) {
            return Err(format!("{name}: full-file SHA-256 differs from manifest"));
        }
        if b.starts_with(b"NSO0") {
            Nso::parse(&b)?;
        }
    }
    eprintln!("verified exact ExeFS file set, hashes, and NSO segment hashes; RomFS content is not hash-verified");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sarc() -> Vec<u8> {
        // A tiny independently constructed little-endian archive with one file.
        let mut b = vec![0u8; 65];
        b[..4].copy_from_slice(b"SARC");
        b[4..6].copy_from_slice(&20u16.to_le_bytes());
        b[6..8].copy_from_slice(&[0xff, 0xfe]);
        b[8..12].copy_from_slice(&65u32.to_le_bytes());
        b[12..16].copy_from_slice(&64u32.to_le_bytes());
        b[16..18].copy_from_slice(&0x100u16.to_le_bytes());
        b[20..24].copy_from_slice(b"SFAT");
        b[24..26].copy_from_slice(&12u16.to_le_bytes());
        b[26..28].copy_from_slice(&1u16.to_le_bytes());
        b[36..40].copy_from_slice(&0x1000000u32.to_le_bytes());
        b[44..48].copy_from_slice(&1u32.to_le_bytes());
        b[48..52].copy_from_slice(b"SFNT");
        b[52..54].copy_from_slice(&8u16.to_le_bytes());
        b[56..58].copy_from_slice(b"a\0");
        b[64] = 42;
        b
    }
    #[test]
    fn validate_sarc() {
        let b = sarc();
        assert_eq!(sarc_entries(&b).unwrap(), vec![("a".into(), 64, 1)]);
        for len in 0..b.len() {
            assert!(sarc_entries(&b[..len]).is_err())
        }
    }
    #[test]
    fn sarc_bounds() {
        let mut b = sarc();
        b[44..48].copy_from_slice(&100u32.to_le_bytes());
        assert!(sarc_entries(&b).is_err());
        b = sarc();
        b[16] = 1;
        assert!(sarc_entries(&b).is_err())
    }
}
