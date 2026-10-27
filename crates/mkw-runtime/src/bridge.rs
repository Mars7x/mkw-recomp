//! Serialized diagnostic memory backend. All callbacks share one lock, including
//! ordinary accesses. Stronger than RCpc; no claim of weak-memory equivalence.
use crate::{
    abi::{AtomicCallback, Cpu, MemoryCallback, OrderedCallback},
    GuestAddr, Memory,
};
use std::{
    ffi::c_void,
    marker::PhantomPinned,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MemoryOrder {
    Plain = 0,
    AcquireRcpc = 1,
    AcquireRcsc = 2,
    Release = 3,
}
pub struct MemoryBridge {
    pub(crate) memory: Mutex<Memory>,
    pub(crate) last_fault: Mutex<Option<String>>,
    pub(crate) atomic_counts: [AtomicU64; 4],
    counts: [AtomicU64; 4],
    _pin: PhantomPinned,
}
impl MemoryBridge {
    pub fn new(memory: Memory) -> Pin<Box<Self>> {
        Box::pin(Self {
            memory: Mutex::new(memory),
            last_fault: Mutex::new(None),
            atomic_counts: std::array::from_fn(|_| AtomicU64::new(0)),
            counts: std::array::from_fn(|_| AtomicU64::new(0)),
            _pin: PhantomPinned,
        })
    }
    pub fn counts(&self) -> [u64; 4] {
        std::array::from_fn(|i| self.counts[i].load(Ordering::Relaxed))
    }
    pub fn last_fault(&self) -> Option<String> {
        self.last_fault.lock().ok().and_then(|v| v.clone())
    }
    pub fn access(
        &self,
        order: MemoryOrder,
        store: bool,
        address: GuestAddr,
        words: &mut [u64],
        bytes: usize,
    ) -> Result<(), String> {
        let result = (|| {
            if !matches!(bytes, 1 | 2 | 4 | 8 | 16 | 32) || words.len() < bytes.div_ceil(8) {
                return Err("unsupported memory width".into());
            }
            if order != MemoryOrder::Plain {
                if bytes > 8 || (order == MemoryOrder::Release) != store {
                    return Err("invalid ordered memory operation".into());
                }
                if address.0 % (bytes as u64) != 0 {
                    return Err("unaligned ordered access unsupported by diagnostic profile".into());
                }
            }
            let mut memory = self
                .memory
                .lock()
                .map_err(|_| "memory backend lock poisoned")?;
            let mut data = [0u8; 32];
            if store {
                for (i, byte) in data[..bytes].iter_mut().enumerate() {
                    *byte = (words[i / 8] >> ((i % 8) * 8)) as u8;
                }
                memory.write_bytes(address, &data[..bytes])?;
            } else {
                memory.read_into(address, &mut data[..bytes])?;
                let mut result = [0u64; 4];
                for (i, &byte) in data[..bytes].iter().enumerate() {
                    result[i / 8] |= u64::from(byte) << ((i % 8) * 8);
                }
                words[..bytes.div_ceil(8)].copy_from_slice(&result[..bytes.div_ceil(8)]);
            }
            self.counts[order as usize].fetch_add(1, Ordering::Relaxed);
            Ok(())
        })();
        if let Err(ref error) = result {
            if let Ok(mut slot) = self.last_fault.lock() {
                *slot = Some(format!("{address:?}: {error}"));
            }
        }
        result
    }
    pub fn attach<'a>(self: Pin<&'a Self>, cpu: &'a mut Cpu) -> Attachment<'a> {
        let previous = (
            cpu.memory,
            cpu.ordered_memory,
            cpu.memory_context,
            cpu.atomic_memory,
        );
        cpu.memory = Some(memory_callback);
        cpu.atomic_memory = Some(crate::atomics::callback);
        cpu.ordered_memory = Some(ordered_callback);
        cpu.memory_context = self.get_ref() as *const Self as *mut c_void;
        Attachment {
            cpu,
            _bridge: self,
            previous,
        }
    }
}
pub struct Attachment<'a> {
    cpu: &'a mut Cpu,
    _bridge: Pin<&'a MemoryBridge>,
    previous: (
        Option<MemoryCallback>,
        Option<OrderedCallback>,
        *mut c_void,
        Option<AtomicCallback>,
    ),
}
impl Attachment<'_> {
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        self.cpu
    }
}
impl Drop for Attachment<'_> {
    fn drop(&mut self) {
        self.cpu.memory = self.previous.0;
        self.cpu.ordered_memory = self.previous.1;
        self.cpu.memory_context = self.previous.2;
        self.cpu.atomic_memory = self.previous.3;
    }
}
/// # Safety
/// context must be a live pinned MemoryBridge installed by Attachment. words
/// must reference ceil(bytes/8) writable, aligned u64s, exclusively borrowed for this call.
unsafe extern "C" fn memory_callback(
    context: *mut c_void,
    store: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    // SAFETY: forwarded unchanged under this function's documented ABI contract.
    unsafe { dispatch(context, 0, store, address, words, bytes) }
}
/// # Safety
/// Same contract as memory_callback. Only explicit RCpc/RCsc acquire or release
/// operations are accepted; ordinary accesses use the separate callback.
unsafe extern "C" fn ordered_callback(
    context: *mut c_void,
    order: u32,
    store: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    if order == 0 {
        return 3;
    }
    // SAFETY: forwarded unchanged under this function's documented ABI contract.
    unsafe { dispatch(context, order, store, address, words, bytes) }
}
unsafe fn dispatch(
    context: *mut c_void,
    order: u32,
    store: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    if context.is_null()
        || words.is_null()
        || context as usize % std::mem::align_of::<MemoryBridge>() != 0
        || words as usize % std::mem::align_of::<u64>() != 0
    {
        return 7;
    }
    if !matches!(store, 0 | 1) {
        return 3;
    }
    let order = match order {
        0 => MemoryOrder::Plain,
        1 => MemoryOrder::AcquireRcpc,
        2 => MemoryOrder::AcquireRcsc,
        3 => MemoryOrder::Release,
        _ => return 3,
    };
    if !matches!(bytes, 1 | 2 | 4 | 8 | 16 | 32) {
        return 3;
    }
    // SAFETY: Attachment keeps the pinned bridge alive. The generated ABI always
    // supplies a uniquely borrowed transfer buffer; no guest pointer is cast.
    let (bridge, words) = unsafe {
        (
            &*context.cast::<MemoryBridge>(),
            std::slice::from_raw_parts_mut(words, (bytes as usize).div_ceil(8)),
        )
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bridge.access(order, store != 0, GuestAddr(address), words, bytes as usize)
    })) {
        Ok(Ok(())) => 0,
        Ok(Err(_)) => 5,
        Err(_) => 7,
    }
}
