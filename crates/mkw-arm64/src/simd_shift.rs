//! Non-saturating immediate SIMD shifts, inserts and widening shifts.
use super::Op;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Left,
    UnsignedRight,
    SignedRight,
    InsertLeft,
    InsertRight,
    UnsignedWiden,
    SignedWiden,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Shift {
    pub bytes: u8,
    pub total: u8,
    pub dst: u8,
    pub src: u8,
    pub amount: u8,
    pub upper: bool,
    pub kind: Kind,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    if w & (1 << 28) != 0 && w & (1 << 30) == 0 {
        return None;
    }
    if w & 0xaf3ffc00 == 0x2e213800 {
        let size = ((w >> 22) & 3) as u8;
        if size == 3 || w & (1 << 28) != 0 {
            return Some(Err("reserved SHLL arrangement".into()));
        }
        let bytes = 1 << size;
        return Some(Ok(Op::SimdShift(Shift {
            bytes,
            total: 16,
            dst: (w & 31) as u8,
            src: ((w >> 5) & 31) as u8,
            amount: bytes * 8,
            upper: w & (1 << 30) != 0,
            kind: Kind::UnsignedWiden,
        })));
    }
    use Kind::*;
    let kind = match w & 0xaf80fc00 {
        0x0f005400 => Left,
        0x2f005400 => InsertLeft,
        0x0f000400 => SignedRight,
        0x2f000400 => UnsignedRight,
        0x2f004400 => InsertRight,
        0x0f00a400 => SignedWiden,
        0x2f00a400 => UnsignedWiden,
        _ => return None,
    };
    let imm = ((w >> 16) & 127) as u8;
    if imm < 8 {
        return None;
    }
    let bits = 1u8 << (7 - imm.leading_zeros());
    let bytes = bits / 8;
    let scalar = w & (1 << 28) != 0;
    let q = w & (1 << 30) != 0;
    let widen = matches!(kind, UnsignedWiden | SignedWiden);
    if (widen && (scalar || bytes == 8))
        || (!widen && ((scalar && bytes != 8) || (!scalar && bytes == 8 && !q)))
    {
        return Some(Err("reserved SIMD immediate-shift arrangement".into()));
    }
    let amount = if matches!(kind, SignedRight | UnsignedRight | InsertRight) {
        (u16::from(bits) * 2 - u16::from(imm)) as u8
    } else {
        imm - bits
    };
    Some(Ok(Op::SimdShift(Shift {
        bytes,
        total: if widen || q && !scalar {
            16
        } else if scalar {
            bytes
        } else {
            8
        },
        dst: (w & 31) as u8,
        src: ((w >> 5) & 31) as u8,
        amount,
        upper: widen && q,
        kind,
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shifts_widening_bounds_and_neighbor_domains() {
        for (w, bytes, total, amount, upper) in [
            (0x4f095420, 1, 16, 1, false),
            (0x0f080420, 1, 8, 8, false),
            (0x5f415420, 8, 8, 1, false),
            (0x2f09a420, 1, 16, 1, false),
            (0x6f09a420, 1, 16, 1, true),
        ] {
            let Op::SimdShift(v) = crate::decode(w, 0).unwrap().op else {
                panic!()
            };
            assert_eq!(
                (v.bytes, v.total, v.amount, v.upper),
                (bytes, total, amount, upper)
            );
        }
        for w in [0x0f415420, 0x5f095420, 0x6f40a420, 0x5f09a420] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
        assert!(matches!(
            crate::decode(0x4f005420, 0).unwrap().op,
            Op::Simd(_)
        ));
        assert!(matches!(
            crate::decode(0x1f095420, 0).unwrap().op,
            Op::Floating(_)
        ));
    }
}

#[cfg(test)]
mod shll_tests {
    use super::*;
    #[test]
    fn full_width_widening_and_reserved_fields() {
        for q in 0..2 {
            for size in 0..4 {
                let w = 0x2e213800 | (q << 30) | (size << 22) | (31 << 5) | 31;
                if size == 3 {
                    assert!(crate::decode(w, 0).is_err());
                    continue;
                }
                let Op::SimdShift(v) = crate::decode(w, 0).unwrap().op else {
                    panic!()
                };
                assert_eq!(
                    (v.bytes, v.total, v.amount, v.upper, v.dst, v.src, v.kind),
                    (
                        1 << size,
                        16,
                        8 << size,
                        q != 0,
                        31,
                        31,
                        Kind::UnsignedWiden
                    )
                );
                assert!(crate::decode(w | (1 << 28) | (1 << 30), 0).is_err());
            }
        }
        assert!(decode(0x2e203820).is_none());
        assert!(decode(0x2e213c20).is_none());
    }
}
