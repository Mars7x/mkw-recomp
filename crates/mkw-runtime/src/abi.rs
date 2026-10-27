//! ABI v7 mirrors native/cpu.h; integration tests compare its size and offsets
//! against the C compiler before invoking generated native functions.
use std::ffi::c_void;
pub const CPU_ABI_VERSION: u32 = 7;
/// Internal dispatcher request, never a completed guest return.
pub const EXIT_TAIL: i32 = 8;
pub type Call = unsafe extern "C" fn(*mut c_void, *mut Cpu, u64, *mut u64) -> i32;
pub type PAuth = unsafe extern "C" fn(*mut c_void, u32, u32, u64, u64, *mut u64) -> i32;
pub type MemoryCallback = unsafe extern "C" fn(*mut c_void, i32, u64, *mut u64, u32) -> i32;
pub type OrderedCallback = unsafe extern "C" fn(*mut c_void, u32, i32, u64, *mut u64, u32) -> i32;
pub type AtomicCallback =
    unsafe extern "C" fn(*mut c_void, u32, u32, u64, u64, u64, *mut u64, u32) -> i32;
pub type BarrierCallback = unsafe extern "C" fn(*mut c_void, u32, u32) -> i32;
pub type FloatingCallback =
    unsafe extern "C" fn(u32, u32, u32, u32, u64, u64, u64, *mut u64, *mut u32) -> i32;
pub type ConvertCallback =
    unsafe extern "C" fn(u32, u32, u32, u32, u32, u64, *mut u64, *mut u32) -> i32;
#[repr(C)]
#[derive(Default)]
pub struct Cpu {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub module_base: u64,
    pub n: u32,
    pub z: u32,
    pub c: u32,
    pub v: u32,
    pub pauth: Option<PAuth>,
    pub pac_context: *mut c_void,
    pub memory: Option<MemoryCallback>,
    pub memory_context: *mut c_void,
    pub fault_address: u64,
    pub fault_status: i32,
    pub call: Option<Call>,
    pub call_context: *mut c_void,
    pub ordered_memory: Option<OrderedCallback>,
    pub vregs: [[u64; 2]; 32],
    pub atomic_memory: Option<AtomicCallback>,
    pub barrier: Option<BarrierCallback>,
    pub fpcr: u32,
    pub fpsr: u32,
    pub floating: Option<FloatingCallback>,
    pub convert: Option<ConvertCallback>,
    pub tpidr_el0: u64,
    pub tpidrro_el0: u64,
    pub thread_state_configured: u32,
}

impl Cpu {
    /// Configure raw guest software thread-register values. These are not host
    /// pointers and do not allocate/initialize TLS or validate game startup.
    pub fn configure_thread_registers(&mut self, tpidr_el0: u64, tpidrro_el0: u64) {
        self.tpidr_el0 = tpidr_el0;
        self.tpidrro_el0 = tpidrro_el0;
        self.thread_state_configured = 1;
    }
}
