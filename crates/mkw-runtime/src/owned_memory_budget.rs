//! Host-owned, page-exact accounting for sealed private mappings and a bounded
//! heap. This is not physical-memory discovery or a kernel GetInfo handler.
use crate::{exclusive::ExclusiveMemory, Memory};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    StaleGeneration,
    Poisoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    /// Explicit caller-supplied allowance, not host RAM or a discovered limit.
    pub allowance: u64,
    /// Currently guest-visible bytes, including every fixed mapping once.
    pub used: u64,
    /// Maximum visible bytes if the reserved heap grows to capacity.
    pub committed_capacity: u64,
    /// Allowance minus currently visible bytes. Not a kernel resource counter.
    pub available: u64,
}

#[derive(Clone)]
pub struct OwnedMemoryBudget {
    memory: Arc<Mutex<ExclusiveMemory>>,
    authority: Arc<()>,
    allowance: u64,
}
impl OwnedMemoryBudget {
    /// Bind to one sealed memory generation. All mappings must have page-exact
    /// bases and sizes; no guessed rounding, alias deduplication or external
    /// resource consumption is modeled. Reserve enough allowance for the full
    /// backing capacity so every existing legal heap resize stays in budget.
    pub fn new(memory: Arc<Mutex<ExclusiveMemory>>, allowance: u64) -> Result<Self, Error> {
        if allowance == 0 || allowance % 4096 != 0 {
            return Err(Error::Configuration);
        }
        let authority = {
            let m = memory.lock().map_err(|_| Error::Poisoned)?;
            let (_, capacity) = m.page_accounting().ok_or(Error::Configuration)?;
            if capacity > allowance {
                return Err(Error::Configuration);
            }
            m.authority()
        };
        Ok(Self {
            memory,
            authority,
            allowance,
        })
    }

    /// Snapshot usage under the same mutex as native memory/service effects and
    /// heap resize. No memory, owner slots or exclusive reservations are changed.
    /// Replacing the container's memory invalidates all old budget capabilities.
    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        let m = self.memory.lock().map_err(|_| Error::Poisoned)?;
        if !m.has_authority(&self.authority) {
            return Err(Error::StaleGeneration);
        }
        let (used, capacity) = m.page_accounting().ok_or(Error::Configuration)?;
        if capacity > self.allowance {
            return Err(Error::Configuration);
        }
        Ok(Snapshot {
            allowance: self.allowance,
            used,
            committed_capacity: capacity,
            available: self
                .allowance
                .checked_sub(used)
                .ok_or(Error::Configuration)?,
        })
    }
}

impl Memory {
    pub(crate) fn page_accounting(&self) -> Option<(u64, u64)> {
        if !self.sealed {
            return None;
        }
        self.regions
            .values()
            .try_fold((0u64, 0u64), |(used, capacity), r| {
                let visible = u64::try_from(r.visible_len()).ok()?;
                let reserved = u64::try_from(r.bytes.len()).ok()?;
                if r.base.0 % 4096 != 0
                    || visible % 4096 != 0
                    || reserved % 4096 != 0
                    || visible > reserved
                {
                    return None;
                }
                Some((used.checked_add(visible)?, capacity.checked_add(reserved)?))
            })
    }
}
