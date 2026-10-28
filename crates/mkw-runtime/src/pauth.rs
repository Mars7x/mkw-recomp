//! Explicit diagnostic PAC backend. This does not establish a Switch 2 profile.
use crate::{
    abi::{PAuth, CPU_ABI_VERSION},
    exclusive_native::CpuV10,
};
use libloading::{Library, Symbol};
use std::ffi::c_void;

/// An explicit assumption, not a detected or verified target configuration.
pub enum DiagnosticProfile {
    Legacy39NoTbiQarma5,
}

/// Architectural key halves, IA then IB. No default keys or Debug/serialization.
#[repr(C)]
pub struct Keys {
    high: [u64; 2],
    low: [u64; 2],
}
impl Keys {
    pub fn new(ia_high: u64, ia_low: u64, ib_high: u64, ib_low: u64) -> Self {
        Self {
            high: [ia_high, ib_high],
            low: [ia_low, ib_low],
        }
    }
}

/// Owns immutable process keys and borrows the native helper's library.
/// Key erasure from host memory is not guaranteed.
pub struct Backend<'code> {
    function: Symbol<'code, PAuth>,
    keys: Box<Keys>,
}
impl<'code> Backend<'code> {
    /// # Safety
    /// The library must be trusted and export the four u32 metadata queries from
    /// native/pac_profile.c and mkw_pac39 with native/pac.h's ABI and semantics.
    /// It must never mutate/retain the key context or output pointer, unwind,
    /// or share mutable operation state unsafely. Queries detect accidental
    /// mismatches; they cannot authenticate arbitrary machine code. Selecting
    /// this diagnostic profile does not authorize original game execution.
    pub unsafe fn load(
        library: &'code Library,
        profile: DiagnosticProfile,
        keys: Keys,
    ) -> Result<Self, String> {
        let expected_profile = match profile {
            DiagnosticProfile::Legacy39NoTbiQarma5 => 1,
        };
        for (name, expected) in [
            (b"mkw_pac39_callback_abi\0".as_slice(), CPU_ABI_VERSION),
            (b"mkw_pac39_profile\0".as_slice(), expected_profile),
            (
                b"mkw_pac39_context_size\0".as_slice(),
                std::mem::size_of::<Keys>() as u32,
            ),
            (
                b"mkw_pac39_low_offset\0".as_slice(),
                std::mem::offset_of!(Keys, low) as u32,
            ),
        ] {
            // SAFETY: trusted query signatures are part of the caller's contract.
            let query = unsafe { library.get::<unsafe extern "C" fn() -> u32>(name) }
                .map_err(|_| "missing PAC profile query")?;
            if unsafe { query() } != expected {
                return Err("PAC profile/ABI/layout mismatch".into());
            }
        }
        // SAFETY: caller guarantees the callback ABI and read-only key access.
        let function =
            unsafe { library.get::<PAuth>(b"mkw_pac39\0") }.map_err(|_| "missing PAC callback")?;
        Ok(Self {
            function,
            keys: Box::new(keys),
        })
    }

    pub fn attach<'a>(&'a self, cpu: &'a mut CpuV10) -> Attachment<'a, 'code> {
        let previous = (cpu.base.pauth, cpu.base.pac_context);
        cpu.base.pauth = Some(*self.function);
        cpu.base.pac_context = std::ptr::addr_of!(*self.keys).cast_mut().cast();
        Attachment {
            cpu,
            _backend: self,
            previous,
        }
    }
}

/// Full-frame borrow composes with the coherent memory and guarded router.
pub struct Attachment<'a, 'code> {
    cpu: &'a mut CpuV10,
    _backend: &'a Backend<'code>,
    previous: (Option<PAuth>, *mut c_void),
}
impl Attachment<'_, '_> {
    pub fn cpu_mut(&mut self) -> &mut CpuV10 {
        self.cpu
    }
}
impl Drop for Attachment<'_, '_> {
    fn drop(&mut self) {
        self.cpu.base.pauth = self.previous.0;
        self.cpu.base.pac_context = self.previous.1;
    }
}
