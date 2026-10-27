use mkw_runtime::abi::Cpu;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
pub fn generate(words: &[u32], name: &str) -> String {
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    mkw_codegen::emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
        .unwrap()
        .replace(mkw_codegen::HEADER, "")
        .replace("mkw_guest", name)
}
pub fn folder(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing")
        .join(name);
    std::fs::create_dir_all(&p).unwrap();
    p
}
pub fn build(code: &str, path: &Path, opt: &str) -> libloading::Library {
    let src = path.join("generated.c");
    std::fs::write(&src,format!("{}\n{code}\nsize_t cpu_size(void){{return sizeof(MkwCpu);}}\nsize_t fp_offset(void){{return offsetof(MkwCpu,fpcr);}}\nsize_t barrier_offset(void){{return offsetof(MkwCpu,barrier);}}
size_t convert_offset(void){{return offsetof(MkwCpu,convert);}}\nunsigned abi_version(void){{return MKW_CPU_ABI_VERSION;}}",mkw_codegen::HEADER)).unwrap();
    let lib = path.join(format!("native{}.so", &opt[1..]));
    let result = Command::new("cc")
        .args([
            "-std=c11",
            opt,
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=undefined",
        ])
        .arg(&src)
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // SAFETY: locally compiled synthetic ABI-v9 functions; check layout before calls.
    unsafe {
        let library = libloading::Library::new(lib).unwrap();
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"cpu_size\0")
                .unwrap()(),
            std::mem::size_of::<Cpu>()
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"fp_offset\0")
                .unwrap()(),
            std::mem::offset_of!(Cpu, fpcr)
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"barrier_offset\0")
                .unwrap()(),
            std::mem::offset_of!(Cpu, barrier)
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> usize>(b"convert_offset\0")
                .unwrap()(),
            std::mem::offset_of!(Cpu, convert)
        );
        assert_eq!(
            library
                .get::<unsafe extern "C" fn() -> u32>(b"abi_version\0")
                .unwrap()(),
            mkw_runtime::abi::CPU_ABI_VERSION
        );
        library
    }
}
#[allow(dead_code)] // Shared with non-FP native fixtures.
pub fn backend(path: &Path, opt: &str) -> libloading::Library {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = path.join(&opt[1..]);
    let build = Command::new("python3")
        .arg(root.join("tools/build_fp.py"))
        .arg("--output")
        .arg(&output)
        .arg("--optimization")
        .arg(&opt[1..])
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    // SAFETY: built from the verified BSD subset and our exact callback adapter.
    unsafe { libloading::Library::new(output.join("fp.so")).unwrap() }
}
pub type Entry = unsafe extern "C" fn(*mut Cpu, u64) -> i32;
