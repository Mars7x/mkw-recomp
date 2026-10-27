//! LSE operations in the serialized normal-memory diagnostic profile. All plain,
//! ordered and atomic accesses share one mutex; weak-memory equivalence is absent.
use crate::{bridge::MemoryBridge, GuestAddr};
use std::{ffi::c_void, sync::atomic::Ordering};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum AtomicOrder {
    Relaxed = 0,
    Acquire = 1,
    Release = 2,
    AcquireRelease = 3,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
impl MemoryBridge {
    pub fn atomic_counts(&self) -> [u64; 4] {
        std::array::from_fn(|i| self.atomic_counts[i].load(Ordering::Relaxed))
    }
    pub fn atomic_access(
        &self,
        order: AtomicOrder,
        operation: AtomicOp,
        address: GuestAddr,
        input: u64,
        replacement: u64,
        bytes: usize,
    ) -> Result<u64, String> {
        let result = (|| {
            if !matches!(bytes, 1 | 2 | 4 | 8) || address.0 % (bytes as u64) != 0 {
                return Err("unaligned/unsupported atomic access in diagnostic profile".into());
            }
            let mut memory = self
                .memory
                .lock()
                .map_err(|_| "memory backend lock poisoned")?;
            memory.spans(address, bytes, false)?;
            memory.spans(address, bytes, true)?;
            let mut data = [0u8; 8];
            memory.read_into(address, &mut data[..bytes])?;
            let old = u64::from_le_bytes(data);
            let mask = if bytes == 8 {
                u64::MAX
            } else {
                (1u64 << (bytes * 8)) - 1
            };
            let input = input & mask;
            let sign = 1u64 << (bytes * 8 - 1);
            let next = match operation {
                AtomicOp::Add => old.wrapping_add(input),
                AtomicOp::Clear => old & !input,
                AtomicOp::Xor => old ^ input,
                AtomicOp::Set => old | input,
                AtomicOp::SignedMax => {
                    if old ^ sign > input ^ sign {
                        old
                    } else {
                        input
                    }
                }
                AtomicOp::SignedMin => {
                    if old ^ sign < input ^ sign {
                        old
                    } else {
                        input
                    }
                }
                AtomicOp::UnsignedMax => old.max(input),
                AtomicOp::UnsignedMin => old.min(input),
                AtomicOp::Swap => input,
                AtomicOp::CompareSwap => {
                    if old == input {
                        replacement
                    } else {
                        old
                    }
                }
            } & mask;
            if operation != AtomicOp::CompareSwap || old == input {
                memory.write_bytes(address, &next.to_le_bytes()[..bytes])?;
            }
            self.atomic_counts[order as usize].fetch_add(1, Ordering::Relaxed);
            Ok(old)
        })();
        if let Err(ref error) = result {
            if let Ok(mut slot) = self.last_fault.lock() {
                *slot = Some(format!("{address:?}: {error}"));
            }
        }
        result
    }
}
/// # Safety
/// Context is an attached pinned bridge. output is one exclusively borrowed,
/// aligned writable u64. No guest address is interpreted as a host pointer.
pub(crate) unsafe extern "C" fn callback(
    context: *mut c_void,
    order: u32,
    operation: u32,
    address: u64,
    input: u64,
    replacement: u64,
    output: *mut u64,
    bytes: u32,
) -> i32 {
    if context.is_null()
        || output.is_null()
        || context as usize % std::mem::align_of::<MemoryBridge>() != 0
        || output as usize % std::mem::align_of::<u64>() != 0
    {
        return 7;
    }
    let order = match order {
        0 => AtomicOrder::Relaxed,
        1 => AtomicOrder::Acquire,
        2 => AtomicOrder::Release,
        3 => AtomicOrder::AcquireRelease,
        _ => return 3,
    };
    let operation = match operation {
        0 => AtomicOp::Add,
        1 => AtomicOp::Clear,
        2 => AtomicOp::Xor,
        3 => AtomicOp::Set,
        4 => AtomicOp::SignedMax,
        5 => AtomicOp::SignedMin,
        6 => AtomicOp::UnsignedMax,
        7 => AtomicOp::UnsignedMin,
        8 => AtomicOp::Swap,
        9 => AtomicOp::CompareSwap,
        _ => return 3,
    };
    // SAFETY: the attachment pins/borrows the context; the generated C caller
    // supplies a live local result buffer for this call only.
    let bridge = unsafe { &*context.cast::<MemoryBridge>() };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bridge.atomic_access(
            order,
            operation,
            GuestAddr(address),
            input,
            replacement,
            bytes as usize,
        )
    })) {
        Ok(Ok(old)) => {
            unsafe {
                *output = old;
            }
            0
        }
        Ok(Err(_)) => 5,
        Err(_) => 7,
    }
}
