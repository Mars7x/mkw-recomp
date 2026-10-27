//! Ordinary A64 scalar/SIMD transfer formats. Atomic/SVE/MTE/unprivileged forms
//! require separate semantics and never fall through as ordinary transfers.
use super::{reg, Op, Reg, Width};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ValueReg {
    Gpr(Reg),
    Vector(u8),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum AddressMode {
    Offset(i64),
    PreIndex(i64),
    PostIndex(i64),
    Register { index: Reg, extend: u8, shift: u8 },
    Literal(u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Transfer {
    pub load: bool,
    pub bytes: u8,
    pub first: ValueReg,
    pub second: Option<ValueReg>,
    pub base: Reg,
    pub address: AddressMode,
    pub signed: bool,
    pub width: Width,
    pub non_temporal: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Prefetch {
    pub hint: u8,
    pub base: Reg,
    pub address: AddressMode,
    pub unscaled: bool,
}
fn prefetch(w: u32, pc: u64) -> Option<Result<Op, String>> {
    let hint = (w & 31) as u8;
    let base = reg((w >> 5) & 31, true);
    let (address, unscaled) = if w & 0xff000000 == 0xd8000000 {
        (
            AddressMode::Literal(pc.wrapping_add((signed_offset(w) * 4) as u64)),
            false,
        )
    } else if w & 0xffc00000 == 0xf9800000 {
        (AddressMode::Offset(i64::from((w >> 10) & 4095) * 8), false)
    } else if w & 0xffe00c00 == 0xf8800000 {
        (AddressMode::Offset(signed((w >> 12) & 511, 9)), true)
    } else if w & 0xffe00c00 == 0xf8a00800 {
        let extend = ((w >> 13) & 7) as u8;
        if !matches!(extend, 2 | 3 | 6 | 7) {
            return Some(Err("reserved prefetch register extension".into()));
        }
        (
            AddressMode::Register {
                index: reg((w >> 16) & 31, false),
                extend,
                shift: if w & (1 << 12) != 0 { 3 } else { 0 },
            },
            false,
        )
    } else {
        return None;
    };
    if base == Reg::Sp && !matches!(address, AddressMode::Literal(_)) {
        return Some(Err("stack-pointer prefetch profile unsupported".into()));
    }
    Some(Ok(Op::Prefetch(Prefetch {
        hint,
        base: if matches!(address, AddressMode::Literal(_)) {
            Reg::Zero
        } else {
            base
        },
        address,
        unscaled,
    })))
}
fn signed(value: u32, bits: u32) -> i64 {
    ((value as i64) << (64 - bits)) >> (64 - bits)
}
fn scalar(size: u32, opc: u32) -> Result<(bool, u8, bool, Width), String> {
    let bytes = 1u8 << size;
    match opc {
        0 => Ok((
            false,
            bytes,
            false,
            if size == 3 { Width::X64 } else { Width::W32 },
        )),
        1 => Ok((
            true,
            bytes,
            false,
            if size == 3 { Width::X64 } else { Width::W32 },
        )),
        2 if size < 3 => Ok((true, bytes, true, Width::X64)),
        3 if size < 2 => Ok((true, bytes, true, Width::W32)),
        _ => Err("prefetch/reserved scalar transfer".into()),
    }
}
pub fn decode(w: u32, pc: u64) -> Option<Result<Op, String>> {
    if let Some(result) = prefetch(w, pc) {
        return Some(result);
    }
    let vector = w & (1 << 26) != 0;
    let rt = w & 31;
    let rn = (w >> 5) & 31;
    let base = reg(rn, true);
    let value = |r: u32| {
        if vector {
            ValueReg::Vector(r as u8)
        } else {
            ValueReg::Gpr(reg(r, false))
        }
    };
    let make = |load, bytes, signed, width, address, second, non_temporal| {
        Op::Memory(Transfer {
            load,
            bytes,
            first: value(rt),
            second,
            base,
            address,
            signed,
            width,
            non_temporal,
        })
    };
    if w & 0x3b000000 == 0x18000000 {
        let opc = w >> 30;
        let format = if vector {
            match opc {
                0 => Ok((4, false)),
                1 => Ok((8, false)),
                2 => Ok((16, false)),
                _ => Err("reserved SIMD literal"),
            }
        } else {
            match opc {
                0 => Ok((4, false)),
                1 => Ok((8, false)),
                2 => Ok((4, true)),
                _ => Err("prefetch literal unsupported"),
            }
        };
        return Some(
            format
                .map(|(bytes, signed)| {
                    make(
                        true,
                        bytes,
                        signed,
                        if bytes == 8 || signed {
                            Width::X64
                        } else {
                            Width::W32
                        },
                        AddressMode::Literal(pc.wrapping_add((signed_offset(w) * 4) as u64)),
                        None,
                        false,
                    )
                })
                .map_err(String::from),
        );
    }
    if w & 0x3a000000 == 0x28000000 {
        let opc = w >> 30;
        let load = w & (1 << 22) != 0;
        let mode = (w >> 23) & 3;
        let rt2 = (w >> 10) & 31;
        let (bytes, sign, width) = if vector {
            match opc {
                0 => (4, false, Width::W32),
                1 => (8, false, Width::X64),
                2 => (16, false, Width::X64),
                _ => return Some(Err("reserved SIMD pair size".into())),
            }
        } else {
            match opc {
                0 => (4, false, Width::W32),
                1 if load && mode != 0 => (4, true, Width::X64),
                2 => (8, false, Width::X64),
                _ => return Some(Err("reserved scalar pair".into())),
            }
        };
        if load && rt == rt2 {
            return Some(Err("constrained overlapping pair destinations".into()));
        }
        if !vector && matches!(mode, 1 | 3) && rn != 31 && (rn == rt || rn == rt2) {
            return Some(Err("constrained pair writeback overlap".into()));
        }
        let offset = signed((w >> 15) & 127, 7) * i64::from(bytes);
        let address = match mode {
            1 => AddressMode::PostIndex(offset),
            3 => AddressMode::PreIndex(offset),
            _ => AddressMode::Offset(offset),
        };
        return Some(Ok(make(
            load,
            bytes,
            sign,
            width,
            address,
            Some(value(rt2)),
            mode == 0,
        )));
    }
    let address = if w & 0x3b000000 == 0x39000000 {
        // The scaled immediate is finalized after decoding the transfer width.
        Some((0, 0))
    } else if w & 0x3b200000 == 0x38000000 {
        let mode = (w >> 10) & 3;
        if mode == 2 {
            return Some(Err("unprivileged transfer unsupported".into()));
        }
        Some((1, mode))
    } else if w & 0x3b200c00 == 0x38200800 {
        Some((2, 0))
    } else {
        None
    };
    let (kind, mode) = address?;
    let size = w >> 30;
    let opc = (w >> 22) & 3;
    let format = if vector {
        if opc >= 2 {
            if size != 0 {
                Err("reserved SIMD transfer size".into())
            } else {
                Ok((opc & 1 != 0, 16, false, Width::X64))
            }
        } else {
            Ok((opc & 1 != 0, 1 << size, false, Width::X64))
        }
    } else {
        scalar(size, opc)
    };
    Some(format.and_then(|(load, bytes, sign, width)| {
        let addr = match kind {
            0 => AddressMode::Offset(i64::from((w >> 10) & 4095) * i64::from(bytes)),
            1 => {
                if !vector && mode != 0 && rn != 31 && rn == rt {
                    return Err("constrained transfer writeback overlap".into());
                }
                let offset = signed((w >> 12) & 511, 9);
                match mode {
                    1 => AddressMode::PostIndex(offset),
                    3 => AddressMode::PreIndex(offset),
                    _ => AddressMode::Offset(offset),
                }
            }
            _ => {
                let extend = ((w >> 13) & 7) as u8;
                if !matches!(extend, 2 | 3 | 6 | 7) {
                    return Err("reserved register-offset extension".into());
                }
                AddressMode::Register {
                    index: reg((w >> 16) & 31, false),
                    extend,
                    shift: if w & (1 << 12) != 0 {
                        bytes.trailing_zeros() as u8
                    } else {
                        0
                    },
                }
            }
        };
        Ok(make(load, bytes, sign, width, addr, None, false))
    }))
}
fn signed_offset(w: u32) -> i64 {
    signed((w >> 5) & 0x7ffff, 19)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constrain_reserved_and_writeback_forms() {
        for word in [
            0xfdc00000,
            0x7d800000,
            0xf8400c00,
            0x3c400800,
            0xf8620820,
            0xadc107e0 | 0x40000000,
        ] {
            assert!(super::super::decode(word, 0).is_err(), "{word:08x}");
        }
    }
    #[test]
    fn literal_vector_and_signed_modes() {
        assert!(matches!(
            super::super::decode(0x9c000080, 0x1000).unwrap().op,
            Op::Memory(Transfer {
                first: ValueReg::Vector(0),
                bytes: 16,
                address: AddressMode::Literal(0x1010),
                ..
            })
        ));
        assert!(matches!(
            super::super::decode(0x39800020, 0).unwrap().op,
            Op::Memory(Transfer {
                signed: true,
                width: Width::X64,
                bytes: 1,
                ..
            })
        ));
    }
}

#[cfg(test)]
mod prefetch_tests {
    use super::*;
    #[test]
    fn prefetch_address_hint_fields_and_extreme_offsets() {
        let Op::Prefetch(p) = crate::decode(0xf9800000 | (4095 << 10) | (1 << 5) | 31, 0x1000)
            .unwrap()
            .op
        else {
            panic!()
        };
        assert_eq!(
            (p.hint, p.base, p.address, p.unscaled),
            (31, Reg::Gpr(1), AddressMode::Offset(32760), false)
        );
        let Op::Prefetch(p) = crate::decode(0xf8800000 | (256 << 12) | (1 << 5) | 3, 0)
            .unwrap()
            .op
        else {
            panic!()
        };
        assert_eq!(
            (p.hint, p.base, p.address, p.unscaled),
            (3, Reg::Gpr(1), AddressMode::Offset(-256), true)
        );
        let Op::Prefetch(p) = crate::decode(
            0xf8a00800 | (31 << 16) | (6 << 13) | (1 << 12) | (1 << 5) | 7,
            0,
        )
        .unwrap()
        .op
        else {
            panic!()
        };
        assert_eq!(
            p.address,
            AddressMode::Register {
                index: Reg::Zero,
                extend: 6,
                shift: 3
            }
        );
        let Op::Prefetch(p) = crate::decode(0xd8000000 | (0x40000 << 5) | 9, 0x1000)
            .unwrap()
            .op
        else {
            panic!()
        };
        assert_eq!(
            (p.hint, p.base, p.address),
            (
                9,
                Reg::Zero,
                AddressMode::Literal(0x1000u64.wrapping_sub(1048576))
            )
        );
    }
    #[test]
    fn prefetch_does_not_accept_reserved_writeback_unprivileged_or_extensions() {
        for w in [
            0xf98003e0, 0xf88003e0, 0xf8a26be0, 0xf8800420, 0xf8800c20, 0xf8800820, 0xf8a20820,
            0xf9c00020, 0xdc000020,
        ] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
    }
}
