//! Explicit reference QueryMemory for one owned stack, not a virtual memory manager.
use crate::{
    exclusive::ExclusiveMemory,
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services},
    GuestAddr,
};
use std::sync::{Arc, Mutex};

pub const QUERY_MEMORY: u16 = 6;
pub enum Abi {
    Horizon64OwnedStack,
}

/// Immutable mapping declaration tied to both the memory container and its
/// generation. The caller explicitly assigns stack semantics; names infer none.
pub struct StackQuery {
    memory: Arc<Mutex<ExclusiveMemory>>,
    authority: Arc<()>,
    base: GuestAddr,
    end: u64,
}
impl StackQuery {
    pub fn new(
        _abi: Abi,
        memory: Arc<Mutex<ExclusiveMemory>>,
        base: GuestAddr,
        size: usize,
    ) -> Result<Self, String> {
        let end = base.0.checked_add(size as u64).ok_or("stack overflow")?;
        if size == 0 || base.0 % 4096 != 0 || size % 4096 != 0 {
            return Err("stack query requires whole aligned pages".into());
        }
        let authority = memory
            .lock()
            .map_err(|_| "poisoned stack memory")?
            .stack_authority(base, size)
            .ok_or("stack must exactly match one owned RW mapping")?;
        Ok(Self {
            memory,
            authority,
            base,
            end,
        })
    }
    pub fn registry(self) -> Services {
        Services::new(vec![(QUERY_MEMORY, Arc::new(self))]).expect("one stack query handler")
    }
}
impl Service for StackQuery {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let args = call.arguments();
        if !call.matches_memory(&self.memory, &self.authority)
            || !(self.base.0..self.end).contains(&args[2])
        {
            return Err(HostFault::unsupported());
        }
        let mut info = Vec::with_capacity(40);
        info.extend_from_slice(&self.base.0.to_le_bytes());
        info.extend_from_slice(&(self.end - self.base.0).to_le_bytes());
        // Stack state, no modeled attributes, RW, no IPC/device references,
        // explicit zero padding. Unknown regions have no invented response.
        for word in [11u32, 0, 3, 0, 0, 0] {
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
