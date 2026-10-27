//! Incremental AArch64 semantics; supplied code is surveyed, while execution remains diagnostic.
//! PAC operations carry explicit semantics; a configured backend is required at execution.
pub mod atomics;
pub mod bitops;
pub mod floating;
pub mod integer;
pub mod memory;
pub mod simd;
pub mod simd_shift;
pub mod structures;
pub mod vector_compare;
pub mod vector_fp;
pub mod vector_integer;
use serde::Serialize;
pub const SOURCE_INPUTS: &[&str] = &[
    include_str!("lib.rs"),
    include_str!("memory.rs"),
    include_str!("bitops.rs"),
    include_str!("integer.rs"),
    include_str!("simd.rs"),
    include_str!("simd_shift.rs"),
    include_str!("structures.rs"),
    include_str!("atomics.rs"),
    include_str!("floating.rs"),
    include_str!("vector_fp.rs"),
    include_str!("vector_integer.rs"),
    include_str!("vector_compare.rs"),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Width {
    W32,
    X64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Reg {
    Gpr(u8),
    Sp,
    Zero,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u32)]
pub enum MemoryOrder {
    AcquireRcpc = 1,
    AcquireRcsc = 2,
    Release = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Op {
    Nop,
    YieldHint,
    VectorFp(vector_fp::VectorFp),
    VectorInteger(vector_integer::VectorInteger),
    VectorCompare(vector_compare::Compare),
    Structure(structures::Structure),
    InterleavedStructure(structures::Interleaved),
    SimdShift(simd_shift::Shift),
    Crc32 {
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        bytes: u8,
        castagnoli: bool,
    },
    IntegerUnary {
        width: Width,
        dst: Reg,
        src: Reg,
        operation: u8,
    },
    CarryArithmetic {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        sub: bool,
        flags: bool,
    },
    Floating(floating::Floating),
    ConditionalFloat {
        operation: floating::Floating,
        condition: u8,
        nzcv: u8,
    },
    Convert(floating::Conversion),
    FloatSelect {
        bytes: u8,
        dst: u8,
        lhs: u8,
        rhs: u8,
        condition: u8,
    },
    SystemRegister {
        read: bool,
        register: floating::SystemReg,
        value: Reg,
    },
    Barrier {
        kind: u8,
        option: u8,
    },
    Atomic(atomics::Atomic),
    Simd(simd::Simd),
    Multiply {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        acc: Reg,
        sub: bool,
        kind: integer::MultiplyKind,
    },
    Divide {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        signed: bool,
    },
    Logical {
        width: Width,
        dst: Reg,
        src: Reg,
        rhs: bitops::Operand,
        operation: u8,
    },
    Bitfield {
        width: Width,
        dst: Reg,
        src: Reg,
        operation: u8,
        rotate: u8,
        sign_bit: u8,
        wmask: u64,
        tmask: u64,
    },
    Extract {
        width: Width,
        dst: Reg,
        high: Reg,
        low: Reg,
        amount: u8,
    },
    VariableShift {
        width: Width,
        dst: Reg,
        src: Reg,
        amount: Reg,
        kind: u8,
    },
    AddSubExtended {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        extend: u8,
        shift: u8,
        sub: bool,
        flags: bool,
    },
    Select {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        condition: u8,
        operation: u8,
    },
    ConditionalCompare {
        width: Width,
        lhs: Reg,
        rhs: Reg,
        immediate: Option<u8>,
        condition: u8,
        nzcv: u8,
        sub: bool,
    },
    TailBranch {
        target: u64,
    },
    IndirectBranch {
        target: Reg,
    },
    IndirectCall {
        target: Reg,
    },
    TestBit {
        src: Reg,
        bit: u8,
        target: u64,
        nonzero: bool,
    },
    AddSubReg {
        width: Width,
        dst: Reg,
        lhs: Reg,
        rhs: Reg,
        shift: u8,
        amount: u8,
        sub: bool,
        flags: bool,
    },
    OrderedMemory {
        order: MemoryOrder,
        bytes: u8,
        value: Reg,
        base: Reg,
    },
    Address {
        dst: Reg,
        address: u64,
        page: bool,
    },
    Call {
        target: u64,
    },
    Memory(memory::Transfer),
    PointerAuth {
        key: u8,
        operation: u8,
    },
    AuthReturn {
        key: u8,
    },
    MoveReg {
        width: Width,
        dst: Reg,
        src: Reg,
    },
    MoveWide {
        width: Width,
        dst: Reg,
        imm: u64,
        keep: bool,
        invert: bool,
        shift: u8,
    },
    AddSubImm {
        width: Width,
        dst: Reg,
        src: Reg,
        imm: u64,
        sub: bool,
        flags: bool,
    },
    Branch {
        target: u64,
    },
    CompareBranch {
        width: Width,
        src: Reg,
        target: u64,
        nonzero: bool,
    },
    BranchCond {
        target: u64,
        cond: u8,
    },
    Ret {
        target: Reg,
    },
}
#[derive(Clone, Debug, Serialize)]
pub struct Instruction {
    pub pc: u64,
    pub word: u32,
    pub op: Op,
}
fn reg(n: u32, sp: bool) -> Reg {
    if n == 31 {
        if sp {
            Reg::Sp
        } else {
            Reg::Zero
        }
    } else {
        Reg::Gpr(n as u8)
    }
}
fn target(pc: u64, imm: u32, bits: u32) -> u64 {
    let signed = ((imm as i64) << (64 - bits)) >> (64 - bits);
    pc.wrapping_add((signed * 4) as u64)
}
pub fn decode(word: u32, pc: u64) -> Result<Instruction, String> {
    let width = if word >> 31 != 0 {
        Width::X64
    } else {
        Width::W32
    };
    let rd = word & 31;
    let rn = (word >> 5) & 31;
    let op = if let Some(result) = structures::decode(word) {
        result
            .map_err(|error| format!("unsupported instruction {word:08x} ({error}) at {pc:#x}"))?
    } else if let Some(result) = simd_shift::decode(word) {
        result
            .map_err(|error| format!("unsupported instruction {word:08x} ({error}) at {pc:#x}"))?
    } else if let Some(result) = vector_integer::decode(word) {
        result
            .map_err(|error| format!("unsupported instruction {word:08x} ({error}) at {pc:#x}"))?
    } else if let Some(result) = vector_compare::decode(word) {
        result
            .map_err(|error| format!("unsupported instruction {word:08x} ({error}) at {pc:#x}"))?
    } else if let Some(result) = vector_fp::decode(word) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if let Some(result) = floating::decode(word) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if let Some(op) = atomics::decode(word) {
        op
    } else if let Some(result) = simd::decode(word) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if let Some(result) = integer::decode(word, width) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if let Some(result) = bitops::decode(word, width) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if let Some(result) = memory::decode(word, pc) {
        result.map_err(|error| format!("{error} at {pc:#x}"))?
    } else if word & 0x3fe00800 == 0x1a800000 {
        Op::Select {
            width,
            dst: reg(rd, false),
            lhs: reg(rn, false),
            rhs: reg((word >> 16) & 31, false),
            condition: ((word >> 12) & 15) as u8,
            operation: ((((word >> 30) & 1) << 1) | ((word >> 10) & 1)) as u8,
        }
    } else if word & 0x3fe00410 == 0x3a400000 {
        Op::ConditionalCompare {
            width,
            lhs: reg(rn, false),
            rhs: reg((word >> 16) & 31, false),
            immediate: if word & (1 << 11) != 0 {
                Some(((word >> 16) & 31) as u8)
            } else {
                None
            },
            condition: ((word >> 12) & 15) as u8,
            nzcv: (word & 15) as u8,
            sub: word & (1 << 30) != 0,
        }
    } else if word & 0xfffffc1f == 0xd61f0000 {
        Op::IndirectBranch {
            target: reg(rn, false),
        }
    } else if word & 0xfffffc1f == 0xd63f0000 {
        Op::IndirectCall {
            target: reg(rn, false),
        }
    } else if word & 0x7e000000 == 0x36000000 {
        Op::TestBit {
            src: reg(rd, false),
            bit: (((word >> 31) << 5) | ((word >> 19) & 31)) as u8,
            target: target(pc, (word >> 5) & 0x3fff, 14),
            nonzero: word & (1 << 24) != 0,
        }
    } else if word & 0x1f200000 == 0x0b000000 {
        let shift = ((word >> 22) & 3) as u8;
        let amount = ((word >> 10) & 63) as u8;
        if shift == 3 || (width == Width::W32 && amount >= 32) {
            return Err(format!("reserved shifted arithmetic at {pc:#x}"));
        }
        Op::AddSubReg {
            width,
            dst: reg(rd, false),
            lhs: reg(rn, false),
            rhs: reg((word >> 16) & 31, false),
            shift,
            amount,
            sub: word & (1 << 30) != 0,
            flags: word & (1 << 29) != 0,
        }
    } else if word & 0x3ffffc00 == 0x38bfc000 {
        Op::OrderedMemory {
            order: MemoryOrder::AcquireRcpc,
            bytes: 1u8 << (word >> 30),
            value: reg(rd, false),
            base: reg(rn, true),
        }
    } else if matches!(word & 0x3ffffc00, 0x08dffc00 | 0x089ffc00) {
        Op::OrderedMemory {
            order: if word & 0x00400000 != 0 {
                MemoryOrder::AcquireRcsc
            } else {
                MemoryOrder::Release
            },
            bytes: 1u8 << (word >> 30),
            value: reg(rd, false),
            base: reg(rn, true),
        }
    } else if word & 0x1f000000 == 0x10000000 {
        let immediate = (((word >> 5) & 0x7ffff) << 2) | ((word >> 29) & 3);
        let signed = ((immediate as i64) << 43) >> 43;
        let page = word >> 31 != 0;
        let address = if page {
            (pc & !4095).wrapping_add((signed.wrapping_mul(4096)) as u64)
        } else {
            pc.wrapping_add(signed as u64)
        };
        Op::Address {
            dst: reg(rd, false),
            address,
            page,
        }
    } else if word & 0xfc000000 == 0x94000000 {
        Op::Call {
            target: target(pc, word & 0x3ffffff, 26),
        }
    } else if matches!(
        word,
        0xd503233f | 0xd503237f | 0xd50323bf | 0xd50323ff | 0xd50320ff
    ) {
        Op::PointerAuth {
            key: if matches!(word, 0xd503237f | 0xd50323ff) {
                1
            } else {
                0
            },
            operation: if word == 0xd50320ff {
                2
            } else if matches!(word, 0xd50323bf | 0xd50323ff) {
                1
            } else {
                0
            },
        }
    } else if matches!(word, 0xd65f0bff | 0xd65f0fff) {
        Op::AuthReturn {
            key: if word == 0xd65f0fff { 1 } else { 0 },
        }
    } else if word & 0x7fe0ffe0 == 0x2a0003e0 {
        Op::MoveReg {
            width,
            dst: reg(rd, false),
            src: reg((word >> 16) & 31, false),
        }
    } else if word == 0xd503203f {
        Op::YieldHint
    } else if word == 0xd503201f {
        Op::Nop
    } else if word & 0x1f800000 == 0x12800000 {
        let opc = (word >> 29) & 3;
        let shift = ((word >> 21) & 3) * 16;
        if opc == 1 || (width == Width::W32 && shift >= 32) {
            return Err(format!("reserved move-wide at {pc:#x}"));
        }
        Op::MoveWide {
            width,
            dst: reg(rd, false),
            imm: ((word >> 5) & 65535) as u64,
            keep: opc == 3,
            invert: opc == 0,
            shift: shift as u8,
        }
    } else if word & 0x1f800000 == 0x11000000 {
        let flags = word & (1 << 29) != 0;
        Op::AddSubImm {
            width,
            dst: reg(rd, !flags),
            src: reg(rn, true),
            imm: (((word >> 10) & 4095) as u64) << if word & (1 << 22) != 0 { 12 } else { 0 },
            sub: word & (1 << 30) != 0,
            flags,
        }
    } else if word & 0xfc000000 == 0x14000000 {
        Op::Branch {
            target: target(pc, word & 0x3ffffff, 26),
        }
    } else if word & 0x7e000000 == 0x34000000 {
        Op::CompareBranch {
            width,
            src: reg(rd, false),
            target: target(pc, (word >> 5) & 0x7ffff, 19),
            nonzero: word & (1 << 24) != 0,
        }
    } else if word & 0xff000010 == 0x54000000 {
        Op::BranchCond {
            target: target(pc, (word >> 5) & 0x7ffff, 19),
            cond: (word & 15) as u8,
        }
    } else if word & 0xfffffc1f == 0xd65f0000 {
        Op::Ret {
            target: reg(rn, false),
        }
    } else {
        return Err(format!(
            "unsupported instruction {word:08x} ({}) at module VA {pc:#x}",
            classify(word)
        ));
    };
    Ok(Instruction { pc, word, op })
}
pub fn classify(w: u32) -> &'static str {
    match w {
        0xd503233f => return "PACIASP",
        0xd503237f => return "PACIBSP",
        0xd50323bf => return "AUTIASP",
        0xd50323ff => return "AUTIBSP",
        0xd50320ff => return "XPACLRI",
        0xd65f0bff => return "RETAA",
        0xd65f0fff => return "RETAB",
        _ => {}
    }
    if w & 0xfffffc1f == 0xd503241f {
        return "BTI-hint";
    }
    if w & 0xfc000000 == 0x94000000 {
        return "BL-immediate";
    }
    if w & 0xfc000000 == 0x14000000 {
        return "B-immediate";
    }
    if w & 0xfffffc1f == 0xd61f0000 {
        return "BR-register";
    }
    if w & 0xfffffc1f == 0xd63f0000 {
        return "BLR-register";
    }
    if w & 0xfffffc1f == 0xd65f0000 {
        return "RET-register";
    }
    if w & 0xffe0001f == 0xd4000001 {
        return "SVC";
    }
    if w & 0xfff00000 == 0xd5300000 {
        return "MRS";
    }
    if w & 0xfff00000 == 0xd5100000 {
        return "MSR";
    }
    "other-unclassified"
}
/// Reachable CFG for the supported subset. Unknown reachable words and escaping edges
/// are hard errors; no scanning through unsupported code and no interpreter fallback.
pub fn lift(bytes: &[u8], base: u64, entry: u64) -> Result<Vec<Instruction>, String> {
    lift_bounded(bytes, base, entry, false)
}
/// Exact function extents may end in a direct tail branch to a separate registered
/// entry. Unlike strict slice lifting, only explicit B edges may escape the extent.
pub fn lift_function(bytes: &[u8], base: u64, entry: u64) -> Result<Vec<Instruction>, String> {
    lift_bounded(bytes, base, entry, true)
}
fn lift_bounded(
    bytes: &[u8],
    base: u64,
    entry: u64,
    tails: bool,
) -> Result<Vec<Instruction>, String> {
    use std::collections::{BTreeMap, VecDeque};
    if bytes.len() % 4 != 0 || base % 4 != 0 || entry % 4 != 0 {
        return Err("unaligned program".into());
    }
    let end = base
        .checked_add(bytes.len() as u64)
        .ok_or("address overflow")?;
    let mut pending = VecDeque::from([entry]);
    let mut found = BTreeMap::new();
    while let Some(pc) = pending.pop_front() {
        if found.contains_key(&pc) {
            continue;
        }
        if pc < base || pc >= end || pc % 4 != 0 {
            return Err(format!("unresolved CFG edge {pc:#x}"));
        }
        let at = (pc - base) as usize;
        let mut ins = decode(
            u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()),
            pc,
        )?;
        if tails {
            if let Op::Branch { target } = ins.op {
                if target < base || target >= end {
                    ins.op = Op::TailBranch { target };
                }
            }
        }
        match ins.op {
            Op::Ret { .. }
            | Op::AuthReturn { .. }
            | Op::IndirectBranch { .. }
            | Op::TailBranch { .. } => {}
            Op::Branch { target } => pending.push_back(target),
            Op::CompareBranch { target, .. }
            | Op::BranchCond { target, .. }
            | Op::TestBit { target, .. } => {
                pending.push_back(target);
                pending.push_back(pc + 4)
            }
            _ => pending.push_back(pc + 4),
        }
        found.insert(pc, ins);
    }
    Ok(found.into_values().collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pac_never_silent() {
        for w in [0xd503233f, 0xd503237f, 0xd50323bf, 0xd50323ff, 0xd50320ff] {
            assert!(matches!(decode(w, 0).unwrap().op, Op::PointerAuth { .. }));
        }
        assert!(matches!(
            decode(0xd65f0fff, 0).unwrap().op,
            Op::AuthReturn { key: 1 }
        ));
        assert!(decode(0xd503245f, 0).is_err());
    }

    #[test]
    fn backward_branch() {
        assert_eq!(
            decode(0x17ffffff, 0x100).unwrap().op,
            Op::Branch { target: 0xfc }
        )
    }
    #[test]
    fn register31_semantics() {
        assert!(matches!(
            decode(0x910003ff, 0).unwrap().op,
            Op::AddSubImm {
                dst: Reg::Sp,
                src: Reg::Sp,
                ..
            }
        ));
        assert!(matches!(
            decode(0xf10007ff, 0).unwrap().op,
            Op::AddSubImm {
                dst: Reg::Zero,
                src: Reg::Sp,
                ..
            }
        ));
    }
    #[test]
    fn unsupported_and_escape_fail() {
        assert!(lift(&0u32.to_le_bytes(), 0, 0).is_err());
        assert!(lift(&0x14000002u32.to_le_bytes(), 0, 0).is_err())
    }
    #[test]
    fn reserved_mov() {
        assert!(decode(0x52c00000, 0).is_err());
        assert!(decode(0x32800000, 0).is_err())
    }
}

#[cfg(test)]
mod memory_decode_tests {
    use super::*;
    #[test]
    fn widths_and_calls() {
        for (word, bytes) in [
            (0x394003e0u32, 1),
            (0x794003e0, 2),
            (0xb94003e0, 4),
            (0xf94003e0, 8),
        ] {
            assert!(
                matches!(decode(word,0).unwrap().op,Op::Memory(memory::Transfer{load:true,bytes:b,base:Reg::Sp,..})if b==bytes)
            );
        }
        assert!(matches!(
            decode(0x97ffffff, 0x100).unwrap().op,
            Op::Call { target: 0xfc }
        ));
    }
    #[test]
    fn constrained_writeback_rejects_and_vector_pair_decodes() {
        assert!(decode(0xa9c00400, 0).is_err());
        assert!(matches!(decode(0xadc107e0, 0).unwrap().op, Op::Memory(_)));
        assert!(decode(0xf98003e0, 0).is_err());
    }
}

#[cfg(test)]
mod address_tests {
    use super::*;
    #[test]
    fn pc_relative_signed_page_edges() {
        assert_eq!(
            decode(0xb0000000, 0x1ffc).unwrap().op,
            Op::Address {
                dst: Reg::Gpr(0),
                address: 0x2000,
                page: true
            }
        );
        assert_eq!(
            decode(0xf0ffffe0, 0x1004).unwrap().op,
            Op::Address {
                dst: Reg::Gpr(0),
                address: 0,
                page: true
            }
        );
        assert_eq!(
            decode(0x10000020, 0x1ffc).unwrap().op,
            Op::Address {
                dst: Reg::Gpr(0),
                address: 0x2000,
                page: false
            }
        );
    }
}

#[cfg(test)]
mod ordered_tests {
    use super::*;
    #[test]
    fn distinguish_order_and_width() {
        for (base, order) in [
            (0x38bfc000u32, MemoryOrder::AcquireRcpc),
            (0x08dffc00, MemoryOrder::AcquireRcsc),
            (0x089ffc00, MemoryOrder::Release),
        ] {
            for size in 0..4 {
                let w = base | (size << 30) | (31 << 5) | 1;
                assert_eq!(
                    decode(w, 0).unwrap().op,
                    Op::OrderedMemory {
                        order,
                        bytes: 1 << size,
                        value: Reg::Gpr(1),
                        base: Reg::Sp
                    }
                );
            }
        }
    }
    #[test]
    fn neighbouring_atomic_encodings_fail() {
        assert!(decode(0x38bfc508, 0).is_err());
        assert!(decode(0x08807c00, 0).is_err());
        assert!(matches!(
            decode(0x38bfc01f, 0).unwrap().op,
            Op::OrderedMemory {
                value: Reg::Zero,
                ..
            }
        ));
    }
}

#[cfg(test)]
mod branch_arithmetic_tests {
    use super::*;
    #[test]
    fn bit_branch_sign_and_high_bit() {
        assert_eq!(
            decode(0xb6ffffe0, 0x100).unwrap().op,
            Op::TestBit {
                src: Reg::Gpr(0),
                bit: 63,
                target: 0xfc,
                nonzero: false
            }
        );
        assert!(matches!(
            decode(0xd63f03c0, 0).unwrap().op,
            Op::IndirectCall {
                target: Reg::Gpr(30)
            }
        ));
    }
    #[test]
    fn shifted_arithmetic_constraints() {
        assert!(matches!(
            decode(0xeb09015f, 0).unwrap().op,
            Op::AddSubReg {
                dst: Reg::Zero,
                lhs: Reg::Gpr(10),
                rhs: Reg::Gpr(9),
                sub: true,
                flags: true,
                ..
            }
        ));
        assert!(decode(0x0b018000, 0).is_err());
        assert!(decode(0x0bc10000, 0).is_err());
    }
}

#[cfg(test)]
mod select_tests {
    use super::*;
    #[test]
    fn conditional_select_reserved_bit_rejected() {
        assert!(decode(0x3a800000, 0).is_err());
        assert!(matches!(
            decode(0x1a9f17e0, 0).unwrap().op,
            Op::Select {
                lhs: Reg::Zero,
                rhs: Reg::Zero,
                condition: 1,
                operation: 1,
                ..
            }
        ));
    }
}

#[cfg(test)]
mod tail_compare_tests {
    use super::*;
    #[test]
    fn tails_require_explicit_extent_policy_and_do_not_scan_fallthrough() {
        let bytes: Vec<_> = [0x14000002u32]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        assert!(lift(&bytes, 0, 0).is_err());
        assert!(matches!(
            lift_function(&bytes, 0, 0).unwrap()[0].op,
            Op::TailBranch { target: 8 }
        ));
        assert!(lift_function(&0x54000040u32.to_le_bytes(), 0, 0).is_err());
        let bytes: Vec<_> = [0xd61f0120u32, 0xffffffff]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        assert_eq!(lift(&bytes, 0, 0).unwrap().len(), 1);
    }
    #[test]
    fn conditional_compare_reserved_bits_and_register31() {
        assert!(matches!(
            decode(0xfa5f03e5, 0).unwrap().op,
            Op::ConditionalCompare {
                lhs: Reg::Zero,
                rhs: Reg::Zero,
                immediate: None,
                ..
            }
        ));
        for word in [
            0xfa400010,
            0xfa400400,
            0xda400000,
            0xfa400000 | (1 << 23),
            0xd61f0001,
        ] {
            assert!(decode(word, 0).is_err(), "{word:08x}");
        }
    }
}

#[cfg(test)]
mod yield_tests {
    #[test]
    fn yield_is_exact_and_other_wait_hints_still_refuse() {
        assert!(matches!(
            super::decode(0xd503203f, 0).unwrap().op,
            super::Op::YieldHint
        ));
        for word in [0xd503205f, 0xd503207f, 0xd503209f, 0xd50320bf] {
            assert!(super::decode(word, 0).is_err());
        }
    }
}
