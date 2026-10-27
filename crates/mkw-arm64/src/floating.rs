//! Scalar FP operations retain explicit FPCR/FPSR and checked helper requirements.
use super::{reg, Op, Reg, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SystemReg {
    Fpcr,
    Fpsr,
    Nzcv,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Conversion {
    pub operation: u8,
    pub int_width: Width,
    pub bytes: u8,
    pub int_reg: Reg,
    pub vector: u8,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
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
    let unary = match w & 0xff3ffc00 {
        0x1e214000 => Some(7),
        0x1e20c000 => Some(8),
        0x1e21c000 => Some(9),
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
    if w & 0xff208000 == 0x1f000000 {
        let bytes = match (w >> 22) & 3 {
            0 => 4,
            1 => 8,
            _ => return Some(Err("unsupported fused FP type".into())),
        };
        return Some(Ok(Op::Floating(Floating {
            bytes,
            operation: 6,
            dst: (w & 31) as u8,
            lhs: ((w >> 5) & 31) as u8,
            rhs: ((w >> 16) & 31) as u8,
            accumulate: ((w >> 10) & 31) as u8,
            zero: false,
        })));
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
            0x1e202001, 0x1f220c20, 0x1f028c20, 0x1fc20c20,
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
