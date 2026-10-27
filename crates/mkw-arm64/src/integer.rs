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
    if word & 0x7fe0e000 == 0x1ac04000 {
        let size = (word >> 10) & 3;
        if (width == Width::X64) != (size == 3) {
            return Some(Err("reserved CRC input width".into()));
        }
        return Some(Ok(Op::Crc32 {
            dst: rd,
            lhs,
            rhs,
            bytes: 1 << size,
            castagnoli: word & (1 << 12) != 0,
        }));
    }
    if word & 0x1fe0fc00 == 0x1a000000 {
        return Some(Ok(Op::CarryArithmetic {
            width,
            dst: rd,
            lhs,
            rhs,
            sub: word & (1 << 30) != 0,
            flags: word & (1 << 29) != 0,
        }));
    }
    if word & 0x7ffffc00 >= 0x5ac00000 && word & 0x7ffffc00 <= 0x5ac01400 {
        let operation = ((word >> 10) & 7) as u8;
        if operation == 3 && width == Width::W32 {
            return Some(Err("reserved word REV size".into()));
        }
        return Some(Ok(Op::IntegerUnary {
            width,
            dst: rd,
            src: lhs,
            operation,
        }));
    }
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

#[cfg(test)]
mod bit_carry_tests {
    use super::*;
    #[test]
    fn reserved_bit_operations_and_carry_fields_reject() {
        for w in [0x5ac00c00, 0x5ac01800, 0x1a000400, 0x1a200000] {
            assert!(crate::decode(w, 0).is_err(), "{w:08x}");
        }
        assert!(matches!(
            crate::decode(0xda0203e0, 0).unwrap().op,
            Op::CarryArithmetic {
                width: Width::X64,
                lhs: super::super::Reg::Zero,
                sub: true,
                ..
            }
        ));
    }
}

#[cfg(test)]
mod crc_tests {
    use super::*;
    #[test]
    fn crc_widths_zero_registers_and_reserved_sf() {
        for size in 0..4 {
            for castagnoli in [false, true] {
                let word = 0x1adf43ff
                    | (size << 10)
                    | (u32::from(castagnoli) << 12)
                    | (if size == 3 { 1 << 31 } else { 0 });
                assert!(
                    matches!(crate::decode(word,0).unwrap().op,Op::Crc32 {dst:super::super::Reg::Zero,lhs:super::super::Reg::Zero,rhs:super::super::Reg::Zero,bytes,castagnoli:c} if bytes==1<<size && c==castagnoli)
                );
                assert!(crate::decode(word ^ (1 << 31), 0).is_err());
            }
        }
        for word in [0x1ac06020, 0x1ac0e020, 0x1ae04020] {
            assert!(crate::decode(word, 0).is_err());
        }
    }
}
