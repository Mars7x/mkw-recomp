#[path = "support/a64_reference.rs"]
mod reference;
use mkw_arm64::{simd::Simd, Op};
use mkw_codegen::{emit, HEADER};
use std::{collections::BTreeSet, path::Path, process::Command};
fn cases() -> Vec<u32> {
    let mut words = BTreeSet::new();
    for ftype in [0, 1, 3] {
        for src in [0, 1, 31] {
            words.insert(0x1e204000 | (ftype << 22) | (src << 5));
        }
        for imm in 0..256 {
            words.insert(0x1e201000 | (ftype << 22) | (imm << 13));
        }
        for sf in 0..2 {
            for direction in 0..2 {
                for src in [0, 1, 31] {
                    words.insert(
                        0x1e260000 | (ftype << 22) | (sf << 31) | (direction << 16) | (src << 5),
                    );
                }
            }
        }
    }
    for direction in 0..2 {
        for src in [0, 1, 31] {
            words.insert(0x9eae0000 | (direction << 16) | (src << 5));
        }
    }
    for q in 0..2 {
        for op in 0..2 {
            for mode in 0..16 {
                for o2 in 0..2 {
                    for imm in [0, 1, 0x3f, 0x40, 0x70, 0x80, 0xa5, 0xff] {
                        words.insert(
                            0x0f000400
                                | (q << 30)
                                | (op << 29)
                                | (mode << 12)
                                | (o2 << 11)
                                | ((imm >> 5) << 16)
                                | ((imm & 31) << 5),
                        );
                    }
                }
            }
        }
    }
    for q in 0..2 {
        for op in 0..2 {
            for imm5 in 0..32 {
                for imm4 in [0, 1, 3, 5, 7, 8, 12, 15] {
                    for src in [0, 1, 31] {
                        words.insert(
                            0x0e000400
                                | (q << 30)
                                | (op << 29)
                                | (imm5 << 16)
                                | (imm4 << 11)
                                | (src << 5),
                        );
                    }
                }
            }
        }
        for op in 0..8 {
            for (rn, rm) in [(1, 2), (0, 0), (31, 1), (1, 0)] {
                words.insert(
                    0x0e201c00
                        | (q << 30)
                        | ((op >> 2) << 29)
                        | ((op & 3) << 22)
                        | (rm << 16)
                        | (rn << 5),
                );
            }
        }
        for offset in 0..16 {
            for (rn, rm) in [(1, 2), (0, 0), (31, 1)] {
                words.insert(0x2e000000 | (q << 30) | (rm << 16) | (offset << 11) | (rn << 5));
            }
        }
    }
    for imm5 in 1..32 {
        for src in [0, 1, 31] {
            words.insert(0x5e000400 | (imm5 << 16) | (src << 5));
        }
    }
    words
        .into_iter()
        .filter(|&w| matches!(mkw_arm64::decode(w,0),Ok(i)if matches!(i.op,Op::Simd(_))))
        .collect()
}
#[test]
fn raw_simd_native_matches_independent_qemu_execution() {
    let words = cases();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/simd-reference");
    std::fs::create_dir_all(&folder).unwrap();
    if Command::new("qemu-aarch64")
        .arg("--version")
        .output()
        .is_err()
    {
        std::fs::write(
            folder.join("receipt.json"),
            "{\"qemu_reference\":\"unavailable\",\"hardware_reference\":false}\n",
        )
        .unwrap();
        eprintln!("optional qemu-aarch64 reference unavailable; no comparison performed");
        return;
    }
    let elf = folder.join("synthetic-aarch64");
    std::fs::write(&elf, reference::elf(&words)).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&elf, std::fs::Permissions::from_mode(0o700)).unwrap();
    let oracle = Command::new("qemu-aarch64")
        .args(["-cpu", "max"])
        .current_dir(&folder)
        .arg(&elf)
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "QEMU {:?}: {}",
        oracle.status.code(),
        String::from_utf8_lossy(&oracle.stderr)
    );
    assert_eq!(oracle.stdout.len(), words.len() * 32);
    let mut code = String::from(HEADER);
    code+="\n#include <stdio.h>\nstatic void output(uint64_t v){for(unsigned i=0;i<8;i++)putchar((int)((v>>(i*8))&255));}\n";
    let mut names = Vec::new();
    for (index, &word) in words.iter().enumerate() {
        let bytes: Vec<_> = [word, 0xd65f03c0]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let name = format!("fixture{index}");
        code += &emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
            .unwrap()
            .replace(HEADER, "")
            .replace("mkw_guest", &name);
        names.push(name);
    }
    let mut main=format!("int main(void){{int(*functions[])(MkwCpu*,uint64_t)={{{}}};MkwCpu c;for(unsigned i=0;i<sizeof(functions)/sizeof(functions[0]);i++){{\n",names.join(","));
    main += "c=(MkwCpu){0};c.sp=99;c.n=1;c.z=0;c.c=1;c.v=1;\n";
    for (reg, value) in reference::INITIAL_X.into_iter().enumerate() {
        main += &format!("c.x[{reg}]=UINT64_C({value});\n");
    }
    for (reg, values) in [0, 1, 2, 31].into_iter().zip(reference::INITIAL_V) {
        for (half, value) in values.into_iter().enumerate() {
            main += &format!("c.vregs[{reg}][{half}]=UINT64_C({value});\n");
        }
    }
    main+="if(functions[i](&c,10)||c.sp!=99){return 2;}\noutput(c.vregs[0][0]);output(c.vregs[0][1]);output(c.x[0]);output(((uint64_t)c.n<<31)|((uint64_t)c.z<<30)|((uint64_t)c.c<<29)|((uint64_t)c.v<<28));\n}return ferror(stdout)?3:0;}\n";
    let src = folder.join("native.c");
    std::fs::write(&src, code + &main).unwrap();
    for opt in ["-O0", "-O2"] {
        let exe = folder.join(format!("native{}", &opt[1..]));
        let build = Command::new("cc")
            .args([
                "-std=c11",
                opt,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
            ])
            .arg(&src)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let native = Command::new(&exe).output().unwrap();
        assert!(
            native.status.success(),
            "{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(native.stdout.len(), oracle.stdout.len());
        for (i, (actual, expected)) in native
            .stdout
            .chunks_exact(32)
            .zip(oracle.stdout.chunks_exact(32))
            .enumerate()
        {
            assert_eq!(
                actual,
                expected,
                "word {:08x} {:?}, {opt}, fixture {i}",
                words[i],
                mkw_arm64::decode(words[i], 0).unwrap().op
            );
        }
    }
    let mut families = std::collections::BTreeMap::new();
    for &w in &words {
        let Op::Simd(op) = mkw_arm64::decode(w, 0).unwrap().op else {
            unreachable!()
        };
        let kind = match op {
            Simd::Immediate { .. } => "immediate",
            Simd::Scalar { .. } => "scalar",
            Simd::Duplicate { .. } => "duplicate",
            Simd::Insert { .. } => "insert",
            Simd::Extract { .. } => "extract",
            Simd::Logical { .. } => "logical",
            Simd::ExtractVector { .. } => "ext",
            Simd::Permute { .. } => "permutation",
            Simd::Table { .. } => "table",
            Simd::ByteCount { .. } => "byte_count",
            Simd::Transform { .. } => "transform",
        };
        *families.entry(kind).or_insert(0usize) += 1;
    }
    let qemu_version = String::from_utf8(
        Command::new("qemu-aarch64")
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let report = serde_json::json!({"qemu_version":qemu_version.lines().next(),"reference":"qemu-aarch64 -cpu max","synthetic_cases":words.len(),"families":families,"native_optimizations":["O0","O2"],"fatal_ubsan":true,"game_code_executed":false,"hardware_reference":false});
    std::fs::write(
        folder.join("receipt.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
