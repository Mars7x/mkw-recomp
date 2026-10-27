use mkw_arm64::lift;
use mkw_codegen::{emit, HEADER};
#[test]
fn all_conditional_select_variants_preserve_flags() {
    let mut code = HEADER.to_string();
    let mut harness=String::from("int main(void){MkwCpu c;const unsigned masks[16]={0xf0f0,0x0f0f,0xcccc,0x3333,0xff00,0x00ff,0xaaaa,0x5555,0x0c0c,0xf3f3,0xaa55,0x55aa,0x0a05,0xf5fa,0xffff,0xffff};\n");
    for width in 0..2u32 {
        for operation in 0..4u32 {
            for condition in 0..16u32 {
                let word = 0x1a800000
                    | (width << 31)
                    | ((operation >> 1) << 30)
                    | ((operation & 1) << 10)
                    | (condition << 12)
                    | (2 << 16)
                    | (1 << 5);
                let bytes: Vec<_> = [word, 0xd65f03c0]
                    .iter()
                    .flat_map(|w| w.to_le_bytes())
                    .collect();
                let name = format!("select_{width}_{operation}_{condition}");
                code += &emit(&lift(&bytes, 0, 0).unwrap(), 0)
                    .unwrap()
                    .replace(HEADER, "")
                    .replace("mkw_guest", &name);
                let lhs = if width == 1 {
                    "UINT64_C(0x0123456789abcdef)"
                } else {
                    "UINT64_C(0x89abcdef)"
                };
                let rhs = match operation {
                    0 => {
                        if width == 1 {
                            "UINT64_MAX"
                        } else {
                            "UINT64_C(0xffffffff)"
                        }
                    }
                    1 | 2 => "UINT64_C(0)",
                    _ => "UINT64_C(1)",
                };
                harness+=&format!("for(unsigned f=0;f<16;f++){{c=(MkwCpu){{0}};c.x[1]=UINT64_C(0x0123456789abcdef);c.x[2]=UINT64_MAX;c.n=f>>3;c.z=(f>>2)&1;c.c=(f>>1)&1;c.v=f&1;uint64_t expected=(masks[{condition}]&(1u<<f))?{lhs}:{rhs};if({name}(&c,10)||c.x[0]!=expected||c.n!=(f>>3)||c.z!=((f>>2)&1)||c.c!=((f>>1)&1)||c.v!=(f&1))return 1;}}\n");
            }
        }
    }
    // CSINC W0,WZR,WZR,NE is the CSET W0,EQ alias.
    let bytes: Vec<_> = [0x1a9f17e0u32, 0xd65f03c0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    code += &emit(&lift(&bytes, 0, 0).unwrap(), 0)
        .unwrap()
        .replace(HEADER, "")
        .replace("mkw_guest", "cset");
    harness += "c=(MkwCpu){0};c.z=1;if(cset(&c,10)||c.x[0]!=1)return 2;return 0;}\n";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/select");
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("test.c");
    let exe = folder.join("test");
    std::fs::write(&source, code + &harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let run = std::process::Command::new(&exe).output().unwrap();
        assert!(
            run.status.success(),
            "{:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}
