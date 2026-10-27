//! Checked entry execution for trusted synthetic v10 libraries.
//! Original guest identity, execute permissions and target configuration remain
//! future-bootstrap requirements; this API does not establish those facts.
use crate::{
    exclusive_native::{Attachment, CpuV10, CPU_V10_ABI_VERSION},
    GuestAddr,
};
use libloading::{Library, Symbol};
type Function = unsafe extern "C" fn(*mut CpuV10, *mut u64) -> i32;
pub struct Entry<'code> {
    pub(crate) function: Symbol<'code, Function>,
    pub(crate) address: GuestAddr,
    pub(crate) module_base: u64,
}
impl<'code> Entry<'code> {
    /// # Safety
    /// The library must be trusted and its advertised v10 entry must implement
    /// exclusive_v10.h, use the shared budget, never unwind or retain pointers,
    /// and preserve the installed callback/table/context identities. Only
    /// synthetic fixtures are currently authorized by the project bootstrap.
    pub unsafe fn load(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
    ) -> Result<Self, String> {
        unsafe { Self::load_contract(library, symbol, address, module_base, None) }
    }
    pub(crate) unsafe fn load_table(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
        table_version: u32,
    ) -> Result<Self, String> {
        unsafe { Self::load_contract(library, symbol, address, module_base, Some(table_version)) }
    }
    unsafe fn load_contract(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
        table_version: Option<u32>,
    ) -> Result<Self, String> {
        if address.0 % 4 != 0 || module_base % 4096 != 0 || address.0 < module_base {
            return Err("invalid native v10 target address/base".into());
        }
        let name = symbol.strip_suffix(&[0]).unwrap_or(symbol);
        if name.is_empty() || name.contains(&0) {
            return Err("invalid native v10 entry symbol".into());
        }
        let mut version_name = name.to_vec();
        version_name.extend_from_slice(b"_abi_version\0");
        // SAFETY: the trusted library must export a non-unwinding version query.
        let version = unsafe { library.get::<unsafe extern "C" fn() -> u32>(&version_name) }
            .map_err(|e| e.to_string())?;
        if unsafe { version() } != CPU_V10_ABI_VERSION {
            return Err("native v10 target ABI version mismatch".into());
        }
        if let Some(expected) = table_version {
            let mut query = name.to_vec();
            query.extend_from_slice(b"_exclusive_version\0");
            let version = unsafe { library.get::<unsafe extern "C" fn() -> u32>(&query) }
                .map_err(|e| e.to_string())?;
            if unsafe { version() } != expected {
                return Err("native exclusive-v2 target version mismatch".into());
            }
        }
        // SAFETY: the caller guarantees the advertised version's function ABI.
        let function = unsafe { library.get::<Function>(symbol) }.map_err(|e| e.to_string())?;
        Ok(Self {
            function,
            address,
            module_base,
        })
    }
    /// Invalid attachment/profile refusals occur before PC/base/budget mutation.
    /// A tail/unsupported/fault status is returned unchanged, never routed as a
    /// completed return. Cross-module dispatch is not yet implemented here.
    pub fn run(&self, attachment: &mut Attachment<'_>, budget: &mut u64) -> Result<i32, String> {
        attachment.validate()?;
        let cpu = attachment.cpu_mut();
        cpu.base.module_base = self.module_base;
        cpu.base.pc = self.address.0;
        // SAFETY: load's contract and the live owned attachment preserve ABI,
        // code/context lifetime and exclusive access to CPU/shared budget.
        Ok(unsafe { (self.function)(cpu, budget) })
    }
}
/// Host-side routing of explicit v10 tail requests between registered entries.
/// Nested BL/BLR callbacks remain unbound; this does not install an ABI-v9 call.
pub struct TailRouter<'code> {
    entries: std::collections::BTreeMap<GuestAddr, Entry<'code>>,
}
impl<'code> TailRouter<'code> {
    pub fn new(entries: Vec<Entry<'code>>) -> Result<Self, String> {
        let mut targets = std::collections::BTreeMap::new();
        for entry in entries {
            if targets.insert(entry.address, entry).is_some() {
                return Err("duplicate v10 tail target".into());
            }
        }
        Ok(Self { entries: targets })
    }
    /// Leaves the last invoked module base and native PC/status visible. Every
    /// entry revalidates the live attachment. A tail must consume budget before
    /// routing; exhausted tails return budget status without invoking a target.
    pub fn run(
        &self,
        attachment: &mut Attachment<'_>,
        target: GuestAddr,
        budget: &mut u64,
    ) -> Result<i32, String> {
        attachment.validate()?;
        let mut next = target;
        loop {
            let Some(entry) = self.entries.get(&next) else {
                let c = &mut attachment.cpu_mut().base;
                c.fault_address = next.0;
                c.fault_status = 3;
                return Ok(3);
            };
            let before = *budget;
            let status = entry.run(attachment, budget)?;
            if status != crate::abi::EXIT_TAIL {
                return Ok(status);
            }
            if *budget >= before {
                attachment.cpu_mut().base.fault_status = 6;
                return Ok(6);
            }
            if *budget == 0 {
                return Ok(2);
            }
            next = GuestAddr(attachment.cpu_mut().base.pc);
        }
    }
}
