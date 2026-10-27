//! Raw SIMD/FP register transport. No host floating-point arithmetic is used.
use super::{reg, Op, Reg, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum LaneSource {
    General(Reg),
    Vector { register: u8, lane: u8 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ImmediateKind {
    Move,
    Or,
    Bic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Simd {
    Immediate {
        dst: u8,
        value: u64,
        bytes: u8,
        kind: ImmediateKind,
    },
    Scalar {
        dst: u8,
        bytes: u8,
        source: LaneSource,
    },
    Duplicate {
        dst: u8,
        bytes: u8,
        vector_bytes: u8,
        source: LaneSource,
    },
    Insert {
        dst: u8,
        lane: u8,
        bytes: u8,
        source: LaneSource,
    },
    Extract {
        dst: Reg,
        width: Width,
        src: u8,
        lane: u8,
        bytes: u8,
        signed: bool,
    },
    Logical {
        dst: u8,
        lhs: u8,
        rhs: u8,
        bytes: u8,
        operation: u8,
    },
    ExtractVector {
        dst: u8,
        lhs: u8,
        rhs: u8,
        bytes: u8,
        offset: u8,
    },
}
fn repeat(value: u64, bits: u32) -> u64 {
    let mut out = 0;
    for shift in (0..64).step_by(bits as usize) {
        out |= value << shift;
    }
    out
}
/// Architectural compact FP immediates expand directly into IEEE bit patterns.
pub fn float_immediate(imm: u8, bits: u32) -> u64 {
    let (exponent, fraction) = match bits {
        16 => (5, 10),
        32 => (8, 23),
        64 => (11, 52),
        _ => unreachable!(),
    };
    let b = u64::from((imm >> 6) & 1);
    let exp = ((b ^ 1) << (exponent - 1))
        | ((b * ((1 << (exponent - 3)) - 1)) << 2)
        | u64::from((imm >> 4) & 3);
    (u64::from(imm >> 7) << (bits - 1))
        | (exp << fraction)
        | (u64::from(imm & 15) << (fraction - 4))
}
fn scalar_bytes(ftype: u32) -> Result<u8, String> {
    match ftype {
        0 => Ok(4),
        1 => Ok(8),
        3 => Ok(2),
        _ => Err("reserved scalar FP type".into()),
    }
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    let dst = (w & 31) as u8;
    let src = ((w >> 5) & 31) as u8;
    let q = w & (1 << 30) != 0;
    let vector_bytes = if q { 16 } else { 8 };
    let vector = |lane| LaneSource::Vector {
        register: src,
        lane,
    };
    let make = |op| Ok(Op::Simd(op));
    if w & 0xff3ffc00 == 0x1e204000 {
        return Some(scalar_bytes((w >> 22) & 3).and_then(|bytes| {
            make(Simd::Scalar {
                dst,
                bytes,
                source: vector(0),
            })
        }));
    }
    if w & 0xff201fe0 == 0x1e201000 {
        return Some(scalar_bytes((w >> 22) & 3).and_then(|bytes| {
            make(Simd::Immediate {
                dst,
                value: float_immediate(((w >> 13) & 255) as u8, u32::from(bytes) * 8),
                bytes,
                kind: ImmediateKind::Move,
            })
        }));
    }
    if w & 0x7f26fc00 == 0x1e260000 {
        let sf = w >> 31 != 0;
        let ftype = (w >> 22) & 3;
        let rmode = (w >> 19) & 3;
        let high = sf && ftype == 2 && rmode == 1;
        let bytes = if high {
            8
        } else if rmode == 0 {
            match (sf, ftype) {
                (false, 0) => 4,
                (true, 1) => 8,
                (_, 3) => 2,
                _ => return Some(Err("unsupported FP/general move type".into())),
            }
        } else {
            return Some(Err("unsupported FP conversion encoding".into()));
        };
        let to_vector = w & (1 << 16) != 0;
        return Some(make(if to_vector {
            if high {
                Simd::Insert {
                    dst,
                    lane: 1,
                    bytes,
                    source: LaneSource::General(reg(u32::from(src), false)),
                }
            } else {
                Simd::Scalar {
                    dst,
                    bytes,
                    source: LaneSource::General(reg(u32::from(src), false)),
                }
            }
        } else {
            Simd::Extract {
                dst: reg(u32::from(dst), false),
                width: if sf { Width::X64 } else { Width::W32 },
                src,
                lane: u8::from(high),
                bytes,
                signed: false,
            }
        }));
    }
    if w & 0x9ff80400 == 0x0f000400 {
        let op = w & (1 << 29) != 0;
        let mode = (w >> 12) & 15;
        let o2 = w & (1 << 11) != 0;
        let imm = (((w >> 16) & 7) << 5) | ((w >> 5) & 31);
        if o2 {
            if mode != 15 || op {
                return Some(Err("reserved half-vector immediate".into()));
            }
            return Some(make(Simd::Immediate {
                dst,
                value: repeat(float_immediate(imm as u8, 16), 16),
                bytes: vector_bytes,
                kind: ImmediateKind::Move,
            }));
        }
        let mut value = match mode {
            0..=7 => repeat(u64::from(imm) << (8 * (mode / 2)), 32),
            8..=11 => repeat(u64::from(imm) << (8 * ((mode / 2) & 1)), 16),
            12 => repeat((u64::from(imm) << 8) | 255, 32),
            13 => repeat((u64::from(imm) << 16) | 65535, 32),
            14 if !op => repeat(u64::from(imm), 8),
            14 => {
                let mut v = 0;
                for bit in 0..8 {
                    if imm & (1 << bit) != 0 {
                        v |= 255u64 << (bit * 8);
                    }
                }
                v
            }
            _ if !op => repeat(float_immediate(imm as u8, 32), 32),
            _ if q => float_immediate(imm as u8, 64),
            _ => return Some(Err("reserved double-vector immediate".into())),
        };
        let kind = if mode < 12 && mode & 1 != 0 {
            if op {
                ImmediateKind::Bic
            } else {
                ImmediateKind::Or
            }
        } else {
            if op && mode < 14 {
                value = !value;
            }
            ImmediateKind::Move
        };
        return Some(make(Simd::Immediate {
            dst,
            value,
            bytes: vector_bytes,
            kind,
        }));
    }
    let scalar_copy = w & 0xffe08400 == 0x5e000400;
    if scalar_copy || w & 0x9fe08400 == 0x0e000400 {
        let imm5 = (w >> 16) & 31;
        let imm4 = (w >> 11) & 15;
        let size = imm5.trailing_zeros();
        if size > 3 {
            return Some(Err("reserved SIMD element size".into()));
        }
        let bytes = 1u8 << size;
        let lane = (imm5 >> (size + 1)) as u8;
        if scalar_copy {
            return Some(if imm4 == 0 {
                make(Simd::Scalar {
                    dst,
                    bytes,
                    source: vector(lane),
                })
            } else {
                Err("reserved scalar copy opcode".into())
            });
        }
        if w & (1 << 29) != 0 {
            if !q || imm4 & ((1 << size) - 1) != 0 {
                return Some(Err("reserved element insert".into()));
            }
            return Some(make(Simd::Insert {
                dst,
                lane,
                bytes,
                source: vector((imm4 >> size) as u8),
            }));
        }
        return Some(match imm4 {
            0 | 1 if q || size < 3 => make(Simd::Duplicate {
                dst,
                bytes,
                vector_bytes,
                source: if imm4 == 0 {
                    vector(lane)
                } else {
                    LaneSource::General(reg(u32::from(src), false))
                },
            }),
            3 if q => make(Simd::Insert {
                dst,
                lane,
                bytes,
                source: LaneSource::General(reg(u32::from(src), false)),
            }),
            5 if (q && size < 3) || (!q && size < 2) => make(Simd::Extract {
                dst: reg(u32::from(dst), false),
                width: if q { Width::X64 } else { Width::W32 },
                src,
                lane,
                bytes,
                signed: true,
            }),
            7 if (q && size == 3) || (!q && size < 3) => make(Simd::Extract {
                dst: reg(u32::from(dst), false),
                width: if q { Width::X64 } else { Width::W32 },
                src,
                lane,
                bytes,
                signed: false,
            }),
            _ => Err("reserved SIMD copy opcode/width".into()),
        });
    }
    if w & 0x9f20fc00 == 0x0e201c00 {
        return Some(make(Simd::Logical {
            dst,
            lhs: src,
            rhs: ((w >> 16) & 31) as u8,
            bytes: vector_bytes,
            operation: (((w >> 29) & 1) * 4 + ((w >> 22) & 3)) as u8,
        }));
    }
    if w & 0xbfe08400 == 0x2e000000 {
        let offset = ((w >> 11) & 15) as u8;
        return Some(if offset < vector_bytes {
            make(Simd::ExtractVector {
                dst,
                lhs: src,
                rhs: ((w >> 16) & 31) as u8,
                bytes: vector_bytes,
                offset,
            })
        } else {
            Err("reserved EXT offset".into())
        });
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn reserved_simd_encodings_reject() {
        for word in [
            0x1ea04000, 0x2f00f400, 0x6f00fc00, 0x0f00ec00, 0x0e000400, 0x0e080c00, 0x0e081c00,
            0x4e013c00, 0x0e042c00, 0x6e020c00, 0x2e004000,
        ] {
            assert!(crate::decode(word, 0).is_err(), "{word:08x}");
        }
    }
    #[test]
    fn immediate_bit_patterns_and_register31() {
        use super::*;
        for (bits, expected) in [(16, 0x3c00), (32, 0x3f800000), (64, 0x3ff0000000000000)] {
            assert_eq!(float_immediate(0x70, bits), expected);
            assert_eq!(float_immediate(0xf0, bits), expected | (1u64 << (bits - 1)));
        }
        assert!(matches!(
            crate::decode(0x1e2703ff, 0).unwrap().op,
            Op::Simd(Simd::Scalar {
                dst: 31,
                source: LaneSource::General(Reg::Zero),
                ..
            })
        ));
    }
}
