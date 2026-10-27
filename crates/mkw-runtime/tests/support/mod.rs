#![allow(dead_code)]
use mkw_runtime::{abi::Cpu, bridge::MemoryBridge, GuestAddr, Memory, Permissions};
use std::path::Path;
pub struct Program {
    library: libloading::Library,
}
impl Program {
    pub fn build(label: &str, words: &[u32], optimization: &str) -> Self {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let ir = mkw_arm64::lift(&bytes, 0, 0).unwrap();
        let mut code = mkw_codegen::emit(&ir, 0).unwrap();
        code+="\nsize_t vector_offset(void){return offsetof(MkwCpu,vregs);}\nsize_t cpu_size(void){return sizeof(MkwCpu);}\nunsigned abi_version(void){return MKW_CPU_ABI_VERSION;}\n";
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let folder = root
            .join("private/testing")
            .join(label)
            .join(&optimization[1..]);
        std::fs::create_dir_all(&folder).unwrap();
        let source = folder.join("generated.c");
        let library = folder.join("generated.so");
        std::fs::write(&source, code).unwrap();
        let build = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
                "-shared",
                "-fPIC",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        // SAFETY: the library was compiled above from our synthetic IR and shared ABI.
        let library = unsafe { libloading::Library::new(library) }.unwrap();
        unsafe {
            assert_eq!(
                library
                    .get::<unsafe extern "C" fn() -> usize>(b"vector_offset\0")
                    .unwrap()(),
                std::mem::offset_of!(Cpu, vregs)
            );
            assert_eq!(
                library
                    .get::<unsafe extern "C" fn() -> usize>(b"cpu_size\0")
                    .unwrap()(),
                std::mem::size_of::<Cpu>()
            );
            assert_eq!(
                library
                    .get::<unsafe extern "C" fn() -> u32>(b"abi_version\0")
                    .unwrap()(),
                mkw_runtime::abi::CPU_ABI_VERSION
            );
        }
        Self { library }
    }
    /// # Safety
    /// CPU callbacks and contexts must satisfy native/cpu.h for the entire call.
    pub unsafe fn run(&self, cpu: &mut Cpu) -> i32 {
        // SAFETY: the checked synthetic library implements our exact ABI. Caller
        // supplies a live CPU with contexts retained by pinned attachments.
        unsafe {
            self.library
                .get::<unsafe extern "C" fn(*mut Cpu, u64) -> i32>(b"mkw_guest\0")
                .unwrap()(cpu, 10000)
        }
    }
}
pub fn memory() -> std::pin::Pin<Box<MemoryBridge>> {
    let mut memory = Memory::default();
    for (base, len, permissions, name) in [
        (0x1000, 128, Permissions::RW, "a"),
        (0x1080, 128, Permissions::RW, "b"),
        (0x2000, 64, Permissions::R, "ro"),
        (0x8000, 64, Permissions::RW, "literal"),
    ] {
        memory
            .map(GuestAddr(base), vec![0; len], permissions, name)
            .unwrap();
    }
    memory.seal();
    MemoryBridge::new(memory)
}
pub fn transfer(load: bool, bytes: u8, vector: bool, rt: u32, rn: u32, address: u32) -> u32 {
    let (size, opc) = if vector && bytes == 16 {
        (0, 2 + u32::from(load))
    } else {
        (bytes.trailing_zeros(), u32::from(load))
    };
    (size << 30) | (u32::from(vector) << 26) | (opc << 22) | (rn << 5) | rt | address
}
pub fn pair(
    load: bool,
    bytes: u8,
    vector: bool,
    registers: [u32; 2],
    rn: u32,
    mode: u32,
    offset: i32,
) -> u32 {
    let [rt, rt2] = registers;
    let opc = if vector {
        bytes.trailing_zeros() - 2
    } else if bytes == 8 {
        2
    } else {
        0
    };
    0x28000000
        | (opc << 30)
        | (u32::from(vector) << 26)
        | (mode << 23)
        | (u32::from(load) << 22)
        | (((offset / (i32::from(bytes))) as u32 & 127) << 15)
        | (rt2 << 10)
        | (rn << 5)
        | rt
}
