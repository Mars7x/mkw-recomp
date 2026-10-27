//! S/D Advanced SIMD numeric operations. Half/scaled/structure forms are separate.
use super::Op;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VectorFpKind {
    Arithmetic(u8),
    Conversion(u8),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct VectorFp {
    pub bytes: u8,
    pub total: u8,
    pub dst: u8,
    pub lhs: u8,
    pub rhs: u8,
    pub kind: VectorFpKind,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    let scalar = w & 0xdfbffc00 == 0x5e21d800 || w & 0xdfbffc00 == 0x5ea1b800;
    let conversion = scalar || w & 0x9fbffc00 == 0x0e21d800 || w & 0x9fbffc00 == 0x0ea1b800;
    let kind = if conversion {
        let unsigned = u8::from(w & (1 << 29) != 0);
        VectorFpKind::Conversion(if w & (1 << 23) != 0 {
            8 + unsigned
        } else {
            unsigned
        })
    } else {
        let binary = match w & 0xbfa0fc00 {
            0x0e20d400 => Some(0),
            0x0ea0d400 => Some(1),
            0x2e20dc00 => Some(2),
            0x2e20fc00 => Some(3),
            0x0e20cc00 => Some(6),
            _ => None,
        };
        if let Some(op) = binary {
            VectorFpKind::Arithmetic(op)
        } else {
            VectorFpKind::Arithmetic(match w & 0xbfbffc00 {
                0x0ea0f800 => 8,
                0x2ea0f800 => 7,
                0x2ea1f800 => 9,
                _ => return None,
            })
        }
    };
    let bytes = if w & (1 << 22) != 0 { 8 } else { 4 };
    let total = if scalar {
        bytes
    } else if w & (1 << 30) != 0 {
        16
    } else {
        8
    };
    if !scalar && bytes == 8 && total == 8 {
        return Some(Err("reserved single-double vector arrangement".into()));
    }
    Some(Ok(Op::VectorFp(VectorFp {
        bytes,
        total,
        dst: (w & 31) as u8,
        lhs: ((w >> 5) & 31) as u8,
        rhs: ((w >> 16) & 31) as u8,
        kind,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrangements_and_scalar_conversion_are_bounded() {
        for base in [
            0x0e22d420, 0x0ea2d420, 0x2e22dc20, 0x2e22fc20, 0x0e22cc20, 0x0ea0f820, 0x2ea0f820,
            0x2ea1f820, 0x0e21d820, 0x2e21d820, 0x0ea1b820, 0x2ea1b820,
        ] {
            for q in 0..2 {
                for size in 0..2 {
                    let decoded = decode(base | (q << 30) | (size << 22)).unwrap();
                    if q == 0 && size == 1 {
                        assert!(decoded.is_err());
                        continue;
                    }
                    let Op::VectorFp(v) = decoded.unwrap() else {
                        panic!()
                    };
                    assert_eq!(v.bytes, 4 << size);
                    assert_eq!(v.total, 8 << q);
                    assert_eq!((v.dst, v.lhs), (0, 1));
                }
            }
        }
        for base in [0x5e21d820, 0x7e21d820, 0x5ea1b820, 0x7ea1b820] {
            for size in 0..2 {
                let Op::VectorFp(v) = decode(base | (size << 22)).unwrap().unwrap() else {
                    panic!()
                };
                assert_eq!((v.bytes, v.total), (4 << size, 4 << size));
            }
        }
        for unsupported in [
            0x4e22d420 ^ (1 << 21),
            0x4e22d420 | (1 << 23) | (1 << 29),
            0x4e21d820 ^ (1 << 10),
        ] {
            assert!(decode(unsupported).is_none());
        }
    }
}
