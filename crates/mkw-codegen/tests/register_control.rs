use mkw_arm64::lift;
use mkw_codegen::{emit, HEADER};
#[test]
fn shifted_flags_zero_register_and_bit_branches_execute_natively() {
    let mut source = HEADER.to_string();
    let mut main = String::from("int main(void){MkwCpu c;\n");
    let cases = [
        (
            0x2b020420u32,
            "c.x[1]=0x7ffffffe;c.x[2]=1;",
            "c.x[0]!=0x80000000||!c.n||c.z||c.c||!c.v",
        ),
        (
            0xeb02003f,
            "c.x[0]=55;c.x[1]=UINT64_C(0x8000000000000000);c.x[2]=1;",
            "c.x[0]!=55||c.n||c.z||!c.c||!c.v",
        ),
        (
            0x8b82fc20,
            "c.x[1]=3;c.x[2]=UINT64_C(0x8000000000000000);c.n=1;c.c=1;",
            "c.x[0]!=2||!c.n||!c.c",
        ),
        (
            0x4b427c20,
            "c.x[0]=UINT64_MAX;c.x[1]=1;c.x[2]=UINT64_C(0xffffffff80000000);",
            "c.x[0]!=0",
        ),
        (
            0x0b027fff,
            "c.sp=99;c.x[0]=123;c.x[2]=3;",
            "c.sp!=99||c.x[0]!=123",
        ),
        (
            0xeb020020,
            "c.x[1]=0;c.x[2]=0;",
            "c.x[0]||c.n||!c.z||!c.c||c.v",
        ),
    ];
    for (i, (word, init, check)) in cases.iter().enumerate() {
        let bytes: Vec<_> = [*word, 0xd65f03c0]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        source += &emit(&lift(&bytes, 0, 0).unwrap(), 0)
            .unwrap()
            .replace(HEADER, "")
            .replace("mkw_guest", &format!("f{i}"));
        main += &format!(
            "c=(MkwCpu){{0}};{init}if(f{i}(&c,10)||({check}))return {};\n",
            i + 1
        );
    }
    let words = [
        0xb6f80060u32,
        0x528000a1,
        0x14000002,
        0x528000e1,
        0xd65f03c0,
    ];
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    source += &emit(&lift(&bytes, 0, 0).unwrap(), 0)
        .unwrap()
        .replace(HEADER, "")
        .replace("mkw_guest", "bit_test");
    main+="c=(MkwCpu){0};c.x[0]=UINT64_C(1)<<63;if(bit_test(&c,10)||c.x[1]!=5)return 10;\nc=(MkwCpu){0};c.x[0]=UINT64_C(1)<<62;if(bit_test(&c,10)||c.x[1]!=7)return 11;return 0;}\n";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/register-control");
    std::fs::create_dir_all(&folder).unwrap();
    let src = folder.join("test.c");
    let exe = folder.join("test");
    std::fs::write(&src, source + &main).unwrap();
    for optimization in ["-O0", "-O2"] {
        let build = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
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
        let run = std::process::Command::new(&exe).output().unwrap();
        assert!(
            run.status.success(),
            "{:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}
