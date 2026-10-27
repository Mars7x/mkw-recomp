//! Non-saturating SIMD integer arithmetic, masks and scalar reductions.
use super::Op;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Add,
    Subtract,
    Equal,
    Test,
    SignedGe,
    SignedGt,
    UnsignedGe,
    UnsignedGt,
    SignedLe,
    SignedLt,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Reduction {
    Sum,
    SignedLong,
    UnsignedLong,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VectorInteger {
    Lanes {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: Option<u8>,
        kind: Kind,
    },
    Reduce {
        bytes: u8,
        total: u8,
        dst: u8,
        src: u8,
        kind: Reduction,
    },
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    // Preserve ordinary scalar FP, including overlapping FCCMP/FCSEL fields.
    if w & (1 << 28) != 0 && w & (1 << 30) == 0 {
        return None;
    }
    let q = w & (1 << 30) != 0;
    let size = ((w >> 22) & 3) as u8;
    let bytes = 1 << size;
    let dst = (w & 31) as u8;
    let lhs = ((w >> 5) & 31) as u8;
    let reduction = match w & 0xbf3ffc00 {
        0x0e303800 => Some(Reduction::SignedLong),
        0x2e303800 => Some(Reduction::UnsignedLong),
        0x0e31b800 => Some(Reduction::Sum),
        _ => None,
    };
    if let Some(kind) = reduction {
        if size == 3 || (size == 2 && !q) {
            return Some(Err("reserved integer reduction arrangement".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::Reduce {
            bytes,
            total: if q { 16 } else { 8 },
            dst,
            src: lhs,
            kind,
        })));
    }
    use Kind::*;
    let register = match w & 0xaf20fc00 {
        0x0e208400 => Some(Add),
        0x2e208400 => Some(Subtract),
        0x2e208c00 => Some(Equal),
        0x0e208c00 => Some(Test),
        0x0e203c00 => Some(SignedGe),
        0x0e203400 => Some(SignedGt),
        0x2e203c00 => Some(UnsignedGe),
        0x2e203400 => Some(UnsignedGt),
        _ => None,
    };
    let (kind, rhs) = if let Some(kind) = register {
        (kind, Some(((w >> 16) & 31) as u8))
    } else {
        let kind = match w & 0xaf3ffc00 {
            0x0e209800 => Equal,
            0x2e208800 => SignedGe,
            0x0e208800 => SignedGt,
            0x2e209800 => SignedLe,
            0x0e20a800 => SignedLt,
            _ => return None,
        };
        (kind, None)
    };
    let scalar = w & (1 << 28) != 0;
    if (scalar && size != 3) || (!scalar && size == 3 && !q) {
        return Some(Err("reserved integer lane arrangement".into()));
    }
    Some(Ok(Op::VectorInteger(VectorInteger::Lanes {
        bytes,
        total: if scalar {
            bytes
        } else if q {
            16
        } else {
            8
        },
        dst,
        lhs,
        rhs,
        kind,
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lane_reduction_sizes_and_scalar_domain_boundaries() {
        for (w, bytes, total) in [
            (0x4e228420, 1, 16),
            (0x0e628420, 2, 8),
            (0x4ea28420, 4, 16),
            (0x5ee28420, 8, 8),
            (0x6ee23c20, 8, 16),
        ] {
            assert!(
                matches!(crate::decode(w,0).unwrap().op,Op::VectorInteger(VectorInteger::Lanes{bytes:b,total:t,..})if(b,t)==(bytes,total))
            );
        }
        for (w, bytes, total, kind) in [
            (0x6e303820, 1, 16, Reduction::UnsignedLong),
            (0x0e703820, 2, 8, Reduction::SignedLong),
            (0x4eb1b820, 4, 16, Reduction::Sum),
        ] {
            assert!(
                matches!(crate::decode(w,0).unwrap().op,Op::VectorInteger(VectorInteger::Reduce{bytes:b,total:t,kind:k,..})if(b,t,k)==(bytes,total,kind))
            );
        }
        for w in [0x5e228420, 0x0ee28420, 0x2eb03820, 0x0eb1b820, 0x6ef03820] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
        assert!(matches!(
            crate::decode(0x1e228420, 0).unwrap().op,
            Op::ConditionalFloat { .. }
        ));
        assert!(matches!(
            crate::decode(0x1e228c20, 0).unwrap().op,
            Op::FloatSelect { .. }
        ));
    }
}
