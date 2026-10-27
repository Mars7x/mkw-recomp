//! LSE scalar read-modify-write and compare-and-swap. Exclusive/pair forms differ.
use super::{reg, Op, Reg};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u32)]
pub enum AtomicOrder {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcquireRelease = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[repr(u32)]
pub enum AtomicOp {
    Add = 0,
    Clear = 1,
    Xor = 2,
    Set = 3,
    SignedMax = 4,
    SignedMin = 5,
    UnsignedMax = 6,
    UnsignedMin = 7,
    Swap = 8,
    CompareSwap = 9,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Atomic {
    pub bytes: u8,
    pub order: AtomicOrder,
    pub operation: AtomicOp,
    pub base: Reg,
    pub input: Reg,
    pub replacement: Reg,
    pub result: Reg,
}
fn order(acquire: bool, release: bool) -> AtomicOrder {
    match (acquire, release) {
        (false, false) => AtomicOrder::Relaxed,
        (true, false) => AtomicOrder::Acquire,
        (false, true) => AtomicOrder::Release,
        (true, true) => AtomicOrder::AcquireRelease,
    }
}
pub fn decode(w: u32) -> Option<Op> {
    let rs = (w >> 16) & 31;
    let rt = w & 31;
    let base = reg((w >> 5) & 31, true);
    let bytes = 1 << (w >> 30);
    if w & 0x3fa07c00 == 0x08a07c00 {
        return Some(Op::Atomic(Atomic {
            bytes,
            order: order(w & (1 << 22) != 0, w & (1 << 15) != 0),
            operation: AtomicOp::CompareSwap,
            base,
            input: reg(rs, false),
            replacement: reg(rt, false),
            result: reg(rs, false),
        }));
    }
    if w & 0x3f200c00 != 0x38200000 {
        return None;
    }
    let operation = match (w >> 12) & 15 {
        0 => AtomicOp::Add,
        1 => AtomicOp::Clear,
        2 => AtomicOp::Xor,
        3 => AtomicOp::Set,
        4 => AtomicOp::SignedMax,
        5 => AtomicOp::SignedMin,
        6 => AtomicOp::UnsignedMax,
        7 => AtomicOp::UnsignedMin,
        8 => AtomicOp::Swap,
        _ => return None,
    };
    Some(Op::Atomic(Atomic {
        bytes,
        order: order(w & (1 << 23) != 0 && rt != 31, w & (1 << 22) != 0),
        operation,
        base,
        input: reg(rs, false),
        replacement: Reg::Zero,
        result: reg(rt, false),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordered_tags_and_neighbouring_forms_stay_distinct() {
        for (word, operation, expected) in [
            (0xb8e10040, AtomicOp::Add, AtomicOrder::AcquireRelease),
            (0xb8e1005f, AtomicOp::Add, AtomicOrder::Release),
            (
                0xc8e0fc41,
                AtomicOp::CompareSwap,
                AtomicOrder::AcquireRelease,
            ),
        ] {
            assert!(
                matches!(crate::decode(word,0).unwrap().op,Op::Atomic(Atomic{operation:op,order,..})if op==operation&&order==expected)
            );
        }
        assert!(matches!(
            crate::decode(0x38bfc020, 0).unwrap().op,
            Op::OrderedMemory { .. }
        ));
        for word in [0x38219040, 0x38210440, 0x08207c00, 0xc85f7c00] {
            assert!(crate::decode(word, 0).is_err(), "{word:08x}");
        }
    }
}
