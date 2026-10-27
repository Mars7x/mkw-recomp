use mkw_arm64::lift;
use mkw_codegen::{emit, HEADER};
struct Cases {
    code: String,
    main: String,
    count: usize,
}
impl Cases {
    fn new() -> Self {
        Self {
            code: HEADER.into(),
            main: "int main(void){MkwCpu c;\n".into(),
            count: 0,
        }
    }
    fn function(&mut self, word: u32) -> String {
        let name = format!("case{}", self.count);
        self.count += 1;
        let bytes: Vec<_> = [word, 0xd65f03c0]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        self.code += &emit(&lift(&bytes, 0, 0).unwrap(), 0)
            .unwrap()
            .replace(HEADER, "")
            .replace("mkw_guest", &name);
        name
    }
    fn check(&mut self, name: &str, init: &str, check: &str) {
        self.main += &format!("c=(MkwCpu){{0}};{init}if({name}(&c,10)||({check}))return 1;\n");
    }
    fn run(mut self, label: &str) {
        self.main += "return 0;}\n";
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let folder = root.join("private/testing").join(label);
        std::fs::create_dir_all(&folder).unwrap();
        let src = folder.join("test.c");
        let exe = folder.join("test");
        std::fs::write(&src, self.code + &self.main).unwrap();
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
            let run = std::process::Command::new(&exe).output().unwrap();
            assert!(
                run.status.success(),
                "{:?}: {}",
                run.status.code(),
                String::from_utf8_lossy(&run.stderr)
            );
        }
    }
}
fn mask(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
/// Independent field-copy oracle, without the production rotate/write-mask algorithm.
fn field_reference(bits: u32, source: u64, old: u64, r: u32, s: u32, op: u32) -> u64 {
    let mut out = if op == 1 { old & mask(bits) } else { 0 };
    let start = if s >= r { r } else { 0 };
    let mut highest = 0;
    for bit in start..=s {
        let target = (bit + bits - r) % bits;
        highest = highest.max(target);
        out = (out & !(1u64 << target)) | (((source >> bit) & 1) << target);
    }
    if op == 0 && (source >> s) & 1 != 0 {
        for bit in highest + 1..bits {
            out |= 1u64 << bit;
        }
    }
    out & mask(bits)
}
#[test]
fn native_bitfields_match_independent_field_copy() {
    let mut cases = Cases::new();
    for bits in [32u32, 64] {
        let sf = u32::from(bits == 64);
        for op in 0..3 {
            for (r, s) in [
                (0, 0),
                (0, bits - 1),
                (bits - 1, bits - 1),
                (1, 0),
                (bits / 2, 7),
                (7, bits / 2),
                (bits - 1, 0),
                (3, 6),
            ] {
                let word = 0x13000000
                    | (sf << 31)
                    | (op << 29)
                    | (sf << 22)
                    | (r << 16)
                    | (s << 10)
                    | (1 << 5);
                let name = cases.function(word);
                for source in [0u64, u64::MAX, 0x8000000080000001, 0x0123456789abcdef] {
                    let old = 0xa5a5a5a55a5a5a5a;
                    let expected = field_reference(bits, source, old, r, s, op);
                    cases.check(&name,&format!("c.x[0]=UINT64_C({old});c.x[1]=UINT64_C({source});c.n=1;c.z=1;c.c=1;c.v=1;"),&format!("c.x[0]!=UINT64_C({expected})||!c.n||!c.z||!c.c||!c.v"));
                }
            }
        }
    }
    cases.run("bitfield-reference");
}
fn shift_reference(value: u64, bits: u32, kind: u32, amount: u32) -> u64 {
    let value = value & mask(bits);
    let mut out = 0;
    for bit in 0..bits {
        let src = match kind {
            0 => {
                if bit >= amount {
                    Some(bit - amount)
                } else {
                    None
                }
            }
            1 => {
                if bit + amount < bits {
                    Some(bit + amount)
                } else {
                    None
                }
            }
            2 => Some((bit + amount).min(bits - 1)),
            _ => Some((bit + amount) % bits),
        };
        if let Some(src) = src {
            out |= ((value >> src) & 1) << bit;
        }
    }
    out
}
#[test]
fn native_logical_registers_and_flags() {
    let mut cases = Cases::new();
    for bits in [32u32, 64] {
        let sf = u32::from(bits == 64);
        for op in 0..4u32 {
            for invert in 0..2 {
                for shift in 0..4 {
                    for amount in [0, 1, bits - 1] {
                        let word = 0x0a000000
                            | (sf << 31)
                            | (op << 29)
                            | (shift << 22)
                            | (invert << 21)
                            | (2 << 16)
                            | (amount << 10)
                            | (1 << 5);
                        let name = cases.function(word);
                        let lhs = 0xaaaaaaaa55555555u64;
                        let rhs = 0xf0f0f0f080000001u64;
                        let mut operand = shift_reference(rhs, bits, shift, amount);
                        if invert != 0 {
                            operand = (!operand) & mask(bits)
                        }
                        let result = match op {
                            0 | 3 => lhs & operand,
                            1 => lhs | operand,
                            _ => lhs ^ operand,
                        } & mask(bits);
                        let flags = if op == 3 {
                            format!(
                                "c.n!={}||c.z!={}||c.c||c.v",
                                result >> (bits - 1),
                                u8::from(result == 0)
                            )
                        } else {
                            "!c.n||!c.z||!c.c||!c.v".into()
                        };
                        cases.check(&name,&format!("c.x[1]=UINT64_C({lhs});c.x[2]=UINT64_C({rhs});c.n=1;c.z=1;c.c=1;c.v=1;"),&format!("c.x[0]!=UINT64_C({result})||({flags})"));
                    }
                }
            }
        }
    }
    cases.run("logical-registers");
}
#[test]
fn native_logical_immediates_repeat_circular_runs() {
    let mut cases = Cases::new();
    for bits in [32u32, 64] {
        let sf = u32::from(bits == 64);
        for element in [2u32, 4, 8, 16, 32, 64].into_iter().filter(|&e| e <= bits) {
            for length in [1, element / 2, element - 1] {
                for rotate in [0, 1, element - 1] {
                    let mut immediate = 0u64;
                    for bit in 0..bits {
                        if (bit + rotate) % element < length {
                            immediate |= 1u64 << bit;
                        }
                    }
                    let imms = (((!(element - 1)) << 1) | (length - 1)) & 63;
                    let n = u32::from(element == 64);
                    for op in 0..4u32 {
                        let word = 0x12000000
                            | (sf << 31)
                            | (op << 29)
                            | (n << 22)
                            | (rotate << 16)
                            | (imms << 10)
                            | (1 << 5);
                        let name = cases.function(word);
                        let src = 0x80000000deadbeefu64;
                        let result = match op {
                            0 | 3 => src & immediate,
                            1 => src | immediate,
                            _ => src ^ immediate,
                        } & mask(bits);
                        let flags = if op == 3 {
                            format!(
                                "c.n!={}||c.z!={}||c.c||c.v",
                                result >> (bits - 1),
                                u8::from(result == 0)
                            )
                        } else {
                            "!c.n||!c.z||!c.c||!c.v".into()
                        };
                        cases.check(
                            &name,
                            &format!("c.x[1]=UINT64_C({src});c.n=1;c.z=1;c.c=1;c.v=1;"),
                            &format!("c.x[0]!=UINT64_C({result})||({flags})"),
                        );
                    }
                }
            }
        }
    }
    let name = cases.function(0xb24003ff);
    cases.check(&name, "c.sp=UINT64_MAX;", "c.sp!=1");
    cases.run("logical-immediates");
}
#[test]
fn native_variable_shifts_extract_and_extended_arithmetic() {
    let mut cases = Cases::new();
    for bits in [32u32, 64] {
        let sf = u32::from(bits == 64);
        for kind in 0..4 {
            let name =
                cases.function(0x1ac02000 | (sf << 31) | (2 << 16) | (kind << 10) | (1 << 5));
            for amount in [0, 1, bits - 1, bits, bits + 1, 255] {
                let source = 0x8000000180000001u64;
                let result = shift_reference(source, bits, kind, amount & (bits - 1));
                cases.check(
                    &name,
                    &format!("c.x[1]=UINT64_C({source});c.x[2]={amount};"),
                    &format!("c.x[0]!=UINT64_C({result})"),
                );
            }
        }
        for amount in [0, 1, bits / 2, bits - 1] {
            let name = cases.function(
                0x13800000 | (sf << 31) | (sf << 22) | (2 << 16) | (amount << 10) | (1 << 5),
            );
            let high = 0x0123456789abcdefu64 & mask(bits);
            let low = 0xfedcba9876543210u64 & mask(bits);
            let result =
                ((((u128::from(high) << bits) | u128::from(low)) >> amount) as u64) & mask(bits);
            cases.check(
                &name,
                &format!("c.x[1]=UINT64_C({high});c.x[2]=UINT64_C({low});"),
                &format!("c.x[0]!=UINT64_C({result})"),
            );
        }
    }
    let name = cases.function(0x8b22d020);
    cases.check(
        &name,
        "c.x[1]=0x100;c.x[2]=UINT64_C(0xdeadbeeffffffffe);",
        "c.x[0]!=0xe0",
    );
    let name = cases.function(0x8b2243ff);
    cases.check(
        &name,
        "c.sp=10;c.x[2]=UINT64_C(0xdeadbeef00000003);",
        "c.sp!=13",
    );
    cases.run("shifts-extract-extend");
}

#[test]
fn native_multiply_matches_wide_reference() {
    let mut cases = Cases::new();
    let mut values = vec![
        0,
        1,
        2,
        0xffffffff,
        0x80000000,
        0x7fffffff,
        0x8000000000000000,
        0x7fffffffffffffff,
        u64::MAX,
        0x0123456789abcdef,
    ];
    let mut random = 0x514d32f685ced129u64;
    for _ in 0..12 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        values.push(random);
    }
    for (sf, group) in [(0, 0), (1, 0), (1, 1), (1, 5), (1, 2), (1, 6)] {
        for sub in 0..2u32 {
            let high = group == 2 || group == 6;
            if high && sub == 1 {
                continue;
            }
            let word = 0x1b000000
                | (sf << 31)
                | (group << 21)
                | (sub << 15)
                | (2 << 16)
                | ((if high { 31 } else { 3 }) << 10)
                | (1 << 5);
            let name = cases.function(word);
            let mut rows = Vec::new();
            for (i, &a) in values.iter().enumerate() {
                for (j, &b) in values.iter().enumerate() {
                    let acc = values[(i + j + 5) % values.len()];
                    let product = match group {
                        0 => a.wrapping_mul(b),
                        1 => ((a as i32 as i64) * (b as i32 as i64)) as u64,
                        5 => u64::from(a as u32) * u64::from(b as u32),
                        2 => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
                        6 => ((u128::from(a) * u128::from(b)) >> 64) as u64,
                        _ => unreachable!(),
                    };
                    let expected = if high {
                        product
                    } else if sub == 1 {
                        acc.wrapping_sub(product)
                    } else {
                        acc.wrapping_add(product)
                    };
                    let expected = if sf == 0 {
                        expected & 0xffffffff
                    } else {
                        expected
                    };
                    rows.push(format!(
                        "{{UINT64_C({a}),UINT64_C({b}),UINT64_C({acc}),UINT64_C({expected})}}"
                    ));
                }
            }
            cases.main+=&format!("{{const uint64_t rows[][4]={{{}}};for(unsigned i=0;i<sizeof(rows)/sizeof(rows[0]);i++){{c=(MkwCpu){{0}};c.x[1]=rows[i][0];c.x[2]=rows[i][1];c.x[3]=rows[i][2];c.n=1;c.z=0;c.c=1;c.v=1;if({name}(&c,10)||c.x[0]!=rows[i][3]||c.x[1]!=rows[i][0]||c.x[2]!=rows[i][1]||c.x[3]!=rows[i][2]||!c.n||c.z||!c.c||!c.v)return 2;}}}}\n",rows.join(","));
        }
    }
    // MUL X0,XZR,X2 and MADD WZR,W1,W2,W3 preserve unrelated state.
    let zero = cases.function(0x9b027fe0);
    cases.check(&zero, "c.x[2]=123;c.sp=99;", "c.x[0]||c.sp!=99");
    let discard = cases.function(0x1b020c3f);
    cases.check(
        &discard,
        "c.x[0]=123;c.x[1]=456;c.x[2]=789;c.x[3]=10;",
        "c.x[0]!=123",
    );
    cases.run("multiply");
}
#[test]
fn native_divide_zero_overflow_signs_and_widths() {
    let mut cases = Cases::new();
    let values = [
        0u64,
        1,
        2,
        31,
        0xffffffff,
        0x80000000,
        0x7fffffff,
        0x8000000000000000,
        0x7fffffffffffffff,
        u64::MAX,
    ];
    for sf in 0..2u32 {
        for signed in 0..2u32 {
            let name =
                cases.function(0x1ac00800 | (sf << 31) | (signed << 10) | (2 << 16) | (1 << 5));
            let mut rows = Vec::new();
            for &a in &values {
                for &b in &values {
                    let (aa, bb) = if sf == 0 {
                        (a & 0xffffffff, b & 0xffffffff)
                    } else {
                        (a, b)
                    };
                    let result = if bb == 0 {
                        0
                    } else if signed == 0 {
                        aa / bb
                    } else {
                        let (a, b) = if sf == 0 {
                            (aa as i32 as i128, bb as i32 as i128)
                        } else {
                            (aa as i64 as i128, bb as i64 as i128)
                        };
                        (a / b) as u64
                    };
                    let expected = if sf == 0 { result & 0xffffffff } else { result };
                    rows.push(format!(
                        "{{UINT64_C({a}),UINT64_C({b}),UINT64_C({expected})}}"
                    ));
                }
            }
            cases.main+=&format!("{{const uint64_t rows[][3]={{{}}};for(unsigned i=0;i<sizeof(rows)/sizeof(rows[0]);i++){{c=(MkwCpu){{0}};c.x[1]=rows[i][0];c.x[2]=rows[i][1];c.n=1;c.z=1;c.c=0;c.v=1;if({name}(&c,10)||c.x[0]!=rows[i][2]||c.x[1]!=rows[i][0]||c.x[2]!=rows[i][1]||!c.n||!c.z||c.c||!c.v)return 1;}}}}\n",rows.join(","));
        }
    }
    let zero = cases.function(0x9adf0820);
    cases.check(&zero, "c.x[1]=123;", "c.x[0]!=0");
    cases.run("divide");
}

