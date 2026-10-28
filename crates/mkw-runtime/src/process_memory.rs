//! Explicit isolated private-page process queries. No discovered target limits.
use crate::{
    host_call::HostFault,
    owned_memory_budget::OwnedMemoryBudget,
    process_entropy::{ProcessEntropy, GET_INFO, INVALID_COMBINATION, RANDOM_ENTROPY},
    process_regions::{Error as RegionError, ProcessRegions, Span},
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
    regions: Option<(ProcessRegions, Span)>,
    observed_remote: bool,
}
struct Handler {
    memory: ProcessMemory,
    entropy: Option<ProcessEntropy>,
}
impl ProcessMemory {
    pub fn new(_abi: Abi, budget: OwnedMemoryBudget) -> Self {
        Self {
            budget,
            regions: None,
            observed_remote: false,
        }
    }
    /// Add the NX reference region queries under the same GetInfo handler.
    /// ASLR is an explicit enclosing domain, not the code reservation. All five
    /// host reservations must fit it. Remote queries require a separate explicit opt-in.
    pub fn with_regions(
        mut self,
        regions: ProcessRegions,
        aslr: Span,
    ) -> Result<Self, RegionError> {
        if !regions.matches_budget(&self.budget) {
            return Err(RegionError::Configuration);
        }
        let layout = regions.snapshot()?;
        if !layout.address_space.contains(aslr)
            || ![
                layout.code,
                layout.heap,
                layout.stack,
                layout.alias,
                layout.remote,
            ]
            .into_iter()
            .all(|r| aslr.contains(r))
        {
            return Err(RegionError::Configuration);
        }
        self.regions = Some((regions, aslr));
        self.observed_remote = false;
        Ok(self)
    }
    /// Opt into the supplied consumer's remote-address/size success protocol.
    /// This is host configuration, not retail bounds/error semantics. Only
    /// current-process/subtype-zero queries are supported. Replacing regions
    /// disables this selection and requires a fresh explicit opt-in.
    pub fn with_observed_remote_region(mut self) -> Result<Self, RegionError> {
        let (regions, _) = self.regions.as_ref().ok_or(RegionError::Configuration)?;
        let remote = regions.snapshot()?.remote;
        if remote.base.0 == 0 || remote.base.0 % 65536 != 0 || remote.size % 65536 != 0 {
            return Err(RegionError::Configuration);
        }
        self.observed_remote = true;
        Ok(self)
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
        if self
            .memory
            .regions
            .as_ref()
            .is_some_and(|(r, _)| !r.matches_call(call))
        {
            return Err(HostFault::unsupported());
        }
        let args = call.arguments();
        match args[1] as u32 {
            RANDOM_ENTROPY => self
                .entropy
                .as_ref()
                .ok_or_else(HostFault::unsupported)?
                .invoke(call),
            29 | 30 => {
                // No unverified kernel error codes or generic zero fallback.
                if !self.memory.observed_remote || args[2] as u32 != CURRENT_PROCESS || args[3] != 0
                {
                    return Err(HostFault::unsupported());
                }
                let (regions, _) = self
                    .memory
                    .regions
                    .as_ref()
                    .ok_or_else(HostFault::unsupported)?;
                let remote = regions
                    .snapshot()
                    .map_err(|_| HostFault::unsupported())?
                    .remote;
                let value = if args[1] as u32 == 29 {
                    remote.base.0
                } else {
                    remote.size
                };
                Ok(Reply::Return(
                    Outputs::default().write_w(0, 0)?.write_x(1, value)?,
                ))
            }
            2..=5 | 12..=15 | TOTAL_NON_SYSTEM | USED_NON_SYSTEM => {
                let region_value = match args[1] as u32 {
                    2..=5 | 12..=15 => {
                        let (regions, aslr) = self
                            .memory
                            .regions
                            .as_ref()
                            .ok_or_else(HostFault::unsupported)?;
                        let l = regions.snapshot().map_err(|_| HostFault::unsupported())?;
                        Some(match args[1] as u32 {
                            2 => l.alias.base.0,
                            3 => l.alias.size,
                            4 => l.heap.base.0,
                            5 => l.heap.size,
                            12 => aslr.base.0,
                            13 => aslr.size,
                            14 => l.stack.base.0,
                            15 => l.stack.size,
                            _ => unreachable!(),
                        })
                    }
                    _ => None,
                };
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
                if let Some(value) = region_value {
                    return Ok(Reply::Return(
                        Outputs::default().write_w(0, 0)?.write_x(1, value)?,
                    ));
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
