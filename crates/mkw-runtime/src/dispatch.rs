//! Registered native entry dispatch, without interpretation or dynamic translation.
//! Guest executable permissions/module identity must be validated by the future
//! game bootstrap; this API currently serves trusted synthetic native libraries.
use crate::{
    abi::{Call, Cpu, EXIT_TAIL},
    GuestAddr,
};
use libloading::{Library, Symbol};
use std::{collections::BTreeMap, ffi::c_void, marker::PhantomPinned, pin::Pin};
type Function = unsafe extern "C" fn(*mut Cpu, *mut u64) -> i32;
pub struct NativeTarget<'code> {
    address: GuestAddr,
    module_base: u64,
    function: Symbol<'code, Function>,
}
impl<'code> NativeTarget<'code> {
    /// # Safety
    /// The trusted symbol must implement native/cpu.h ABI v7, accept a live Cpu
    /// and shared budget, and never unwind into C or retain CPU/budget/context pointers past the call. No game compatibility is implied.
    /// The borrowed Library keeps the native code alive for all dispatch calls.
    pub unsafe fn load(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
    ) -> Result<Self, String> {
        if address.0 % 4 != 0 || module_base % 4096 != 0 || address.0 < module_base {
            return Err("invalid native target address/base".into());
        }
        // SAFETY: the caller guarantees symbol ABI; Symbol borrows library lifetime.
        let function = unsafe { library.get::<Function>(symbol) }.map_err(|e| e.to_string())?;
        Ok(Self {
            address,
            module_base,
            function,
        })
    }
}
pub struct NativeDispatcher<'code> {
    targets: BTreeMap<GuestAddr, NativeTarget<'code>>,
    _pin: PhantomPinned,
}
impl<'code> NativeDispatcher<'code> {
    pub fn new(targets: Vec<NativeTarget<'code>>) -> Result<Pin<Box<Self>>, String> {
        let mut table = BTreeMap::new();
        for target in targets {
            let address = target.address;
            if table.insert(address, target).is_some() {
                return Err(format!("duplicate native entry {address:?}"));
            }
        }
        Ok(Box::pin(Self {
            targets: table,
            _pin: PhantomPinned,
        }))
    }
    pub fn attach<'a>(self: Pin<&'a Self>, cpu: &'a mut Cpu) -> Attachment<'a, 'code> {
        let previous = (cpu.call, cpu.call_context);
        cpu.call = Some(call);
        cpu.call_context = self.get_ref() as *const Self as *mut c_void;
        Attachment {
            cpu,
            _dispatcher: self,
            previous,
        }
    }
}
pub struct Attachment<'a, 'code> {
    cpu: &'a mut Cpu,
    _dispatcher: Pin<&'a NativeDispatcher<'code>>,
    previous: (Option<Call>, *mut c_void),
}
impl Attachment<'_, '_> {
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        self.cpu
    }
}
impl Drop for Attachment<'_, '_> {
    fn drop(&mut self) {
        self.cpu.call = self.previous.0;
        self.cpu.call_context = self.previous.1;
    }
}
/// # Safety
/// Pointers must come from the generated C ABI and an active dispatcher Attachment.
unsafe extern "C" fn call(
    context: *mut c_void,
    cpu: *mut Cpu,
    target: u64,
    budget: *mut u64,
) -> i32 {
    if context.is_null()
        || cpu.is_null()
        || budget.is_null()
        || context as usize % std::mem::align_of::<NativeDispatcher<'_>>() != 0
        || cpu as usize % std::mem::align_of::<Cpu>() != 0
        || budget as usize % std::mem::align_of::<u64>() != 0
    {
        return 7;
    }
    // SAFETY: Attachment pins and borrows the dispatcher. The live C call frame
    // supplies exclusive mutable CPU/budget pointers and borrowed native images.
    let (dispatcher, state) = unsafe { (&*context.cast::<NativeDispatcher<'_>>(), &mut *cpu) };
    let saved_base = state.module_base;
    let mut next = target;
    let status = loop {
        state.fault_address = next;
        let Some(entry) = dispatcher.targets.get(&GuestAddr(next)) else {
            state.fault_status = 3;
            break 3;
        };
        state.module_base = entry.module_base;
        // SAFETY: the generated ABI supplies a live shared budget.
        let remaining = unsafe { *budget };
        // SAFETY: NativeTarget establishes the ABI and borrows its Library.
        let status = unsafe { (entry.function)(state, budget) };
        if status != EXIT_TAIL {
            break status;
        }
        // SAFETY: the budget pointer remains live throughout this callback.
        if unsafe { *budget } >= remaining {
            state.fault_status = 6;
            break 6;
        }
        next = state.pc;
        // A generated BR consumes a budget unit before requesting tail routing.
        // Refuse exhausted trusted callbacks too, preventing zero-budget cycles.
        // SAFETY: same live budget contract.
        if unsafe { *budget } == 0 {
            break 2;
        }
    };
    state.module_base = saved_base;
    status
}
