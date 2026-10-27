//! Integer logical/bitfield encodings; masks are resolved ahead of execution.
use super::{reg, Op, Reg, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Operand {
    Immediate(u64),
    Shifted {
        value: Reg,
        shift: u8,
        amount: u8,
        invert: bool,
    },
}
pub fn ones(bits: u32) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}
pub fn masks(
    width: Width,
    n: bool,
    rotate: u8,
    size: u8,
    immediate: bool,
) -> Result<(u64, u64), String> {
    let bits = if width == Width::X64 { 64 } else { 32 };
    let combined = (u32::from(n) << 6) | u32::from(!size & 63);
    if combined == 0 {
        return Err("reserved bitmask length".into());
    }
    let len = 31 - combined.leading_zeros();
    let element = 1u32 << len;
    if len < 1 || element > bits {
        return Err("reserved bitmask element size".into());
    }
    let levels = element - 1;
    let s = u32::from(size) & levels;
    let r = u32::from(rotate) & levels;
    if immediate && s == levels {
        return Err("reserved all-ones logical immediate".into());
    }
    let base = ones(s + 1);
    let rotated = if r == 0 {
        base
    } else {
        ((base >> r) | (base << (element - r))) & ones(element)
    };
    let top = ones((s.wrapping_sub(r) & levels) + 1);
    let (mut wmask, mut tmask) = (0, 0);
    for offset in (0..bits).step_by(element as usize) {
        wmask |= rotated << offset;
        tmask |= top << offset;
    }
    Ok((wmask, tmask))
}
pub fn decode(w: u32, width: Width) -> Option<Result<Op, String>> {
    let rd = w & 31;
    let rn = (w >> 5) & 31;
    let bits = if width == Width::X64 { 64 } else { 32 };
    if w & 0x1f000000 == 0x0a000000 {
        let amount = ((w >> 10) & 63) as u8;
        if u32::from(amount) >= bits {
            return Some(Err("reserved logical shift amount".into()));
        }
        return Some(Ok(Op::Logical {
            width,
            dst: reg(rd, false),
            src: reg(rn, false),
            operation: ((w >> 29) & 3) as u8,
            rhs: Operand::Shifted {
                value: reg((w >> 16) & 31, false),
                shift: ((w >> 22) & 3) as u8,
                amount,
                invert: w & (1 << 21) != 0,
            },
        }));
    }
    if w & 0x1f800000 == 0x12000000 {
        let operation = ((w >> 29) & 3) as u8;
        return Some(
            masks(
                width,
                w & (1 << 22) != 0,
                ((w >> 16) & 63) as u8,
                ((w >> 10) & 63) as u8,
                true,
            )
            .map(|(mask, _)| Op::Logical {
                width,
                dst: reg(rd, operation != 3),
                src: reg(rn, false),
                rhs: Operand::Immediate(mask),
                operation,
            }),
        );
    }
    if w & 0x1f800000 == 0x13000000 {
        let operation = ((w >> 29) & 3) as u8;
        let rotate = ((w >> 16) & 63) as u8;
        let size = ((w >> 10) & 63) as u8;
        if operation == 3
            || (w & (1 << 22) != 0) != (width == Width::X64)
            || u32::from(rotate) >= bits
            || u32::from(size) >= bits
        {
            return Some(Err("reserved bitfield encoding".into()));
        }
        return Some(masks(width, w & (1 << 22) != 0, rotate, size, false).map(
            |(wmask, tmask)| Op::Bitfield {
                width,
                dst: reg(rd, false),
                src: reg(rn, false),
                operation,
                rotate,
                sign_bit: size,
                wmask,
                tmask,
            },
        ));
    }
    if w & 0x7fa00000 == 0x13800000 {
        let amount = ((w >> 10) & 63) as u8;
        if (w & (1 << 22) != 0) != (width == Width::X64) || u32::from(amount) >= bits {
            return Some(Err("reserved extract encoding".into()));
        }
        return Some(Ok(Op::Extract {
            width,
            dst: reg(rd, false),
            high: reg(rn, false),
            low: reg((w >> 16) & 31, false),
            amount,
        }));
    }
    if matches!(
        w & 0x7fe0fc00,
        0x1ac02000 | 0x1ac02400 | 0x1ac02800 | 0x1ac02c00
    ) {
        return Some(Ok(Op::VariableShift {
            width,
            dst: reg(rd, false),
            src: reg(rn, false),
            amount: reg((w >> 16) & 31, false),
            kind: ((w >> 10) & 3) as u8,
        }));
    }
    if w & 0x1fe00000 == 0x0b200000 {
        let shift = ((w >> 10) & 7) as u8;
        if shift > 4 {
            return Some(Err("reserved extended arithmetic shift".into()));
        }
        let flags = w & (1 << 29) != 0;
        return Some(Ok(Op::AddSubExtended {
            width,
            dst: reg(rd, !flags),
            lhs: reg(rn, true),
            rhs: reg((w >> 16) & 31, false),
            extend: ((w >> 13) & 7) as u8,
            shift,
            sub: w & (1 << 30) != 0,
            flags,
        }));
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exhaustive_logical_encoding_counts() {
        for (width, expected) in [(Width::W32, 3648), (Width::X64, 7680)] {
            let mut count = 0;
            for n in [false, true] {
                for r in 0..64 {
                    for s in 0..64 {
                        if let Ok((mask, _)) = masks(width, n, r, s, true) {
                            assert_ne!(mask, 0);
                            assert_ne!(mask, ones(if width == Width::X64 { 64 } else { 32 }));
                            count += 1;
                        }
                    }
                }
            }
            assert_eq!(count, expected);
        }
    }
    #[test]
    fn reserved_encodings_fail() {
        for w in [0x12400000, 0x1200fc00, 0x73400000, 0x13808000, 0x0b205400] {
            assert!(super::super::decode(w, 0).is_err(), "{w:08x}")
        }
    }
}