#[test]
fn native_raw_simd_known_patterns_and_aliases() {
    let mut cases = Cases::new();
    for (word, init, check) in [
        (
            0x1e204020u32,
            "c.vregs[1][0]=UINT64_C(0xfeedbeef80000001);c.vregs[0][1]=UINT64_MAX;",
            "c.vregs[0][0]!=UINT64_C(0x80000001)||c.vregs[0][1]",
        ),
        (
            0x1e604020,
            "c.vregs[1][0]=UINT64_C(0x7ff0000000000001);c.vregs[1][1]=123;",
            "c.vregs[0][0]!=UINT64_C(0x7ff0000000000001)||c.vregs[0][1]",
        ),
        (
            0x1ee04020,
            "c.vregs[1][0]=UINT64_C(0xdead8001);",
            "c.vregs[0][0]!=UINT64_C(0x8001)||c.vregs[0][1]",
        ),
        (
            0x9eaf0020,
            "c.vregs[0][0]=123;c.x[1]=456;",
            "c.vregs[0][0]!=123||c.vregs[0][1]!=456",
        ),
        (
            0x9eae0020,
            "c.vregs[1][0]=123;c.vregs[1][1]=456;",
            "c.x[0]!=456",
        ),
        (
            0x1e2703e0,
            "c.sp=123;c.vregs[0][0]=456;c.vregs[0][1]=789;",
            "c.vregs[0][0]||c.vregs[0][1]||c.sp!=123",
        ),
        (
            0x1e2603e0,
            "c.vregs[31][0]=UINT64_C(0x1234567887654321);",
            "c.x[0]!=UINT64_C(0x87654321)",
        ),
        (
            0x4e010c20,
            "c.x[1]=UINT64_C(0x123456789abcdef1);",
            "c.vregs[0][0]!=UINT64_C(0xf1f1f1f1f1f1f1f1)||c.vregs[0][1]!=c.vregs[0][0]",
        ),
        (
            0x0e1f0420,
            "c.vregs[1][1]=UINT64_C(0x123456789abcdef0);c.vregs[0][1]=99;",
            "c.vregs[0][0]!=UINT64_C(0x1212121212121212)||c.vregs[0][1]",
        ),
        (
            0x4e1f1c20,
            "c.vregs[0][0]=1;c.vregs[0][1]=2;c.x[1]=0xaa;",
            "c.vregs[0][0]!=1||c.vregs[0][1]!=UINT64_C(0xaa00000000000002)",
        ),
        (
            0x0e012c20,
            "c.vregs[1][0]=0x80;",
            "c.x[0]!=UINT64_C(0xffffff80)",
        ),
        (
            0x4e012c20,
            "c.vregs[1][0]=0x80;",
            "c.x[0]!=UINT64_C(0xffffffffffffff80)",
        ),
        (0x0e013c20, "c.vregs[1][0]=0x80;", "c.x[0]!=0x80"),
        (
            0x0ea11c20,
            "c.vregs[1][0]=123;c.vregs[1][1]=456;c.vregs[0][1]=789;",
            "c.vregs[0][0]!=123||c.vregs[0][1]",
        ),
        (
            0x4e201c00,
            "c.vregs[0][0]=123;c.vregs[0][1]=456;",
            "c.vregs[0][0]!=123||c.vregs[0][1]!=456",
        ),
        (
            0x6e201c00,
            "c.vregs[0][0]=123;c.vregs[0][1]=456;",
            "c.vregs[0][0]||c.vregs[0][1]",
        ),
        (
            0x4f00e400,
            "c.vregs[0][0]=123;c.vregs[0][1]=456;",
            "c.vregs[0][0]||c.vregs[0][1]",
        ),
        (
            0x1e2e1000,
            "c.vregs[0][0]=123;c.vregs[0][1]=456;",
            "c.vregs[0][0]!=UINT64_C(0x3f800000)||c.vregs[0][1]",
        ),
    ] {
        let name = cases.function(word);
        cases.check(
            &name,
            &format!("{init}c.n=1;c.z=1;c.c=0;c.v=1;"),
            &format!("({check})||!c.n||!c.z||c.c||!c.v"),
        );
    }
    cases.run("raw-simd-known");
}
