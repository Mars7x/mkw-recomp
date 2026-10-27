use super::{r, ty, write};
use mkw_arm64::{Reg, Width};
pub fn add_sub(width: Width, dst: Reg, lhs: &str, rhs: &str, sub: bool, flags: bool) -> String {
    let sign = if sub { "-" } else { "+" };
    let bits = if width == Width::W32 { 32 } else { 64 };
    let t = ty(width);
    let mut out = format!("{{ {t} a=({t})({lhs}), b=({t})({rhs}); {t} v=({t})(a {sign} b);\n");
    if flags {
        let c = if sub { "a>=b" } else { "v<a" };
        let v = if sub {
            "((a^b)&(a^v))"
        } else {
            "(~(a^b)&(a^v))"
        };
        out += &format!(
            "c->n=(uint32_t)(v>>{});c->z=(v==0);c->c=({c});c->v=(uint32_t)({v}>>{});\n",
            bits - 1,
            bits - 1
        );
    }
    out += &write(dst, width, "v");
    out += "}\n";
    out
}
pub fn shifted(width: Width, src: Reg, shift: u8, amount: u8) -> String {
    let val = format!("({})({})", ty(width), r(src));
    if amount == 0 {
        return val;
    }
    match shift {
        0 => format!("({val}<<{amount})"),
        1 => format!("({val}>>{amount})"),
        2 => {
            let bits = if width == Width::W32 { 32 } else { 64 };
            format!(
                "(({val}>>{amount})|(({val}>>{})?(UINT{bits}_MAX<<{}):0))",
                bits - 1,
                bits - amount
            )
        }
        3 => {
            let bits = if width == Width::W32 { 32 } else { 64 };
            format!("(({val}>>{amount})|({val}<<{}))", bits - amount)
        }
        _ => unreachable!("validated shift"),
    }
}
