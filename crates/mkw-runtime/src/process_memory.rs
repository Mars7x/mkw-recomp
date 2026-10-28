//! Explicit isolated private-page process queries. No discovered target limits.
use crate::{
    host_call::HostFault,
    owned_memory_budget::OwnedMemoryBudget,
    process_entropy::{ProcessEntropy, GET_INFO, INVALID_COMBINATION, RANDOM_ENTROPY},
    services::{Call, Outputs, Reply, Service, Services},
};
use std::sync::Arc;

pub const TOTAL_NON_SYSTEM: u32 = 21;
pub const USED_NON_SYSTEM: u32 = 22;
pub const CURRENT_PROCESS: u32 = 0xffff8001;

pub enum Abi {
    /// Every accounted page is private/non-system; no shared/external resource
    /// consumers or secure-memory charge. Allowance is an explicitly configured
    /// process limit, not a claim about Switch 2 RAM or resource policy.
    Nx64IsolatedPrivatePages,
}
#[derive(Clone)]
pub struct ProcessMemory {
    budget: OwnedMemoryBudget,
}
struct Handler {
    memory: ProcessMemory,
    entropy: Option<ProcessEntropy>,
}
impl ProcessMemory {
    pub fn new(_abi: Abi, budget: OwnedMemoryBudget) -> Self {
        Self { budget }
    }
    pub fn registry(&self) -> Services {
        self.registry_with(None)
    }
    /// Explicit InfoType dispatch under one GetInfo entry. The entire composed
    /// profile is memory-generation scoped; standalone entropy is unchanged.
    pub fn registry_with_entropy(&self, entropy: &ProcessEntropy) -> Services {
        self.registry_with(Some(entropy.clone()))
    }
    fn registry_with(&self, entropy: Option<ProcessEntropy>) -> Services {
        Services::new(vec![(
            GET_INFO,
            Arc::new(Handler {
                memory: self.clone(),
                entropy,
            }),
        )])
        .expect("one process GetInfo handler")
    }
}
impl Service for Handler {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        if !self.memory.budget.matches_call(call) {
            return Err(HostFault::unsupported());
        }
        let args = call.arguments();
        match args[1] as u32 {
            RANDOM_ENTROPY => self
                .entropy
                .as_ref()
                .ok_or_else(HostFault::unsupported)?
                .invoke(call),
            TOTAL_NON_SYSTEM | USED_NON_SYSTEM => {
                // Process queries validate subtype before handle lookup. Errors
                // preserve unspecified X1, without asserting retail output bits.
                if args[3] != 0 {
                    return Ok(Reply::Return(
                        Outputs::default().write_w(0, INVALID_COMBINATION)?,
                    ));
                }
                if args[2] as u32 != CURRENT_PROCESS {
                    // No real/debug process handle model is supplied here.
                    return Err(HostFault::unsupported());
                }
                let snapshot = self
                    .memory
                    .budget
                    .snapshot()
                    .map_err(|_| HostFault::unsupported())?;
                let value = if args[1] as u32 == TOTAL_NON_SYSTEM {
                    snapshot.allowance
                } else {
                    snapshot.used
                };
                Ok(Reply::Return(
                    Outputs::default().write_w(0, 0)?.write_x(1, value)?,
                ))
            }
            _ => Err(HostFault::unsupported()),
        }
    }
}
