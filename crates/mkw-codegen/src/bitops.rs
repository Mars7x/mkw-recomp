use super::{arithmetic, r, ty, write};
use mkw_arm64::{bitops::Operand, Op, Reg, Width};
fn rotate(width: Width, expression: &str, amount: u8) -> String {
    let value = format!("({})({expression})", ty(width));
    let bits = if width == Width::W32 { 32 } else { 64 };
    if amount == 0 {
        value
    } else {
        format!("(({value}>>{amount})|({value}<<{}))", bits - amount)
    }
}
pub fn extended(src: Reg, extend: u8, shift: u8) -> String {
    let bits = 8u32 << (extend & 3);
    let expr = if bits == 64 {
        r(src)
    } else {
        format!("({}&UINT64_C({}))", r(src), (1u64 << bits) - 1)
    };
    let expr = if extend & 4 != 0 && bits < 64 {
        format!(
            "(({expr}^UINT64_C({}))-UINT64_C({}))",
            1u64 << (bits - 1),
            1u64 << (bits - 1)
        )
    } else {
        expr
    };
    format!("({expr}<<{shift})")
}
pub fn emit(op: Op) -> Result<String, String> {
    Ok(match op {
        Op::Logical {
            width,
            dst,
            src,
            rhs,
            operation,
        } => {
            let right = match rhs {
                Operand::Immediate(mask) => format!("UINT64_C({mask})"),
                Operand::Shifted {
                    value,
                    shift,
                    amount,
                    invert,
                } => {
                    let expr = arithmetic::shifted(width, value, shift, amount);
                    if invert {
                        format!("~({expr})")
                    } else {
                        expr
                    }
                }
            };
            let sign = match operation {
                0 | 3 => "&",
                1 => "|",
                2 => "^",
                _ => return Err("invalid logical operation".into()),
            };
            let t = ty(width);
            let mut out = format!(
                "{{{t} a=({t})({}),b=({t})({right});{t} result=a{sign}b;\n",
                r(src)
            );
            if operation == 3 {
                let bits = if width == Width::W32 { 32 } else { 64 };
                out += &format!(
                    "c->n=(uint32_t)(result>>{});c->z=(result==0);c->c=0;c->v=0;\n",
                    bits - 1
                );
            }
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        Op::Bitfield {
            width,
            dst,
            src,
            operation,
            rotate: amount,
            sign_bit,
            wmask,
            tmask,
        } => {
            let rotated = rotate(width, &r(src), amount);
            let old = format!("({})({})", ty(width), r(dst));
            let mut out=format!("{{uint64_t source=({})({});uint64_t bot=(uint64_t)({rotated})&UINT64_C({wmask});\n",ty(width),r(src));
            match operation{0=>out+=&format!("uint64_t top=((source>>{sign_bit})&1)?UINT64_MAX:0;uint64_t result=(top&~UINT64_C({tmask}))|(bot&UINT64_C({tmask}));\n"),1=>out+=&format!("(void)source;bot=({old}&~UINT64_C({wmask}))|bot;uint64_t result=({old}&~UINT64_C({tmask}))|(bot&UINT64_C({tmask}));\n"),2=>out+=&format!("(void)source;uint64_t result=bot&UINT64_C({tmask});\n"),_=>return Err("invalid bitfield operation".into())}
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        Op::Extract {
            width,
            dst,
            high,
            low,
            amount,
        } => {
            let bits = if width == Width::W32 { 32 } else { 64 };
            let t = ty(width);
            let expr = if amount == 0 {
                r(low)
            } else {
                format!(
                    "(({t})({})>>{amount})|(({t})({})<<{})",
                    r(low),
                    r(high),
                    bits - amount
                )
            };
            write(dst, width, &expr)
        }
        Op::VariableShift {
            width,
            dst,
            src,
            amount,
            kind,
        } => {
            let bits = if width == Width::W32 { 32 } else { 64 };
            let t = ty(width);
            let mut out = format!(
                "{{{t} a=({t})({});unsigned count=(unsigned)({}&{});{t} result;\n",
                r(src),
                r(amount),
                bits - 1
            );
            let expr = match kind {
                0 => "a<<count".to_string(),
                1 => "a>>count".to_string(),
                2 => format!(
                    "(a>>count)|((a>>{})?(UINT{bits}_MAX<<({bits}-count)):0)",
                    bits - 1
                ),
                3 => format!("(a>>count)|(a<<({bits}-count))"),
                _ => return Err("invalid variable shift".into()),
            };
            out += &format!("if(!count)result=a;else result={expr};\n");
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        Op::AddSubExtended {
            width,
            dst,
            lhs,
            rhs,
            extend,
            shift,
            sub,
            flags,
        } => arithmetic::add_sub(
            width,
            dst,
            &r(lhs),
            &extended(rhs, extend, shift),
            sub,
            flags,
        ),
        _ => return Err("not a logical/bitfield operation".into()),
    })
}
