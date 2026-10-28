//! Explicit private-catalog QueryMemory profile; no target configuration discovery.
use crate::{
    host_call::HostFault,
    memory_catalog::{Error, MemoryCatalog, State},
    services::{Call, Outputs, Reply, Service, Services},
    stack_query::QUERY_MEMORY,
    GuestAddr,
};
use std::sync::Arc;
pub enum Abi {
    /// Zero-based address space, private pages, no modeled attributes or IPC/
    /// device references. Bad output memory uses checked host faults, not a
    /// fabricated kernel ResultInvalidPointer. No retail compatibility claim.
    Nx64PrivateCatalog,
}
pub struct MemoryQuery {
    catalog: MemoryCatalog,
}
impl MemoryQuery {
    pub fn new(_abi: Abi, catalog: MemoryCatalog) -> Result<Self, Error> {
        if catalog.address_space().base.0 != 0 {
            return Err(Error::Configuration);
        }
        catalog.snapshot()?;
        Ok(Self { catalog })
    }
    pub fn registry(self) -> Services {
        Services::new(vec![(QUERY_MEMORY, Arc::new(self))]).expect("one catalog query handler")
    }
}
impl Service for MemoryQuery {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        if !self.catalog.matches_call(call) {
            return Err(HostFault::unsupported());
        }
        let args = call.arguments();
        let entries = self
            .catalog
            .snapshot()
            .map_err(|_| HostFault::unsupported())?;
        let end = self.catalog.address_space().size;
        let (base, size, state, permissions) = if args[2] >= end {
            (end, 0u64.wrapping_sub(end), 16u32, 0u32)
        } else {
            let e = entries
                .iter()
                .find(|e| args[2] >= e.span.base.0 && args[2] - e.span.base.0 < e.span.size)
                .ok_or_else(HostFault::unsupported)?;
            let state = match e.state {
                State::Free => 0,
                State::Code => 3,
                State::CodeData => 4,
                State::Normal => 5,
                State::Stack => 11,
                State::ThreadLocal => 12,
            };
            let p = u32::from(e.permissions.read)
                | (u32::from(e.permissions.write) << 1)
                | (u32::from(e.permissions.execute) << 2);
            (e.span.base.0, e.span.size, state, p)
        };
        let mut info = Vec::with_capacity(40);
        info.extend_from_slice(&base.to_le_bytes());
        info.extend_from_slice(&size.to_le_bytes());
        for word in [state, 0, permissions, 0, 0, 0] {
            info.extend_from_slice(&word.to_le_bytes());
        }
        Ok(Reply::Return(
            Outputs::default()
                .write_w(0, 0)?
                .write_w(1, 0)?
                .write_memory(GuestAddr(args[0]), info)?,
        ))
    }
}
