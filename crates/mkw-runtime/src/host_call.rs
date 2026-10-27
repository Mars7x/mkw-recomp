//! Checked native HLE boundary. Guest addresses never become host pointers.
use crate::{exclusive_native::Context, GuestAddr};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostReturn {
    U32(u32),
    Void,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostFault {
    pub(crate) status: i32,
    pub(crate) address: GuestAddr,
}
impl HostFault {
    pub fn unsupported() -> Self {
        Self {
            status: 3,
            address: GuestAddr(0),
        }
    }
}
pub trait HostService: Send + Sync {
    /// Implement a bounded host operation. Validate every output before external
    /// effects, then commit its batch last. No recursive router access is supplied.
    fn invoke(&self, call: &mut HostCall<'_>) -> Result<HostReturn, HostFault>;
}
pub struct HostCall<'a> {
    context: &'a Context,
    args: [u64; 8],
}
impl<'a> HostCall<'a> {
    pub(crate) fn new(context: &'a Context, registers: &[u64; 31]) -> Self {
        Self {
            context,
            args: registers[..8].try_into().unwrap(),
        }
    }
    pub fn arguments(&self) -> [u64; 8] {
        self.args
    }
    fn memory(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, crate::exclusive::ExclusiveMemory>, HostFault> {
        let memory = self
            .context
            .memory
            .lock()
            .map_err(|_| HostFault::unsupported())?;
        memory
            .validate_owner(&self.context.owner)
            .map_err(|_| HostFault::unsupported())?;
        Ok(memory)
    }
    pub fn check_outputs(&self, ranges: &[(GuestAddr, usize)]) -> Result<(), HostFault> {
        let memory = self.memory()?;
        for (address, len) in ranges {
            memory
                .validate_access(*address, *len, true)
                .map_err(|_| HostFault {
                    status: 5,
                    address: *address,
                })?;
        }
        Ok(())
    }
    pub fn write_outputs(&mut self, writes: &[(GuestAddr, &[u8])]) -> Result<(), HostFault> {
        let mut memory = self.memory()?;
        for (address, bytes) in writes {
            memory
                .validate_access(*address, bytes.len(), true)
                .map_err(|_| HostFault {
                    status: 5,
                    address: *address,
                })?;
        }
        memory
            .write_batch(writes)
            .map_err(|_| HostFault::unsupported())
    }
    pub fn c_string(&self, address: GuestAddr, max_bytes: usize) -> Result<Vec<u8>, HostFault> {
        if max_bytes > 4096 {
            return Err(HostFault::unsupported());
        }
        let memory = self.memory()?;
        let mut bytes = Vec::new();
        for i in 0..=max_bytes {
            let at = GuestAddr(
                address
                    .0
                    .checked_add(i as u64)
                    .ok_or(HostFault { status: 5, address })?,
            );
            let mut byte = [0];
            memory.read_into(at, &mut byte).map_err(|_| HostFault {
                status: 5,
                address: at,
            })?;
            if byte[0] == 0 {
                return Ok(bytes);
            }
            bytes.push(byte[0]);
        }
        Err(HostFault::unsupported())
    }
}
