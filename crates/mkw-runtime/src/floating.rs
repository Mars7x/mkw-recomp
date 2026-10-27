//! Borrowed, explicitly attached software FP backend. No interpreter/JIT fallback.
use crate::abi::{Cpu, FloatingCallback, CPU_ABI_VERSION};
use libloading::{Library, Symbol};
pub struct FloatingBackend<'code> {
    function: Symbol<'code, FloatingCallback>,
}
impl<'code> FloatingBackend<'code> {
    /// # Safety
    /// The trusted library must export mkw_fp with native/cpu.h's callback ABI
    /// and mkw_fp_abi_version returning u32. It must not retain output pointers,
    /// unwind across C, or share mutable operation state unsafely across threads.
    pub unsafe fn load(library: &'code Library) -> Result<Self, String> {
        // SAFETY: caller establishes the native symbols' signatures and behavior.
        let version =
            unsafe { library.get::<unsafe extern "C" fn() -> u32>(b"mkw_fp_abi_version\0") }
                .map_err(|e| e.to_string())?;
        if unsafe { version() } != CPU_ABI_VERSION {
            return Err("FP backend ABI version mismatch".into());
        }
        let function =
            unsafe { library.get::<FloatingCallback>(b"mkw_fp\0") }.map_err(|e| e.to_string())?;
        Ok(Self { function })
    }
    pub fn attach<'a>(&'a self, cpu: &'a mut Cpu) -> Attachment<'a, 'code> {
        let previous = cpu.floating;
        cpu.floating = Some(*self.function);
        Attachment {
            cpu,
            _backend: self,
            previous,
        }
    }
}
pub struct Attachment<'a, 'code> {
    cpu: &'a mut Cpu,
    _backend: &'a FloatingBackend<'code>,
    previous: Option<FloatingCallback>,
}
impl Attachment<'_, '_> {
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        self.cpu
    }
}
impl Drop for Attachment<'_, '_> {
    fn drop(&mut self) {
        self.cpu.floating = self.previous;
    }
}
