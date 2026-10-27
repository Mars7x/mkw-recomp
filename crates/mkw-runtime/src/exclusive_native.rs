//! Explicit v10 frame/v1 exclusive callback contract; no default ISA/ABI9 change.
//! Plain, ordered, LSE, barrier and exclusive operations share one authority/lock.
//! Cross-module call routing and LL/SC instruction lowering remain gated.
use crate::{
    abi::{AtomicCallback, BarrierCallback, Cpu, MemoryCallback, OrderedCallback},
    exclusive::{Error, ExclusiveMemory, Owner, Width},
    GuestAddr,
};
use std::{
    ffi::c_void,
    mem::size_of,
    sync::{Arc, Mutex},
};
pub const CPU_V10_ABI_VERSION: u32 = 10;
pub const EXCLUSIVE_V1_VERSION: u32 = 1;
pub type Load = unsafe extern "C" fn(*mut c_void, u64, u32, u32, *mut u64, u32) -> i32;
pub type Store = unsafe extern "C" fn(*mut c_void, u64, u32, u32, *const u64, u32, *mut u32) -> i32;
pub type Clear = unsafe extern "C" fn(*mut c_void) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ops {
    pub version: u32,
    pub size: u32,
    pub load: Option<Load>,
    pub store: Option<Store>,
    pub clear: Option<Clear>,
    pub memory: Option<MemoryCallback>,
    pub ordered_memory: Option<OrderedCallback>,
    pub atomic_memory: Option<AtomicCallback>,
    pub barrier: Option<BarrierCallback>,
}
#[repr(C)]
#[derive(Default)]
pub struct CpuV10 {
    pub base: Cpu,
    pub exclusive: *const Ops,
    pub exclusive_context: *mut c_void,
}
struct Context {
    memory: Arc<Mutex<ExclusiveMemory>>,
    owner: Owner,
}
pub struct Bridge {
    context: Box<Context>,
}
impl Bridge {
    pub fn new(memory: Arc<Mutex<ExclusiveMemory>>) -> Result<Self, String> {
        let owner = memory
            .lock()
            .map_err(|_| "exclusive memory lock poisoned")?
            .new_owner()
            .map_err(|e| format!("exclusive owner: {e:?}"))?;
        Ok(Self {
            context: Box::new(Context { memory, owner }),
        })
    }
    pub fn attach<'a>(&'a mut self, cpu: &'a mut CpuV10) -> Result<Attachment<'a>, String> {
        if !cpu.exclusive.is_null() || !cpu.exclusive_context.is_null() {
            return Err("exclusive frame already attached".into());
        }
        if cpu.base.memory.is_some()
            || cpu.base.ordered_memory.is_some()
            || cpu.base.atomic_memory.is_some()
            || cpu.base.barrier.is_some()
            || !cpu.base.memory_context.is_null()
            || cpu.base.call.is_some()
            || !cpu.base.call_context.is_null()
        {
            return Err(
                "pre-existing memory/call routes cannot be mixed with the exclusive authority"
                    .into(),
            );
        }
        cpu.exclusive = &OPS;
        cpu.exclusive_context = std::ptr::addr_of_mut!(*self.context).cast();
        cpu.base.memory = OPS.memory;
        cpu.base.ordered_memory = OPS.ordered_memory;
        cpu.base.atomic_memory = OPS.atomic_memory;
        cpu.base.barrier = OPS.barrier;
        cpu.base.memory_context = cpu.exclusive_context;
        Ok(Attachment { cpu, _bridge: self })
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        if let Ok(mut memory) = self.context.memory.lock() {
            let _ = memory.release(&self.context.owner);
        }
    }
}
pub struct Attachment<'a> {
    cpu: &'a mut CpuV10,
    _bridge: &'a mut Bridge,
}
impl Attachment<'_> {
    pub fn cpu_mut(&mut self) -> &mut CpuV10 {
        self.cpu
    }
}
impl Drop for Attachment<'_> {
    fn drop(&mut self) {
        self.cpu.base.memory = None;
        self.cpu.base.ordered_memory = None;
        self.cpu.base.atomic_memory = None;
        self.cpu.base.barrier = None;
        self.cpu.base.memory_context = std::ptr::null_mut();
        self.cpu.exclusive = std::ptr::null();
        self.cpu.exclusive_context = std::ptr::null_mut();
    }
}
fn fault(error: Error) -> i32 {
    match error {
        Error::Alignment | Error::Address | Error::Memory(_) => 1,
        _ => 3,
    }
}
unsafe fn context<'a>(pointer: *mut c_void) -> Result<&'a Context, i32> {
    let p = pointer.cast::<Context>();
    if p.is_null() || !p.is_aligned() {
        return Err(3);
    } /* SAFETY: the caller must retain the owned attachment for this context. */
    Ok(unsafe { &*p })
}
unsafe extern "C" fn load(
    context_pointer: *mut c_void,
    address: u64,
    bytes: u32,
    order: u32,
    output: *mut u64,
    words: u32,
) -> i32 {
    if words != 2 || output.is_null() || !output.is_aligned() || order > 1 {
        return 3;
    }
    let Ok(width) = Width::try_from(bytes as usize) else {
        return 3;
    };
    let Ok(ctx) = (unsafe { context(context_pointer) }) else {
        return 3;
    };
    let Ok(mut memory) = ctx.memory.lock() else {
        return 3;
    };
    match memory.load(&ctx.owner, GuestAddr(address), width) {
        Ok(value) => {
            /* SAFETY: two aligned output words are part of the checked native contract. */
            unsafe {
                output.write(value as u64);
                output.add(1).write((value >> 64) as u64);
            }
            0
        }
        Err(error) => fault(error),
    }
}
unsafe extern "C" fn store(
    context_pointer: *mut c_void,
    address: u64,
    bytes: u32,
    order: u32,
    input: *const u64,
    words: u32,
    stored: *mut u32,
) -> i32 {
    if words != 2
        || input.is_null()
        || !input.is_aligned()
        || stored.is_null()
        || !stored.is_aligned()
        || order > 1
    {
        return 3;
    }
    let Ok(width) = Width::try_from(bytes as usize) else {
        return 3;
    };
    let Ok(ctx) = (unsafe { context(context_pointer) }) else {
        return 3;
    };
    /* SAFETY: two aligned input words are required by the native contract. */
    let value = unsafe { u128::from(input.read()) | (u128::from(input.add(1).read()) << 64) };
    let Ok(mut memory) = ctx.memory.lock() else {
        return 3;
    };
    match memory.store_conditional(&ctx.owner, GuestAddr(address), width, value) {
        Ok(success) => {
            /* SAFETY: the checked aligned output status is valid for one u32. */
            unsafe {
                stored.write(u32::from(success));
            }
            0
        }
        Err(error) => fault(error),
    }
}
unsafe extern "C" fn clear(context_pointer: *mut c_void) -> i32 {
    let Ok(ctx) = (unsafe { context(context_pointer) }) else {
        return 3;
    };
    let Ok(mut memory) = ctx.memory.lock() else {
        return 3;
    };
    match memory.clear(&ctx.owner) {
        Ok(()) => 0,
        Err(error) => fault(error),
    }
}
static OPS: Ops = Ops {
    version: EXCLUSIVE_V1_VERSION,
    size: size_of::<Ops>() as u32,
    load: Some(load),
    store: Some(store),
    clear: Some(clear),
    memory: Some(memory_callback),
    ordered_memory: Some(ordered_callback),
    atomic_memory: Some(atomic_callback),
    barrier: Some(barrier_callback),
};

