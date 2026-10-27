//! Ahead-of-time C emitter. Generated native code uses fixed labels, not opcode dispatch.
mod arithmetic;
mod atomics;
mod bitops;
mod condition;
mod floating;
mod integer;
mod memory;
mod simd;
use mkw_arm64::{Instruction, MemoryOrder, Op, Reg, Width};
fn r(reg: Reg) -> String {
    match reg {
        Reg::Gpr(n) => format!("c->x[{n}]"),
        Reg::Sp => "c->sp".into(),
        Reg::Zero => "UINT64_C(0)".into(),
    }
}
fn ty(w: Width) -> &'static str {
    match w {
        Width::W32 => "uint32_t",
        Width::X64 => "uint64_t",
    }
}
fn write(reg: Reg, w: Width, expr: &str) -> String {
    match reg {
        Reg::Zero => format!("(void)({expr});"),
        _ => format!("{} = ({})({expr});", r(reg), ty(w)),
    }
}
/// All semantic and ABI inputs used by the incremental generation identity.
pub const SOURCE_INPUTS: &[&str] = &[
    include_str!("lib.rs"),
    include_str!("arithmetic.rs"),
    include_str!("condition.rs"),
    include_str!("memory.rs"),
    include_str!("bitops.rs"),
    include_str!("integer.rs"),
    include_str!("simd.rs"),
    include_str!("atomics.rs"),
    include_str!("floating.rs"),
    include_str!("../../../native/cpu.h"),
    include_str!("../../../native/fp.c"),
    include_str!("../../../native/convert.c"),
    include_str!("../../../native/fp_environment.h"),
    include_str!("../../../native/softfloat/platform.h"),
    include_str!("../../../third_party/softfloat/PROVENANCE.json"),
];
pub const HEADER: &str = include_str!("../../../native/cpu.h");
pub fn emit(ins: &[Instruction], entry: u64) -> Result<String, String> {
    if ins.is_empty() {
        return Err("empty translation".into());
    }
    let pcs: std::collections::BTreeSet<_> = ins.iter().map(|i| i.pc).collect();
    if pcs.len() != ins.len() || !pcs.contains(&entry) {
        return Err("duplicate PC or missing entry".into());
    }
    // Validate the graph even when callers construct IR themselves.
    for i in ins {
        let mut edges = Vec::new();
        match i.op {
            Op::Ret { .. }
            | Op::AuthReturn { .. }
            | Op::IndirectBranch { .. }
            | Op::TailBranch { .. } => {}
            Op::Branch { target } => edges.push(target),
            Op::CompareBranch { target, .. }
            | Op::BranchCond { target, .. }
            | Op::TestBit { target, .. } => {
                edges.push(target);
                edges.push(i.pc + 4)
            }
            _ => edges.push(i.pc + 4),
        }
        for e in edges {
            if !pcs.contains(&e) {
                return Err(format!("unresolved edge {e:#x}"));
            }
        }
    }
    let mut out = format!(
        "{HEADER}\nint mkw_guest_budget(MkwCpu *c, uint64_t *budget) {{\n goto L_{entry:x};\n"
    );
    for i in ins {
        out+=&format!("L_{:x}: ; /* guest {:016x}, instruction {:08x} */\n c->pc=c->module_base+UINT64_C({}); if (!*budget) return 2; --*budget;\n",i.pc,i.pc,i.word,i.pc);
        let next = format!("goto L_{:x};\n", i.pc + 4);
        match i.op {
            Op::Nop => out += &next,
            Op::FloatSelect {
                bytes,
                dst,
                lhs,
                rhs,
                condition,
            } => {
                let mask = if bytes == 8 {
                    u64::MAX
                } else {
                    (1u64 << (bytes * 8)) - 1
                };
                out+=&format!("{{uint64_t value=({})?c->vregs[{lhs}][0]:c->vregs[{rhs}][0];c->vregs[{dst}][0]=value&UINT64_C({mask});c->vregs[{dst}][1]=0;}}\n",condition::expression(condition)?);
                out += &next;
            }
            op @ (Op::Floating(_)
            | Op::Convert(_)
            | Op::SystemRegister { .. }
            | Op::Barrier { .. }) => {
                out += &floating::emit(op);
                out += &next;
            }
            Op::Atomic(op) => {
                out += &atomics::emit(op);
                out += &next;
            }
            Op::Simd(op) => {
                out += &simd::emit(op);
                out += &next;
            }
            operation @ (Op::Multiply { .. }
            | Op::Divide { .. }
            | Op::IntegerUnary { .. }
            | Op::CarryArithmetic { .. }) => {
                out += &integer::emit(operation)?;
                out += &next;
            }
            operation @ (Op::Logical { .. }
            | Op::Bitfield { .. }
            | Op::Extract { .. }
            | Op::VariableShift { .. }
            | Op::AddSubExtended { .. }) => {
                out += &bitops::emit(operation)?;
                out += &next;
            }
            Op::Select {
                width,
                dst,
                lhs,
                rhs,
                condition,
                operation,
            } => {
                let right = format!("({})({})", ty(width), r(rhs));
                let right = match operation {
                    0 => right,
                    1 => format!("({right}+1u)"),
                    2 => format!("~({right})"),
                    3 => format!("-({right})"),
                    _ => return Err("invalid select operation".into()),
                };
                out += &write(
                    dst,
                    width,
                    &format!(
                        "({})?({}):({right})",
                        condition::expression(condition)?,
                        r(lhs)
                    ),
                );
                out += &next;
            }
            Op::TestBit {
                src,
                bit,
                target,
                nonzero,
            } => {
                out += &format!(
                    "if((({}>>{bit})&1) {} 0)goto L_{target:x};\n",
                    r(src),
                    if nonzero { "!=" } else { "==" }
                );
                out += &next;
            }
            Op::ConditionalCompare {
                width,
                lhs,
                rhs,
                immediate,
                condition,
                nzcv,
                sub,
            } => {
                let right = immediate
                    .map(|n| format!("UINT64_C({n})"))
                    .unwrap_or_else(|| r(rhs));
                out += &format!("if({}){{\n", condition::expression(condition)?);
                out += &arithmetic::add_sub(width, Reg::Zero, &r(lhs), &right, sub, true);
                out += &format!(
                    "}}else{{c->n={};c->z={};c->c={};c->v={};}}\n",
                    nzcv >> 3,
                    (nzcv >> 2) & 1,
                    (nzcv >> 1) & 1,
                    nzcv & 1
                );
                out += &next;
            }
            Op::TailBranch { target } => {
                out += &format!("c->pc=c->module_base+UINT64_C({target});c->fault_address=c->pc;return MKW_EXIT_TAIL;\n");
            }
            Op::IndirectBranch { target } => {
                out += &format!("{{uint64_t target={};c->fault_address=target;\n", r(target));
                out += "switch(target-c->module_base){\n";
                for pc in &pcs {
                    out += &format!("case UINT64_C({pc}): goto L_{pc:x};\n");
                }
                out += "default: c->pc=target;return MKW_EXIT_TAIL;}}\n";
            }
            Op::IndirectCall { target } => {
                out+=&format!("{{uint64_t target={};c->fault_address=target;if(!c->call){{c->fault_status=3;return 3;}}c->x[30]=c->module_base+UINT64_C({});uint64_t caller_base=c->module_base;int status=c->call(c->call_context,c,target,budget);if(status)return status;if(c->module_base!=caller_base||c->pc!=caller_base+UINT64_C({})){{c->fault_status=6;return 6;}}}}\n",r(target),i.pc+4,i.pc+4);
                out += &next;
            }
            Op::AddSubReg {
                width,
                dst,
                lhs,
                rhs,
                shift,
                amount,
                sub,
                flags,
            } => {
                out += &arithmetic::add_sub(
                    width,
                    dst,
                    &r(lhs),
                    &arithmetic::shifted(width, rhs, shift, amount),
                    sub,
                    flags,
                );
                out += &next;
            }
            Op::OrderedMemory {
                order,
                bytes,
                value,
                base,
            } => {
                let store = order == MemoryOrder::Release;
                out+=&format!("{{uint64_t address={};uint64_t values[2]={{{},0}};c->fault_address=address;if(!c->ordered_memory){{c->fault_status=3;return 3;}}int status=c->ordered_memory(c->memory_context,{}, {},address,values,{bytes});if(status){{c->fault_status=status;return status;}}\n",r(base),r(value),order as u32,u8::from(store));
                if !store {
                    let expression = if bytes == 8 {
                        "values[0]".into()
                    } else {
                        format!("values[0]&UINT64_C({})", (1u64 << (bytes * 8)) - 1)
                    };
                    out += &write(
                        value,
                        if bytes == 8 { Width::X64 } else { Width::W32 },
                        &expression,
                    );
                }
                out += "}\n";
                out += &next;
            }
            Op::Address { dst, address, page } => {
                if page {
                    out += "if(c->module_base&UINT64_C(4095)){c->fault_status=3;return 3;}\n";
                }
                out += &write(
                    dst,
                    Width::X64,
                    &format!("c->module_base+UINT64_C({address})"),
                );
                out += &next;
            }
            Op::Call { target } => {
                out+=&format!("if(!c->call){{c->fault_status=3;c->fault_address=c->module_base+UINT64_C({target});return 3;}} c->x[30]=c->module_base+UINT64_C({}); {{uint64_t caller_base=c->module_base;int status=c->call(c->call_context,c,c->module_base+UINT64_C({target}),budget);if(status)return status;if(c->module_base!=caller_base||c->pc!=caller_base+UINT64_C({})){{c->fault_status=6;return 6;}}}}\n",i.pc+4,i.pc+4);
                out += &next;
            }
            Op::Memory(transfer) => {
                out += &memory::emit(transfer);
                out += &next;
            }
            Op::MoveReg { width, dst, src } => {
                out += &write(dst, width, &r(src));
                out += &next;
            }
            Op::PointerAuth { key, operation } => {
                out+=&format!("{{ uint64_t result=0; if(!c->pauth) {{c->fault_status=3;return 3;}} int status=c->pauth(c->pac_context,{operation},{key},c->x[30],c->sp,&result); if(status) {{c->fault_status=status;return status;}} c->x[30]=result; }}\n");
                out += &next;
            }
            Op::AuthReturn { key } => {
                out+=&format!("{{ uint64_t result=0; if(!c->pauth) {{c->fault_status=3;return 3;}} int status=c->pauth(c->pac_context,3,{key},c->x[30],c->sp,&result); if(status) {{c->fault_status=status;return status;}} c->pc=result;return 0;}}\n");
            }
            Op::MoveWide {
                width,
                dst,
                imm,
                keep,
                invert,
                shift,
            } => {
                let val = imm << shift;
                let expr = if keep {
                    format!(
                        "({} & ~UINT64_C({})) | UINT64_C({val})",
                        r(dst),
                        65535u64 << shift
                    )
                } else if invert {
                    format!("~UINT64_C({val})")
                } else {
                    format!("UINT64_C({val})")
                };
                out += &write(dst, width, &expr);
                out += &next;
            }
            Op::AddSubImm {
                width,
                dst,
                src,
                imm,
                sub,
                flags,
            } => {
                out += &arithmetic::add_sub(
                    width,
                    dst,
                    &r(src),
                    &format!("UINT64_C({imm})"),
                    sub,
                    flags,
                );
                out += &next;
            }
            Op::Branch { target } => out += &format!("goto L_{target:x};\n"),
            Op::CompareBranch {
                width,
                src,
                target,
                nonzero,
            } => {
                out += &format!(
                    "if (({})({}) {} 0) goto L_{target:x};\n",
                    ty(width),
                    r(src),
                    if nonzero { "!=" } else { "==" }
                );
                out += &next;
            }
            Op::BranchCond { target, cond } => {
                let condition = condition::expression(cond)?;
                out += &format!("if ({condition}) goto L_{target:x};\n");
                out += &next;
            }
            Op::Ret { target } => out += &format!("c->pc={}; return 0;\n", r(target)),
        }
    }
    out += "}\n";
    out += "int mkw_guest(MkwCpu *c,uint64_t budget){int status=mkw_guest_budget(c,&budget);if(status==MKW_EXIT_TAIL){if(!c->call){c->fault_status=3;return 3;}return c->call(c->call_context,c,c->pc,&budget);}return status; }\n";
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use mkw_arm64::lift;
    #[test]
    fn execute_native_loop_and_flags() {
        // Independently authored: x0=40; x1=2; loop x0++; x1--; cbnz; ret.
        let words = [
            0xd2800500u32,
            0xd2800041,
            0x91000400,
            0xf1000421,
            0xb5ffffc1,
            0xd65f03c0,
        ];
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let code = emit(&lift(&bytes, 0x1000, 0x1000).unwrap(), 0x1000).unwrap();
        let dir = std::env::temp_dir().join(format!("mkw-codegen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.c");
        let exe = dir.join("test");
        let harness = r#"
int main(void) { MkwCpu c={0}; c.x[30]=0xdead;
 if (mkw_guest(&c,100)!=0 || c.x[0]!=42 || c.x[1]!=0 || c.pc!=0xdead || !c.z || !c.c || c.n || c.v) return 1;
 c=(MkwCpu){0}; if (mkw_guest(&c,3)!=2 || c.pc!=0x100c) return 2; return 0; }
"#;
        std::fs::write(&src, code + harness).unwrap();
        let compiled = std::process::Command::new("cc")
            .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-g"])
            .arg(&src)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        assert!(std::process::Command::new(&exe).status().unwrap().success());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;
    #[test]
    fn native_arithmetic_boundaries_and_move_wide() {
        let cases = [
            (
                0x10000020u32,
                "c.module_base=UINT64_C(0x7000000000);",
                "c.x[0]!=UINT64_C(0x7000000004)",
            ),
            (
                0xb0000000,
                "c.module_base=UINT64_C(0x7000000000);",
                "c.x[0]!=UINT64_C(0x7000001000)",
            ),
            (
                0x31000400u32,
                "c.x[0]=UINT64_C(0xfeedbeefffffffff);",
                "c.x[0]!=0 || c.n || !c.z || !c.c || c.v",
            ),
            (
                0x31000400,
                "c.x[0]=0x7fffffff;",
                "c.x[0]!=0x80000000 || !c.n || c.z || c.c || !c.v",
            ),
            (
                0xb1000400,
                "c.x[0]=UINT64_MAX;",
                "c.x[0]!=0 || c.n || !c.z || !c.c || c.v",
            ),
            (
                0xb1000400,
                "c.x[0]=UINT64_C(0x7fffffffffffffff);",
                "c.x[0]!=UINT64_C(0x8000000000000000) || !c.n || c.z || c.c || !c.v",
            ),
            (
                0xf1000400,
                "c.x[0]=0;",
                "c.x[0]!=UINT64_MAX || !c.n || c.z || c.c || c.v",
            ),
            (
                0x71000400,
                "c.x[0]=0x80000000;",
                "c.x[0]!=0x7fffffff || c.n || c.z || !c.c || !c.v",
            ),
            (0x910007ff, "c.sp=15;", "c.sp!=16"), // ADD SP,SP,#1
            (
                0xf2b7dde0,
                "c.x[0]=UINT64_C(0x0123456789abcdef);",
                "c.x[0]!=UINT64_C(0x01234567beefcdef)",
            ), // MOVK X0,#BEEF,LSL16
            (0x12800000, "c.x[0]=UINT64_MAX;", "c.x[0]!=0xffffffff"), // MOVN W0,#0
            (0x5280001f, "c.x[0]=123;", "c.x[0]!=123"), // MOVZ WZR,#0
        ];
        let mut source = String::new();
        let mut harness = String::from("int main(void) {MkwCpu c;\n");
        for (i, (word, init, assertion)) in cases.iter().enumerate() {
            let bytes: Vec<_> = [*word, 0xd65f03c0]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect();
            let code = emit(&mkw_arm64::lift(&bytes, 0, 0).unwrap(), 0)
                .unwrap()
                .replace(HEADER, "")
                .replace("mkw_guest", &format!("f{i}"));
            source += &code;
            harness += &format!(
                "c=(MkwCpu){{0}}; {init} if (f{i}(&c,8)!=0 || ({assertion})) return {};\n",
                i + 1
            );
        }
        harness += "return 0;}\n";
        let dir = std::env::temp_dir().join(format!("mkw-semantic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("edge.c");
        let exe = dir.join("edge");
        std::fs::write(&src, format!("{HEADER}\n{source}\n{harness}")).unwrap();
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
                .arg(&src)
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
                "native semantic test failed: {:?}: {}",
                run.status.code(),
                String::from_utf8_lossy(&run.stderr)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
