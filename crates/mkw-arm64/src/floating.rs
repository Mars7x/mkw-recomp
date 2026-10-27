//! Scalar FP operations retain explicit FPCR/FPSR and checked helper requirements.
use super::{reg, Op, Reg, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SystemReg {
    Fpcr,
    Fpsr,
    Nzcv,
    TpidrEl0,
    TpidrroEl0,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Floating {
    pub bytes: u8,
    pub operation: u8,
    pub dst: u8,
    pub lhs: u8,
    pub rhs: u8,
    pub accumulate: u8,
    pub zero: bool,
}
pub const CONVERT_FIXED_SCALE: u32 = 0x10000;
pub const CONVERT_SCALE_SHIFT: u32 = 8;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Conversion {
    pub operation: u8,
    pub fractional_bits: u8,
    pub int_width: Width,
    pub bytes: u8,
    pub int_reg: Reg,
    pub vector: u8,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    if w & 0x7f200000 == 0x1e000000 {
        let opcode = (w >> 16) & 7;
        let rounding = (w >> 19) & 3;
        let operation = match (opcode, rounding) {
            (2 | 3, 0) => (opcode - 2) as u8,
            (0 | 1, 3) => 8 + opcode as u8,
            _ => return None,
        };
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported scaled conversion FP type".into())),
        };
        let scale = ((w >> 10) & 63) as u8;
        let width = if w >> 31 != 0 { Width::X64 } else { Width::W32 };
        if width == Width::W32 && scale < 32 {
            return Some(Err("reserved word fixed-point scale".into()));
        }
        return Some(Ok(Op::Convert(Conversion {
            operation,
            fractional_bits: 64 - scale,
            int_width: width,
            bytes,
            int_reg: reg(if operation < 2 { (w >> 5) & 31 } else { w & 31 }, false),
            vector: if operation < 2 {
                (w & 31) as u8
            } else {
                ((w >> 5) & 31) as u8
            },
        })));
    }

    if w & 0x7f20fc00 == 0x1e200000 {
        let opcode = (w >> 16) & 7;
        let rounding = (w >> 19) & 3;
        let operation = match (opcode, rounding) {
            (2 | 3, 0) => (opcode - 2) as u8,
            (0 | 1, _) => 2 + (rounding * 2 + opcode) as u8,
            (4 | 5, 0) => 10 + (opcode - 4) as u8,
            _ => return None,
        };
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported conversion FP type".into())),
        };
        return Some(Ok(Op::Convert(Conversion {
            operation,
            fractional_bits: 0,
            int_width: if w >> 31 != 0 { Width::X64 } else { Width::W32 },
            bytes,
            int_reg: reg(if operation < 2 { (w >> 5) & 31 } else { w & 31 }, false),
            vector: if operation < 2 {
                (w & 31) as u8
            } else {
                ((w >> 5) & 31) as u8
            },
        })));
    }
    if matches!(w & 0xfffffc00, 0x1e22c000 | 0x1e624000) {
        return Some(Ok(Op::Floating(Floating {
            bytes: if w & (1 << 22) != 0 { 8 } else { 4 },
            operation: 14,
            dst: (w & 31) as u8,
            lhs: ((w >> 5) & 31) as u8,
            rhs: 0,
            accumulate: 0,
            zero: false,
        })));
    }
    let unary = match w & 0xff3ffc00 {
        0x1e214000 => Some(7),
        0x1e20c000 => Some(8),
        0x1e21c000 => Some(9),
        0x1e244000 => Some(19),
        0x1e24c000 => Some(20),
        0x1e254000 => Some(21),
        0x1e25c000 => Some(22),
        0x1e264000 => Some(23),
        0x1e274000 => Some(24),
        0x1e27c000 => Some(25),
        _ => None,
    };
    if let Some(operation) = unary {
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported unary FP type".into())),
        };
        return Some(Ok(Op::Floating(Floating {
            bytes,
            operation,
            dst: (w & 31) as u8,
            lhs: ((w >> 5) & 31) as u8,
            rhs: 0,
            accumulate: 0,
            zero: false,
        })));
    }
    let system = match w & 0xffffffe0 {
        0xd53b4400 | 0xd51b4400 => Some(SystemReg::Fpcr),
        0xd53b4420 | 0xd51b4420 => Some(SystemReg::Fpsr),
        0xd53b4200 | 0xd51b4200 => Some(SystemReg::Nzcv),
        0xd53bd040 | 0xd51bd040 => Some(SystemReg::TpidrEl0),
        0xd53bd060 => Some(SystemReg::TpidrroEl0),
        _ => None,
    };
    if let Some(register) = system {
        return Some(Ok(Op::SystemRegister {
            read: w & (1 << 21) != 0,
            register,
            value: reg(w & 31, false),
        }));
    }
    let barrier = match w & 0xfffff0ff {
        0xd50330bf => Some(0),
        0xd503309f => Some(1),
        _ => None,
    };
    if let Some(kind) = barrier {
        let option = ((w >> 8) & 15) as u8;
        return Some(if option & 3 != 0 {
            Ok(Op::Barrier { kind, option })
        } else {
            Err("unsupported barrier option/profile".into())
        });
    }
    if w & 0xff200c00 == 0x1e200c00 {
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            3 => 2,
            _ => return Some(Err("reserved FP select type".into())),
        };
        return Some(Ok(Op::FloatSelect {
            bytes,
            dst: (w & 31) as u8,
            lhs: ((w >> 5) & 31) as u8,
            rhs: ((w >> 16) & 31) as u8,
            condition: ((w >> 12) & 15) as u8,
        }));
    }
    if w & 0xff000000 == 0x1f000000 {
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported fused FP type".into())),
        };
        return Some(Ok(Op::Floating(Floating {
            bytes,
            operation: match ((w >> 21) & 1, (w >> 15) & 1) {
                (0, 0) => 6,
                (0, 1) => 11,
                (1, 0) => 12,
                (1, 1) => 13,
                _ => unreachable!(),
            },
            dst: (w & 31) as u8,
            lhs: ((w >> 5) & 31) as u8,
            rhs: ((w >> 16) & 31) as u8,
            accumulate: ((w >> 10) & 31) as u8,
            zero: false,
        })));
    }
    if w & 0xff200c00 == 0x1e200400 {
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported conditional FP compare type".into())),
        };
        return Some(Ok(Op::ConditionalFloat {
            condition: ((w >> 12) & 15) as u8,
            nzcv: (w & 15) as u8,
            operation: Floating {
                bytes,
                operation: 4 + u8::from(w & (1 << 4) != 0),
                dst: 0,
                lhs: ((w >> 5) & 31) as u8,
                rhs: ((w >> 16) & 31) as u8,
                accumulate: 0,
                zero: false,
            },
        }));
    }
    let compare = w & 0xff20fc07 == 0x1e202000;
    let binary = w & 0xff200c00 == 0x1e200800;
    if !compare && !binary {
        return None;
    }
    let bytes = match (w >> 22) & 3 {
        0 => 4,
        1 => 8,
        _ => return Some(Err("unsupported scalar FP type".into())),
    };
    let (operation, zero) = if compare {
        let zero = w & (1 << 3) != 0;
        if zero && (w >> 16) & 31 != 0 {
            return Some(Err("reserved FP compare-zero operand".into()));
        }
        (4 + u8::from(w & (1 << 4) != 0), zero)
    } else {
        let operation = match (w >> 12) & 15 {
            0 => 2,
            1 => 3,
            2 => 0,
            3 => 1,
            8 => 10,
            4 => 15,
            5 => 16,
            6 => 17,
            7 => 18,
            _ => return None,
        };
        (operation, false)
    };
    Some(Ok(Op::Floating(Floating {
        bytes,
        operation,
        dst: (w & 31) as u8,
        lhs: ((w >> 5) & 31) as u8,
        rhs: ((w >> 16) & 31) as u8,
        accumulate: 0,
        zero,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_register_zero_and_barrier_profiles() {
        assert!(matches!(
            crate::decode(0xd51b441f, 0).unwrap().op,
            Op::SystemRegister {
                read: false,
                value: super::super::Reg::Zero,
                ..
            }
        ));
        assert!(matches!(
            crate::decode(0xd53b4421, 0).unwrap().op,
            Op::SystemRegister {
                read: true,
                register: SystemReg::Fpsr,
                ..
            }
        ));
        for word in [0xd5033fbf, 0xd5033a9f, 0xd50339bf] {
            assert!(matches!(
                crate::decode(word, 0).unwrap().op,
                Op::Barrier { .. }
            ));
        }
        for word in [
            0xd50330bf, 0xd503349f, 0xd5033fdf, 0xd5033f9e, 0x1ea02820, 0x1ee02820, 0x1e222028,
            0x1e202001, 0x1fa20c20, 0x1fc20c20,
        ] {
            assert!(crate::decode(word, 0).is_err(), "{word:08x}");
        }
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    #[test]
    fn conversion_width_zero_register_and_reserved_forms() {
        assert!(matches!(
            crate::decode(0x9e6203ff, 0).unwrap().op,
            Op::Convert(Conversion {
                operation: 0,
                int_width: Width::X64,
                int_reg: Reg::Zero,
                vector: 31,
                ..
            })
        ));
        assert!(matches!(
            crate::decode(0x1e38003f, 0).unwrap().op,
            Op::Convert(Conversion {
                operation: 8,
                int_width: Width::W32,
                int_reg: Reg::Zero,
                vector: 1,
                ..
            })
        ));
        for w in [0x1ee20020, 0x1ea20020, 0x1e2a0020, 0x1e2c0020, 0x1ee1c020] {
            assert!(crate::decode(w, 0).is_err(), "{w:08x}");
        }
    }
}

#[cfg(test)]
mod thread_tests {
    use super::*;
    fn system(read: bool, op2: u32, rt: u32) -> u32 {
        0xd5000000 | (u32::from(read) << 21) | (3 << 19) | (3 << 16) | (13 << 12) | (op2 << 5) | rt
    }
    #[test]
    fn guest_thread_registers_require_explicit_el0_access_forms() {
        assert!(matches!(
            crate::decode(system(true, 2, 0), 0).unwrap().op,
            Op::SystemRegister {
                register: SystemReg::TpidrEl0,
                read: true,
                ..
            }
        ));
        assert!(matches!(
            crate::decode(system(false, 2, 31), 0).unwrap().op,
            Op::SystemRegister {
                register: SystemReg::TpidrEl0,
                read: false,
                value: Reg::Zero
            }
        ));
        assert!(matches!(
            crate::decode(system(true, 3, 31), 0).unwrap().op,
            Op::SystemRegister {
                register: SystemReg::TpidrroEl0,
                read: true,
                value: Reg::Zero
            }
        ));
        assert!(crate::decode(system(false, 3, 0), 0).is_err());
    }
}

#[cfg(test)]
mod conditional_fp_tests {
    use super::*;
    #[test]
    fn conditional_fp_fields_and_unsupported_widths() {
        assert!(matches!(
            crate::decode(0x1e22043f, 0).unwrap().op,
            Op::ConditionalFloat {
                condition: 0,
                nzcv: 15,
                operation: Floating {
                    operation: 5,
                    bytes: 4,
                    ..
                }
            }
        ));
        assert!(matches!(
            crate::decode(0x1e628820, 0).unwrap().op,
            Op::Floating(Floating {
                operation: 10,
                bytes: 8,
                ..
            })
        ));
        for word in [0x1ee20420, 0x1ea20420, 0x1ee28820] {
            assert!(crate::decode(word, 0).is_err(), "{word:08x}");
        }
    }
}

#[cfg(test)]
mod fused_tests {
    use super::*;
    #[test]
    fn fused_sign_variants_preserve_accumulator_and_width_fields() {
        for (signs, operation) in [
            (0, 6),
            (1 << 15, 11),
            (1 << 21, 12),
            ((1 << 21) | (1 << 15), 13),
        ] {
            for (ty, bytes) in [(0, 4), (1, 8)] {
                let w = 0x1f1f7fff | signs | (ty << 22);
                let Op::Floating(f) = crate::decode(w, 0).unwrap().op else {
                    panic!()
                };
                assert_eq!(
                    (f.bytes, f.operation, f.dst, f.lhs, f.rhs, f.accumulate),
                    (bytes, operation, 31, 31, 31, 31)
                );
            }
            for ty in [2, 3] {
                assert!(crate::decode(0x1f020c20 | signs | (ty << 22), 0).is_err());
            }
        }
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    #[test]
    fn format_conversion_directions_and_reserved_or_half_types() {
        for (base, bytes) in [(0x1e22c000, 4), (0x1e624000, 8)] {
            let Op::Floating(f) = crate::decode(base | 0x3ff, 0).unwrap().op else {
                panic!()
            };
            assert_eq!((f.bytes, f.operation, f.dst, f.lhs), (bytes, 14, 31, 31));
        }
        for w in [0x1e224020, 0x1e62c020, 0x1ee24020, 0x1e23c020, 0x1ea24020] {
            assert!(crate::decode(w, 0).is_err(), "{w:08x}");
        }
    }
}

#[cfg(test)]
mod minmax_tests {
    use super::*;
    #[test]
    fn minmax_register_width_and_half_refusal() {
        for (encoded, operation) in [(4, 15), (5, 16), (6, 17), (7, 18)] {
            for (ty, bytes) in [(0, 4), (1, 8)] {
                let Op::Floating(f) = crate::decode(0x1e3f0bff | (encoded << 12) | (ty << 22), 0)
                    .unwrap()
                    .op
                else {
                    panic!()
                };
                assert_eq!(
                    (f.operation, f.bytes, f.dst, f.lhs, f.rhs),
                    (operation, bytes, 31, 31, 31)
                );
            }
            for ty in [2, 3] {
                assert!(crate::decode(0x1e220820 | (encoded << 12) | (ty << 22), 0).is_err());
            }
        }
    }
}

#[cfg(test)]
mod rounding_tests {
    use super::*;
    #[test]
    fn integral_rounding_widths_and_reserved_types() {
        for (base, operation) in [
            (0x1e244000, 19),
            (0x1e24c000, 20),
            (0x1e254000, 21),
            (0x1e25c000, 22),
            (0x1e264000, 23),
            (0x1e274000, 24),
            (0x1e27c000, 25),
        ] {
            for (ty, bytes) in [(0, 4), (1, 8)] {
                let Op::Floating(v) = crate::decode(base | (ty << 22) | 0x3ff, 0).unwrap().op
                else {
                    panic!()
                };
                assert_eq!(
                    (v.operation, v.bytes, v.dst, v.lhs),
                    (operation, bytes, 31, 31)
                );
            }
            for ty in [2, 3] {
                assert!(crate::decode(base | (ty << 22) | 0x20, 0).is_err());
            }
        }
        for q in 0..2 {
            for size in 0..2 {
                for base in [
                    0x0e218820, 0x0ea18820, 0x0e219820, 0x0ea19820, 0x2e218820, 0x2e219820,
                    0x2ea19820,
                ] {
                    let w = base | (q << 30) | (size << 22);
                    if q == 0 && size == 1 {
                        assert!(crate::decode(w, 0).is_err());
                    } else {
                        assert!(matches!(crate::decode(w, 0).unwrap().op, Op::VectorFp(_)));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod scaled_tests {
    use super::*;
    #[test]
    fn fixed_point_scales_and_neighbor_domains() {
        for (bits, sf) in [(32, 0), (64, 1u32 << 31)] {
            for fbits in 1..=bits {
                for (base, operation) in [
                    (0x1e020000, 0),
                    (0x1e030000, 1),
                    (0x1e180000, 8),
                    (0x1e190000, 9),
                ] {
                    for ty in 0..2 {
                        let w = base | sf | (ty << 22) | ((64 - fbits) << 10) | 0x3ff;
                        let Op::Convert(v) = crate::decode(w, 0).unwrap().op else {
                            panic!()
                        };
                        assert_eq!(
                            (v.operation, v.fractional_bits, v.bytes),
                            (operation, fbits as u8, 4 << ty)
                        );
                    }
                }
            }
        }
        for w in [0x1e180020, 0x1ee2c020, 0x1ea2c020] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
        assert!(matches!(
            crate::decode(0x1e220020, 0).unwrap().op,
            Op::Convert(Conversion {
                fractional_bits: 0,
                ..
            })
        ));
        assert!(matches!(
            crate::decode(0x1f30fc20, 0).unwrap().op,
            Op::Floating(_)
        ));
        assert!(matches!(
            crate::decode(0x4f00e420, 0).unwrap().op,
            Op::Simd(_)
        ));
        for w in [0x4f30fc20, 0x6f30e420, 0x5f70fc20] {
            assert!(matches!(crate::decode(w, 0).unwrap().op, Op::VectorFp(_)));
        }
        assert!(crate::decode(0x0f70fc20, 0).is_err());
    }
}