// Prefix callbacks preserve the existing ABI9 signatures, width sets and status
// conventions; their implementation uses the same owned authority as exclusives.
unsafe extern "C" fn memory_callback(
    ctx: *mut c_void,
    write: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    // SAFETY: caller supplies ceil(bytes/8) aligned words under the existing ABI.
    unsafe { memory_dispatch(ctx, 0, write, address, words, bytes) }
}
unsafe extern "C" fn ordered_callback(
    ctx: *mut c_void,
    order: u32,
    write: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    if order == 0 {
        return 3;
    }
    // SAFETY: identical validated prefix transfer-buffer contract.
    unsafe { memory_dispatch(ctx, order, write, address, words, bytes) }
}
unsafe fn memory_dispatch(
    ctx: *mut c_void,
    order: u32,
    write: i32,
    address: u64,
    words: *mut u64,
    bytes: u32,
) -> i32 {
    if ctx.is_null()
        || !ctx.cast::<Context>().is_aligned()
        || words.is_null()
        || !words.is_aligned()
    {
        return 7;
    }
    if !matches!(write, 0 | 1)
        || order > 3
        || !matches!(bytes, 1 | 2 | 3 | 4 | 6 | 8 | 12 | 16 | 24 | 32 | 48 | 64)
    {
        return 3;
    }
    if order != 0 {
        if !matches!(bytes, 1 | 2 | 4 | 8) {
            return 3;
        }
        if (order == 3) != (write == 1) || address % u64::from(bytes) != 0 {
            return 5;
        }
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: attachment owns context and caller supplies the declared words.
        let ctx = unsafe { context(ctx) }.map_err(|_| 7)?;
        let mut memory = ctx.memory.lock().map_err(|_| 5)?;
        memory.validate_owner(&ctx.owner).map_err(|_| 3)?;
        let count = (bytes as usize).div_ceil(8);
        let mut data = [0u8; 64];
        if write == 1 {
            for (i, chunk) in data[..bytes as usize].chunks_mut(8).enumerate() {
                let value = unsafe { words.add(i).read() }.to_le_bytes();
                chunk.copy_from_slice(&value[..chunk.len()]);
            }
            memory
                .write(GuestAddr(address), &data[..bytes as usize])
                .map_err(|_| 5)?;
        } else {
            memory
                .read_into(GuestAddr(address), &mut data[..bytes as usize])
                .map_err(|_| 5)?;
            let result: [u64; 8] = std::array::from_fn(|i| {
                u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap())
            });
            for (i, &value) in result[..count].iter().enumerate() {
                unsafe {
                    words.add(i).write(value);
                }
            }
        }
        Ok::<(), i32>(())
    }))
    .map_or(7, |result| result.map_or_else(|status| status, |()| 0))
}
unsafe extern "C" fn atomic_callback(
    ctx: *mut c_void,
    order: u32,
    operation: u32,
    address: u64,
    input: u64,
    replacement: u64,
    output: *mut u64,
    bytes: u32,
) -> i32 {
    if ctx.is_null()
        || !ctx.cast::<Context>().is_aligned()
        || output.is_null()
        || !output.is_aligned()
    {
        return 7;
    }
    if order > 3 || operation > 9 {
        return 3;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if !matches!(bytes, 1 | 2 | 4 | 8) || address % u64::from(bytes) != 0 {
            return Err(5);
        }
        // SAFETY: context/output validity is the attached prefix ABI contract.
        let ctx = unsafe { context(ctx) }.map_err(|_| 7)?;
        let mut memory = ctx.memory.lock().map_err(|_| 5)?;
        memory.validate_owner(&ctx.owner).map_err(|_| 3)?;
        // Match legacy permissions even for a compare mismatch which performs no store.
        memory
            .validate_access(GuestAddr(address), bytes as usize, false)
            .map_err(|_| 5)?;
        memory
            .validate_access(GuestAddr(address), bytes as usize, true)
            .map_err(|_| 5)?;
        let width = Width::try_from(bytes as usize).map_err(|_| 5)?;
        let old = memory.read(GuestAddr(address), width).map_err(|_| 5)? as u64;
        let mask = if bytes == 8 {
            u64::MAX
        } else {
            (1u64 << (bytes * 8)) - 1
        };
        let input = input & mask;
        let sign = 1u64 << (bytes * 8 - 1);
        let next = match operation {
            0 => old.wrapping_add(input),
            1 => old & !input,
            2 => old ^ input,
            3 => old | input,
            4 => {
                if old ^ sign > input ^ sign {
                    old
                } else {
                    input
                }
            }
            5 => {
                if old ^ sign < input ^ sign {
                    old
                } else {
                    input
                }
            }
            6 => old.max(input),
            7 => old.min(input),
            8 => input,
            9 => {
                if old == input {
                    replacement
                } else {
                    old
                }
            }
            _ => unreachable!(),
        } & mask;
        if operation != 9 || old == input {
            memory
                .write(GuestAddr(address), &next.to_le_bytes()[..bytes as usize])
                .map_err(|_| 5)?;
        }
        unsafe {
            output.write(old);
        }
        Ok::<(), i32>(())
    }))
    .map_or(7, |result| result.map_or_else(|status| status, |()| 0))
}
unsafe extern "C" fn barrier_callback(ctx: *mut c_void, kind: u32, option: u32) -> i32 {
    if ctx.is_null() || !ctx.cast::<Context>().is_aligned() {
        return 7;
    }
    if kind > 1 || option > 15 || option & 3 == 0 {
        return 3;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: the active attachment retains this context.
        let ctx = unsafe { context(ctx) }.map_err(|_| 7)?;
        let memory = ctx.memory.lock().map_err(|_| 3)?;
        memory.validate_owner(&ctx.owner).map_err(|_| 3)?;
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
        Ok::<(), i32>(())
    }))
    .map_or(7, |result| result.map_or_else(|status| status, |()| 0))
}
