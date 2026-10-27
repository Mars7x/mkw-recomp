use mkw_codegen::{emit, HEADER};
#[test]
fn conditional_compares_all_conditions_flags_widths_and_forms() {
    // Condition truth table indexed by NZCV, independent of production expression generation.
    let conditions = [
        0xf0f0u16, 0x0f0f, 0xcccc, 0x3333, 0xff00, 0x00ff, 0xaaaa, 0x5555, 0x0c0c, 0xf3f3, 0xaa55,
        0x55aa, 0x0a05, 0xf5fa, 0xffff, 0xffff,
    ];
    let values = [
        0u64,
        1,
        31,
        32,
        0x7fffffff,
        0x80000000,
        0xffffffff,
        0x7fffffffffffffff,
        0x8000000000000000,
        u64::MAX,
    ];
    let mut code = HEADER.to_string();
    let mut harness = String::from("int main(void){MkwCpu c;\n");
    for sf in 0..2u32 {
        for sub in 0..2u32 {
            for imm in 0..2u32 {
                for cond in 0..16u32 {
                    let word = 0x3a400000
                        | (sf << 31)
                        | (sub << 30)
                        | (imm << 11)
                        | (cond << 12)
                        | (2 << 16)
                        | (1 << 5)
                        | cond;
                    let bytes: Vec<_> = [word, 0xd65f03c0]
                        .iter()
                        .flat_map(|w| w.to_le_bytes())
                        .collect();
                    let name = format!("compare_{sf}_{sub}_{imm}_{cond}");
                    code += &emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
                        .unwrap()
                        .replace(HEADER, "")
                        .replace("mkw_guest", &name);
                    let bits = 32u32 << sf;
                    let mask = (1u128 << bits) - 1;
                    let mut expected = Vec::new();
                    for f in 0..16 {
                        for (j, &a) in values.iter().enumerate() {
                            let b = if imm == 1 {
                                2
                            } else {
                                values[(j + 3) % values.len()]
                            };
                            let a128 = u128::from(a) & mask;
                            let b128 = u128::from(b) & mask;
                            let result = if sub == 1 {
                                a128.wrapping_sub(b128) & mask
                            } else {
                                (a128 + b128) & mask
                            };
                            let signed = |n: u128| {
                                if n >> (bits - 1) != 0 {
                                    (n as i128) - (1i128 << bits)
                                } else {
                                    n as i128
                                }
                            };
                            let exact = if sub == 1 {
                                signed(a128) - signed(b128)
                            } else {
                                signed(a128) + signed(b128)
                            };
                            let carry = if sub == 1 {
                                a128 >= b128
                            } else {
                                a128 + b128 > mask
                            };
                            let overflow =
                                exact < -(1i128 << (bits - 1)) || exact >= (1i128 << (bits - 1));
                            let flags = ((result >> (bits - 1)) as u32) * 8
                                + u32::from(result == 0) * 4
                                + u32::from(carry) * 2
                                + u32::from(overflow);
                            expected.push(if conditions[cond as usize] & (1 << f) != 0 {
                                flags
                            } else {
                                cond
                            });
                        }
                    }
                    harness+=&format!("{{const unsigned expected[]={{{}}};const uint64_t values[]={{{}}};for(unsigned f=0;f<16;f++)for(unsigned j=0;j<10;j++){{c=(MkwCpu){{0}};c.x[1]=values[j];c.x[2]=values[(j+3)%10];c.sp=99;c.x[30]=0xdead;c.n=f>>3;c.z=(f>>2)&1;c.c=(f>>1)&1;c.v=f&1;if({name}(&c,10)||((c.n<<3)|(c.z<<2)|(c.c<<1)|c.v)!=expected[f*10+j]||c.x[1]!=values[j]||c.x[2]!=values[(j+3)%10]||c.sp!=99||c.pc!=0xdead)return 1;}}}}\n",expected.iter().map(u32::to_string).collect::<Vec<_>>().join(","),values.iter().map(|v|format!("UINT64_C({v})")).collect::<Vec<_>>().join(","));
                }
            }
        }
    }
    // Force every fallback value, including AL/NV values which cannot be
    // reached as false conditions above. XZR must not read SP.
    for nzcv in 0..16u32 {
        let word = 0xfa5f0be0 | nzcv; // CCMP XZR,#31,#nzcv,EQ
        let bytes: Vec<_> = [word, 0xd65f03c0]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let name = format!("fallback_{nzcv}");
        code += &emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
            .unwrap()
            .replace(HEADER, "")
            .replace("mkw_guest", &name);
        harness+=&format!("c=(MkwCpu){{0}};c.sp=99;if({name}(&c,10)||((c.n<<3)|(c.z<<2)|(c.c<<1)|c.v)!={nzcv}||c.sp!=99)return 2;\n");
        harness+=&format!("c=(MkwCpu){{0}};c.sp=99;c.z=1;if({name}(&c,10)||((c.n<<3)|(c.z<<2)|(c.c<<1)|c.v)!=8||c.sp!=99)return 3;\n");
    }
    harness += "return 0;}\n";
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/conditional-compare");
    std::fs::create_dir_all(&folder).unwrap();
    let src = folder.join("test.c");
    let exe = folder.join("test");
    std::fs::write(&src, code + &harness).unwrap();
    for opt in ["-O0", "-O2"] {
        let build = std::process::Command::new("cc")
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
        assert!(std::process::Command::new(&exe).status().unwrap().success());
    }
}
