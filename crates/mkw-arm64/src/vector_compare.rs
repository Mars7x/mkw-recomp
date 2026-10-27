//! S/D SIMD comparison masks, including scalar and compare-zero forms.
use super::Op;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Relation {
    Equal,
    GreaterEqual,
    Greater,
    LessEqual,
    Less,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Compare {
    pub bytes: u8,
    pub total: u8,
    pub dst: u8,
    pub lhs: u8,
    /// None means the architectural floating zero constant, never V0.
    pub rhs: Option<u8>,
    pub relation: Relation,
    pub absolute: bool,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    use Relation::*;
    // Ordinary scalar FP encodings share low fields with SIMD compares. Leave
    // their Q=0/scalar=1 domain to the scalar decoder (FCCMP/FCSEL included).
    if w & (1 << 28) != 0 && w & (1 << 30) == 0 {
        return None;
    }
    let register = match w & 0xafa0fc00 {
        0x0e20e400 => Some((Equal, false)),
        0x2e20e400 => Some((GreaterEqual, false)),
        0x2ea0e400 => Some((Greater, false)),
        0x2e20ec00 => Some((GreaterEqual, true)),
        0x2ea0ec00 => Some((Greater, true)),
        _ => None,
    };
    let (relation, absolute, rhs) = if let Some((relation, absolute)) = register {
        (relation, absolute, Some(((w >> 16) & 31) as u8))
    } else {
        let relation = match w & 0xafbffc00 {
            0x0ea0d800 => Equal,
            0x2ea0c800 => GreaterEqual,
            0x0ea0c800 => Greater,
            0x2ea0d800 => LessEqual,
            0x0ea0e800 => Less,
            _ => return None,
        };
        (relation, false, None)
    };
    let scalar = w & (1 << 28) != 0;
    let q = w & (1 << 30) != 0;
    let bytes = if w & (1 << 22) != 0 { 8 } else { 4 };
    let total = if scalar {
        bytes
    } else if q {
        16
    } else {
        8
    };
    if !scalar && bytes == 8 && !q {
        return Some(Err("reserved SIMD comparison arrangement".into()));
    }
    Some(Ok(Op::VectorCompare(Compare {
        bytes,
        total,
        dst: (w & 31) as u8,
        lhs: ((w >> 5) & 31) as u8,
        rhs,
        relation,
        absolute,
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_relations_scalar_zero_and_reserved_forms() {
        for (w, bytes, total, relation, absolute, rhs) in [
            (0x4e22e420, 4, 16, Relation::Equal, false, Some(2)),
            (0x6e62e420, 8, 16, Relation::GreaterEqual, false, Some(2)),
            (0x7ea2ec20, 4, 4, Relation::Greater, true, Some(2)),
            (0x5ee0e820, 8, 8, Relation::Less, false, None),
            (0x2ea0d820, 4, 8, Relation::LessEqual, false, None),
        ] {
            let Op::VectorCompare(v) = crate::decode(w, 0).unwrap().op else {
                panic!()
            };
            assert_eq!(
                (v.bytes, v.total, v.relation, v.absolute, v.rhs),
                (bytes, total, relation, absolute, rhs)
            );
        }
        assert!(matches!(
            crate::decode(0x1e22e420, 0).unwrap().op,
            Op::ConditionalFloat { condition: 14, .. }
        ));
        assert!(matches!(
            crate::decode(0x1e22ec20, 0).unwrap().op,
            Op::FloatSelect { condition: 14, .. }
        ));
        for w in [0x2e62e420, 0x4e42e420, 0x5e62ec20, 0x4ea1d820] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
    }
}
