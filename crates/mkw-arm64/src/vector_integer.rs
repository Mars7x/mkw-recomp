//! SIMD integer arithmetic, masks, reductions and saturating high products.
use super::Op;
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Kind {
    Add,
    Subtract,
    Negate,
    SignedMax,
    SignedMin,
    UnsignedMax,
    UnsignedMin,
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
    SignedMax,
    SignedMin,
    UnsignedMax,
    UnsignedMin,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Accumulate {
    Product,
    Add,
    Subtract,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum NarrowKind {
    Signed,
    Unsigned,
    SignedUnsigned,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum VectorInteger {
    MultiplyLow {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        rhs_lane: Option<u8>,
    },
    PairwiseAdd {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: Option<u8>,
    },
    RightShift {
        bytes: u8,
        total: u8,
        dst: u8,
        src: u8,
        amount: u8,
        signed: bool,
        rounding: bool,
        accumulate: bool,
    },
    WidenArithmetic {
        bytes: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        upper: bool,
        signed: bool,
        wide_lhs: bool,
        subtract: bool,
    },
    SaturatingShiftNarrow {
        bytes: u8,
        dst: u8,
        src: u8,
        scalar: bool,
        upper: bool,
        kind: NarrowKind,
        amount: u8,
        rounding: bool,
    },
    RegisterShift {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        signed: bool,
    },
    SaturatingNarrow {
        bytes: u8,
        dst: u8,
        src: u8,
        scalar: bool,
        upper: bool,
        kind: NarrowKind,
    },
    Saturating {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        signed: bool,
        subtract: bool,
    },
    WidenMultiply {
        bytes: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        upper: bool,
        signed: bool,
        accumulate: Accumulate,
        rhs_lane: Option<u8>,
    },
    MultiplyHigh {
        bytes: u8,
        total: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        rounding: bool,
        rhs_lane: Option<u8>,
    },
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
    let multiply_low_regular = w & 0xbf20fc00 == 0x0e209c00;
    let multiply_low_indexed = w & 0xbf00f400 == 0x0f008000;
    if multiply_low_regular || multiply_low_indexed {
        if size == 3 || (multiply_low_indexed && size == 0) {
            return Some(Err("reserved SIMD multiply width".into()));
        }
        let rhs = ((w >> 16)
            & if multiply_low_indexed && size == 1 {
                15
            } else {
                31
            }) as u8;
        let rhs_lane = if multiply_low_indexed {
            Some(if size == 1 {
                (((w >> 11) & 1) * 4 + ((w >> 21) & 1) * 2 + ((w >> 20) & 1)) as u8
            } else {
                (((w >> 11) & 1) * 2 + ((w >> 21) & 1)) as u8
            })
        } else {
            None
        };
        return Some(Ok(Op::VectorInteger(VectorInteger::MultiplyLow {
            bytes,
            total: if q { 16 } else { 8 },
            dst,
            lhs,
            rhs,
            rhs_lane,
        })));
    }
    if w & 0xbf20fc00 == 0x0e20bc00 {
        if !q && size == 3 {
            return Some(Err("reserved vector ADDP width".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::PairwiseAdd {
            bytes,
            total: if q { 16 } else { 8 },
            dst,
            lhs,
            rhs: Some(((w >> 16) & 31) as u8),
        })));
    }
    if w & 0xff3ffc00 == 0x5e31b800 {
        if size != 3 {
            return Some(Err("reserved scalar ADDP width".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::PairwiseAdd {
            bytes: 8,
            total: 8,
            dst,
            lhs,
            rhs: None,
        })));
    }
    let right_shift = match w & 0xaf80fc00 {
        0x0f001400 => Some((true, false, true)),
        0x2f001400 => Some((false, false, true)),
        0x0f002400 => Some((true, true, false)),
        0x2f002400 => Some((false, true, false)),
        0x0f003400 => Some((true, true, true)),
        0x2f003400 => Some((false, true, true)),
        _ => None,
    };
    if let Some((signed, rounding, accumulate)) = right_shift {
        let imm = ((w >> 16) & 127) as u8;
        if imm < 8 {
            return None;
        }
        let bits = 1u8 << (7 - imm.leading_zeros());
        let scalar = w & (1 << 28) != 0;
        if (scalar && bits != 64) || (!scalar && !q && bits == 64) {
            return Some(Err("reserved right shift arrangement".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::RightShift {
            bytes: bits / 8,
            total: if scalar {
                8
            } else if q {
                16
            } else {
                8
            },
            dst,
            src: lhs,
            amount: (u16::from(bits) * 2 - u16::from(imm)) as u8,
            signed,
            rounding,
            accumulate,
        })));
    }
    let widen_arithmetic = match w & 0x9f20fc00 {
        0x0e200000 => Some((false, false)),
        0x0e202000 => Some((false, true)),
        0x0e201000 => Some((true, false)),
        0x0e203000 => Some((true, true)),
        _ => None,
    };
    if let Some((wide_lhs, subtract)) = widen_arithmetic {
        if size == 3 {
            return Some(Err("reserved widening arithmetic width".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::WidenArithmetic {
            bytes,
            dst,
            lhs,
            rhs: ((w >> 16) & 31) as u8,
            upper: q,
            signed: w & (1 << 29) == 0,
            wide_lhs,
            subtract,
        })));
    }
    let shift_narrow = match w & 0xaf80fc00 {
        0x0f009400 => Some((NarrowKind::Signed, false)),
        0x0f009c00 => Some((NarrowKind::Signed, true)),
        0x2f009400 => Some((NarrowKind::Unsigned, false)),
        0x2f009c00 => Some((NarrowKind::Unsigned, true)),
        0x2f008400 => Some((NarrowKind::SignedUnsigned, false)),
        0x2f008c00 => Some((NarrowKind::SignedUnsigned, true)),
        _ => None,
    };
    if let Some((kind, rounding)) = shift_narrow {
        let imm = ((w >> 16) & 127) as u8;
        if imm < 8 {
            return None;
        }
        if imm >= 64 {
            return Some(Err("reserved saturating narrowing shift width".into()));
        }
        let bits = 1u8 << (7 - imm.leading_zeros());
        let scalar = w & (1 << 28) != 0;
        return Some(Ok(Op::VectorInteger(
            VectorInteger::SaturatingShiftNarrow {
                bytes: bits / 8,
                dst,
                src: lhs,
                scalar,
                upper: q && !scalar,
                kind,
                amount: bits * 2 - imm,
                rounding,
            },
        )));
    }
    let register_shift = match w & 0xaf20fc00 {
        0x0e204400 => Some(true),
        0x2e204400 => Some(false),
        _ => None,
    };
    if let Some(signed) = register_shift {
        let scalar = w & (1 << 28) != 0;
        if (scalar && size != 3) || (!scalar && !q && size == 3) {
            return Some(Err("reserved register shift arrangement".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::RegisterShift {
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
            rhs: ((w >> 16) & 31) as u8,
            signed,
        })));
    }
    let narrow = match w & 0xaf3ffc00 {
        0x0e214800 => Some(NarrowKind::Signed),
        0x2e214800 => Some(NarrowKind::Unsigned),
        0x2e212800 => Some(NarrowKind::SignedUnsigned),
        _ => None,
    };
    if let Some(kind) = narrow {
        if size == 3 {
            return Some(Err("reserved saturating narrowing width".into()));
        }
        let scalar = w & (1 << 28) != 0;
        return Some(Ok(Op::VectorInteger(VectorInteger::SaturatingNarrow {
            bytes,
            dst,
            src: lhs,
            scalar,
            upper: q && !scalar,
            kind,
        })));
    }
    let saturating = match w & 0xaf20fc00 {
        0x0e200c00 => Some((true, false)),
        0x2e200c00 => Some((false, false)),
        0x0e202c00 => Some((true, true)),
        0x2e202c00 => Some((false, true)),
        _ => None,
    };
    if let Some((signed, subtract)) = saturating {
        let scalar = w & (1 << 28) != 0;
        if !scalar && !q && size == 3 {
            return Some(Err("reserved saturating arithmetic arrangement".into()));
        }
        return Some(Ok(Op::VectorInteger(VectorInteger::Saturating {
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
            rhs: ((w >> 16) & 31) as u8,
            signed,
            subtract,
        })));
    }
    let regular = match w & 0x9f20fc00 {
        0x0e20c000 => Some(Accumulate::Product),
        0x0e208000 => Some(Accumulate::Add),
        0x0e20a000 => Some(Accumulate::Subtract),
        _ => None,
    };
    let indexed_widen = match w & 0x9f00f400 {
        0x0f00a000 => Some(Accumulate::Product),
        0x0f002000 => Some(Accumulate::Add),
        0x0f006000 => Some(Accumulate::Subtract),
        _ => None,
    };
    if let Some(accumulate) = regular.or(indexed_widen) {
        if size == 3 || (indexed_widen.is_some() && size == 0) {
            return Some(Err("reserved widening multiply width".into()));
        }
        let indexed = indexed_widen.is_some();
        let rhs = ((w >> 16) & if indexed && size == 1 { 15 } else { 31 }) as u8;
        let rhs_lane = if indexed {
            Some(if size == 1 {
                (((w >> 11) & 1) * 4 + ((w >> 21) & 1) * 2 + ((w >> 20) & 1)) as u8
            } else {
                (((w >> 11) & 1) * 2 + ((w >> 21) & 1)) as u8
            })
        } else {
            None
        };
        return Some(Ok(Op::VectorInteger(VectorInteger::WidenMultiply {
            bytes,
            dst,
            lhs,
            rhs,
            upper: q,
            signed: w & (1 << 29) == 0,
            accumulate,
            rhs_lane,
        })));
    }
    let indexed = match w & 0xaf00f400 {
        0x0f00c000 => Some(false),
        0x0f00d000 => Some(true),
        _ => None,
    };
    if let Some(rounding) = indexed {
        if size != 1 && size != 2 {
            return Some(Err("reserved indexed saturating high-product width".into()));
        }
        let high = ((w >> 11) & 1) as u8;
        let low = ((w >> 21) & 1) as u8;
        let middle = ((w >> 20) & 1) as u8;
        let lane = if size == 1 {
            high * 4 + low * 2 + middle
        } else {
            high * 2 + low
        };
        let rhs = ((w >> 16) & if size == 1 { 15 } else { 31 }) as u8;
        let scalar = w & (1 << 28) != 0;
        return Some(Ok(Op::VectorInteger(VectorInteger::MultiplyHigh {
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
            rounding,
            rhs_lane: Some(lane),
        })));
    }
    let multiply = match w & 0xaf20fc00 {
        0x0e20b400 => Some(false),
        0x2e20b400 => Some(true),
        _ => None,
    };
    if let Some(rounding) = multiply {
        if size != 1 && size != 2 {
            return Some(Err("reserved saturating high-product width".into()));
        }
        let scalar = w & (1 << 28) != 0;
        return Some(Ok(Op::VectorInteger(VectorInteger::MultiplyHigh {
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
            rhs: ((w >> 16) & 31) as u8,
            rounding,
            rhs_lane: None,
        })));
    }
    let reduction = match w & 0xbf3ffc00 {
        0x0e303800 => Some(Reduction::SignedLong),
        0x2e303800 => Some(Reduction::UnsignedLong),
        0x0e31b800 => Some(Reduction::Sum),
        0x0e30a800 => Some(Reduction::SignedMax),
        0x0e31a800 => Some(Reduction::SignedMin),
        0x2e30a800 => Some(Reduction::UnsignedMax),
        0x2e31a800 => Some(Reduction::UnsignedMin),
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
        0x0e206400 => Some(SignedMax),
        0x0e206c00 => Some(SignedMin),
        0x2e206400 => Some(UnsignedMax),
        0x2e206c00 => Some(UnsignedMin),
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
            0x2e20b800 => Negate,
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
    if matches!(kind, SignedMax | SignedMin | UnsignedMax | UnsignedMin) && (scalar || size == 3) {
        return Some(Err("reserved integer minmax arrangement".into()));
    }
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

#[cfg(test)]
mod minmax_tests {
    use super::*;
    #[test]
    fn minmax_arrangements_and_scalar_neighbors() {
        for (base, kind) in [
            (0x0e206400, Kind::SignedMax),
            (0x0e206c00, Kind::SignedMin),
            (0x2e206400, Kind::UnsignedMax),
            (0x2e206c00, Kind::UnsignedMin),
        ] {
            for q in 0..2 {
                for size in 0..4 {
                    let w = base | (q << 30) | (size << 22) | (31 << 16) | (31 << 5) | 31;
                    if size == 3 {
                        assert!(crate::decode(w, 0).is_err());
                        continue;
                    }
                    assert!(
                        matches!(crate::decode(w,0).unwrap().op, Op::VectorInteger(VectorInteger::Lanes{kind:k,bytes,total,dst:31,lhs:31,rhs:Some(31)}) if (k,bytes,total)==(kind,1<<size,8<<q))
                    );
                    assert!(crate::decode(w | (1 << 28) | (1 << 30), 0).is_err());
                }
            }
        }
        for base in [0x0e30a800, 0x0e31a800, 0x2e30a800, 0x2e31a800] {
            for q in 0..2 {
                for size in 0..4 {
                    let w = base | (q << 30) | (size << 22) | (31 << 5) | 31;
                    assert_eq!(
                        crate::decode(w, 0).is_ok(),
                        size < 3 && (size != 2 || q == 1)
                    );
                }
            }
        }
        assert!(matches!(
            crate::decode(0x1e226420, 0).unwrap().op,
            Op::ConditionalFloat { .. }
        ));
        assert!(matches!(
            crate::decode(0x1e226c20, 0).unwrap().op,
            Op::FloatSelect { .. }
        ));
    }
}

#[cfg(test)]
mod negate_tests {
    use super::*;
    #[test]
    fn negate_arrangements_and_domains() {
        for scalar in 0..2 {
            for q in 0..2 {
                for size in 0..4 {
                    let w = 0x2e20bbff | (scalar << 28) | (q << 30) | (size << 22);
                    if scalar != 0 && q == 0 {
                        assert!(decode(w).is_none());
                        continue;
                    }
                    if (scalar != 0 && size != 3) || (scalar == 0 && q == 0 && size == 3) {
                        assert!(crate::decode(w, 0).is_err());
                        continue;
                    }
                    assert!(
                        matches!(crate::decode(w,0).unwrap().op,Op::VectorInteger(VectorInteger::Lanes{dst:31,lhs:31,rhs:None,bytes,kind:Kind::Negate,..})if bytes==1<<size)
                    );
                }
            }
        }
        assert!(decode(0x6e207820).is_none());
    }
}

#[cfg(test)]
mod multiply_high_tests {
    use super::*;
    #[test]
    fn signed_high_product_widths_and_scalar_domain() {
        for rounding in 0..2 {
            for scalar in 0..2 {
                for q in 0..2 {
                    for size in 0..4 {
                        let w = 0x0e3fb7ff
                            | (rounding << 29)
                            | (scalar << 28)
                            | (q << 30)
                            | (size << 22);
                        if scalar == 1 && q == 0 {
                            assert!(decode(w).is_none());
                            continue;
                        }
                        if size != 1 && size != 2 {
                            assert!(crate::decode(w, 0).is_err(), "{w:x}");
                            continue;
                        }
                        assert!(
                            matches!(crate::decode(w,0).unwrap().op,Op::VectorInteger(VectorInteger::MultiplyHigh{bytes,total,dst:31,lhs:31,rhs:31,rounding:r,..})if(bytes,total,r)==(1<<size,if scalar==1{1<<size}else{8<<q},rounding!=0))
                        );
                    }
                }
            }
        }
        for w in [0x2e20bc20, 0x4e209420] {
            assert!(decode(w).is_none());
        }
    }
}

#[cfg(test)]
mod indexed_product_tests {
    use super::*;
    #[test]
    fn indexed_product_lanes_and_shared_register_bits() {
        for size in [1, 2] {
            for lane in 0..if size == 1 { 8 } else { 4 } {
                let index = if size == 1 {
                    ((lane >> 2) << 11) | (((lane >> 1) & 1) << 21) | ((lane & 1) << 20)
                } else {
                    ((lane >> 1) << 11) | ((lane & 1) << 21)
                };
                let w = 0x4f00d3ff
                    | (size << 22)
                    | index
                    | (15 << 16)
                    | if size == 2 { 1 << 20 } else { 0 };
                assert!(
                    matches!(crate::decode(w,0).unwrap().op,Op::VectorInteger(VectorInteger::MultiplyHigh{dst:31,lhs:31,rhs,rhs_lane:Some(n),..})if (rhs,n)==(if size==1{15}else{31},lane as u8))
                );
            }
        }
        for w in [0x4f00d020, 0x4fc0d020] {
            assert!(crate::decode(w, 0).is_err());
        }
        assert!(decode(0x1f40d020).is_none());
    }
}

#[cfg(test)]
mod widening_tests {
    use super::*;
    #[test]
    fn widening_families_and_reserved_widths() {
        for base in [
            0x0e20c000, 0x0e208000, 0x0e20a000, 0x0f00a000, 0x0f002000, 0x0f006000,
        ] {
            for unsigned in 0..2 {
                for upper in 0..2 {
                    for size in 0..4 {
                        let word = base | (unsigned << 29) | (upper << 30) | (size << 22) | 0x20020;
                        let result = decode(word).unwrap();
                        if size == 3 || (base & 0x01000000 != 0 && size == 0) {
                            assert!(result.is_err());
                        } else {
                            let Op::VectorInteger(VectorInteger::WidenMultiply {
                                signed,
                                upper: high,
                                bytes,
                                ..
                            }) = result.unwrap()
                            else {
                                panic!()
                            };
                            assert_eq!(
                                (signed, high, bytes),
                                (unsigned == 0, upper != 0, 1 << size)
                            );
                        }
                    }
                }
            }
        }
        assert!(decode(0x5e22c020).is_none());
    }
}

#[cfg(test)]
mod saturating_tests {
    use super::*;
    #[test]
    fn saturating_domains_and_widths() {
        for base in [0x0e200c00, 0x2e200c00, 0x0e202c00, 0x2e202c00] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for size in 0..4 {
                        let w = base | (scalar << 28) | (q << 30) | (size << 22) | 0x20020;
                        let result = decode(w);
                        if scalar != 0 && q == 0 {
                            assert!(result.is_none());
                            continue;
                        }
                        if scalar == 0 && q == 0 && size == 3 {
                            assert!(result.unwrap().is_err());
                            continue;
                        }
                        let Op::VectorInteger(VectorInteger::Saturating {
                            bytes,
                            total,
                            signed,
                            subtract,
                            ..
                        }) = result.unwrap().unwrap()
                        else {
                            panic!()
                        };
                        assert_eq!(bytes, 1 << size);
                        assert_eq!(
                            total,
                            if scalar != 0 {
                                bytes
                            } else if q != 0 {
                                16
                            } else {
                                8
                            }
                        );
                        assert_eq!(signed, base & (1 << 29) == 0);
                        assert_eq!(subtract, base & (1 << 13) != 0);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod saturating_narrow_tests {
    use super::*;
    #[test]
    fn saturating_narrow_domains_and_widths() {
        for base in [0x0e214800, 0x2e214800, 0x2e212800] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for size in 0..4 {
                        let w = base | (scalar << 28) | (q << 30) | (size << 22) | 0x20;
                        let result = decode(w);
                        if scalar != 0 && q == 0 {
                            assert!(result.is_none());
                            continue;
                        }
                        if size == 3 {
                            assert!(result.unwrap().is_err());
                            continue;
                        }
                        let Op::VectorInteger(VectorInteger::SaturatingNarrow {
                            bytes,
                            scalar: single,
                            upper,
                            ..
                        }) = result.unwrap().unwrap()
                        else {
                            panic!()
                        };
                        assert_eq!(
                            (bytes, single, upper),
                            (1 << size, scalar != 0, scalar == 0 && q != 0)
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod register_shift_tests {
    use super::*;
    #[test]
    fn register_shift_domains_and_widths() {
        for base in [0x0e204400, 0x2e204400] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for size in 0..4 {
                        let result =
                            decode(base | (scalar << 28) | (q << 30) | (size << 22) | 0x20020);
                        if scalar != 0 && q == 0 {
                            assert!(result.is_none());
                            continue;
                        }
                        if (scalar != 0 && size != 3) || (scalar == 0 && q == 0 && size == 3) {
                            assert!(result.unwrap().is_err());
                            continue;
                        }
                        let Op::VectorInteger(VectorInteger::RegisterShift {
                            bytes, total, ..
                        }) = result.unwrap().unwrap()
                        else {
                            panic!()
                        };
                        assert_eq!(
                            (bytes, total),
                            (
                                1 << size,
                                if scalar != 0 {
                                    8
                                } else if q != 0 {
                                    16
                                } else {
                                    8
                                }
                            )
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod saturating_shift_narrow_tests {
    use super::*;
    #[test]
    fn all_saturating_narrowing_amounts_and_domains() {
        for base in [
            0x0f009400, 0x0f009c00, 0x2f009400, 0x2f009c00, 0x2f008400, 0x2f008c00,
        ] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for imm in 0..128 {
                        let w = base | (scalar << 28) | (q << 30) | (imm << 16) | 0x20;
                        let result = decode(w);
                        if (scalar != 0 && q == 0) || imm < 8 {
                            assert!(result.is_none());
                            continue;
                        }
                        if imm >= 64 {
                            assert!(result.unwrap().is_err());
                            continue;
                        }
                        let Op::VectorInteger(VectorInteger::SaturatingShiftNarrow {
                            bytes,
                            amount,
                            scalar: single,
                            upper,
                            ..
                        }) = result.unwrap().unwrap()
                        else {
                            panic!()
                        };
                        assert!(amount >= 1 && amount <= bytes * 8);
                        assert_eq!(u32::from(bytes) * 16 - u32::from(amount), imm);
                        assert_eq!((single, upper), (scalar != 0, scalar == 0 && q != 0));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod widen_arithmetic_tests {
    use super::*;
    #[test]
    fn widening_arithmetic_domains_and_arrangements() {
        for base in [0x0e200000, 0x0e202000, 0x0e201000, 0x0e203000] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for unsigned in 0..2 {
                        for size in 0..4 {
                            let result = decode(
                                base | (scalar << 28)
                                    | (q << 30)
                                    | (unsigned << 29)
                                    | (size << 22)
                                    | 0x20020,
                            );
                            if scalar != 0 {
                                assert!(result.is_none());
                                continue;
                            }
                            if size == 3 {
                                assert!(result.unwrap().is_err());
                                continue;
                            }
                            let Op::VectorInteger(VectorInteger::WidenArithmetic {
                                bytes,
                                upper,
                                signed,
                                wide_lhs,
                                subtract,
                                ..
                            }) = result.unwrap().unwrap()
                            else {
                                panic!()
                            };
                            assert_eq!(
                                (bytes, upper, signed, wide_lhs, subtract),
                                (
                                    1 << size,
                                    q != 0,
                                    unsigned == 0,
                                    base & 0x1000 != 0,
                                    base & 0x2000 != 0
                                )
                            );
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod right_shift_tests {
    use super::*;
    #[test]
    fn right_shift_counts_scalar_domains_and_full_width() {
        for base in [
            0x0f001400, 0x2f001400, 0x0f002400, 0x2f002400, 0x0f003400, 0x2f003400,
        ] {
            for scalar in 0..2 {
                for q in 0..2 {
                    for imm in 0..128 {
                        let result = decode(base | (scalar << 28) | (q << 30) | (imm << 16) | 0x20);
                        if (scalar != 0 && q == 0) || imm < 8 {
                            assert!(result.is_none());
                            continue;
                        }
                        let bits = 1u32 << (31 - imm.leading_zeros());
                        if (scalar != 0 && bits != 64) || (scalar == 0 && q == 0 && bits == 64) {
                            assert!(result.unwrap().is_err());
                            continue;
                        }
                        let Op::VectorInteger(VectorInteger::RightShift {
                            bytes,
                            amount,
                            total,
                            ..
                        }) = result.unwrap().unwrap()
                        else {
                            panic!()
                        };
                        assert_eq!(u32::from(bytes) * 16 - u32::from(amount), imm);
                        assert!(amount >= 1 && amount <= bytes * 8);
                        assert_eq!(
                            total,
                            if scalar != 0 {
                                8
                            } else if q != 0 {
                                16
                            } else {
                                8
                            }
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod pairwise_add_tests {
    use super::*;
    #[test]
    fn pairwise_add_arrangements_and_scalar_domains() {
        for q in 0..2 {
            for size in 0..4 {
                let result = decode(0x0e22bc20 | (q << 30) | (size << 22)).unwrap();
                if q == 0 && size == 3 {
                    assert!(result.is_err());
                    continue;
                }
                let Op::VectorInteger(VectorInteger::PairwiseAdd {
                    bytes, total, rhs, ..
                }) = result.unwrap()
                else {
                    panic!()
                };
                assert_eq!((bytes, total, rhs), (1 << size, 8 << q, Some(2)));
            }
        }
        for size in 0..4 {
            let result = decode(0x5e31b820 | (size << 22)).unwrap();
            if size != 3 {
                assert!(result.is_err());
                continue;
            }
            let Op::VectorInteger(VectorInteger::PairwiseAdd {
                bytes, total, rhs, ..
            }) = result.unwrap()
            else {
                panic!()
            };
            assert_eq!((bytes, total, rhs), (8, 8, None));
        }
    }
}

#[cfg(test)]
mod multiply_low_tests {
    use super::*;
    #[test]
    fn ordinary_and_indexed_multiply_domains() {
        for base in [0x0e209c00, 0x0f008000] {
            for q in 0..2 {
                for size in 0..4 {
                    let result = decode(base | (q << 30) | (size << 22) | 0x20020).unwrap();
                    if size == 3 || (base == 0x0f008000 && size == 0) {
                        assert!(result.is_err());
                        continue;
                    }
                    let Op::VectorInteger(VectorInteger::MultiplyLow {
                        bytes,
                        total,
                        rhs_lane,
                        ..
                    }) = result.unwrap()
                    else {
                        panic!()
                    };
                    assert_eq!((bytes, total), (1 << size, 8 << q));
                    assert_eq!(rhs_lane.is_some(), base == 0x0f008000);
                }
            }
        }
        assert!(decode(0x5e229c20).is_none());
        assert!(decode(0x2e229c20).is_none());
    }
}
