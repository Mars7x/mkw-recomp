//! Integer multiply/divide formats. Reserved accumulator/width fields reject.
use super::{reg, Op, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MultiplyKind {
    Low,
    UnsignedLong,
    SignedLong,
    UnsignedHigh,
    SignedHigh,
}
pub fn decode(word: u32, width: Width) -> Option<Result<Op, String>> {
    let rd = reg(word & 31, false);
    let lhs = reg((word >> 5) & 31, false);
    let rhs = reg((word >> 16) & 31, false);
    if matches!(word & 0x7fe0fc00, 0x1ac00800 | 0x1ac00c00) {
        return Some(Ok(Op::Divide {
            width,
            dst: rd,
            lhs,
            rhs,
            signed: word & (1 << 10) != 0,
        }));
    }
    let kind = match word & 0x7fe00000 {
        0x1b000000 => MultiplyKind::Low,
        0x1b200000 => MultiplyKind::SignedLong,
        0x1ba00000 => MultiplyKind::UnsignedLong,
        0x1b400000 => MultiplyKind::SignedHigh,
        0x1bc00000 => MultiplyKind::UnsignedHigh,
        _ => return None,
    };
    let high = matches!(kind, MultiplyKind::UnsignedHigh | MultiplyKind::SignedHigh);
    if (kind != MultiplyKind::Low && width != Width::X64) || (high && word & 0xfc00 != 0x7c00) {
        return Some(Err("reserved multiply width/accumulator".into()));
    }
    Some(Ok(Op::Multiply {
        width,
        dst: rd,
        lhs,
        rhs,
        acc: reg((word >> 10) & 31, false),
        sub: word & (1 << 15) != 0,
        kind,
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn reserved_multiply_fields_reject() {
        for word in [
            0x1b200000, 0x1ba00000, 0x9b400000, 0x9b40fc00, 0x9bc00000, 0x9b600000,
        ] {
            assert!(crate::decode(word, 0).is_err(), "{word:08x}");
        }
    }
}
