//! Explicit disjoint virtual reservations over sealed private page mappings.
//! No kernel layout discovery, mapping activation or GetInfo/QueryMemory handler.
use crate::{
    exclusive::{ExclusiveMemory, Heap, HeapSpec},
    GuestAddr, Memory,
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub base: GuestAddr,
    pub size: u64,
}
impl Span {
    fn end(self) -> Option<u64> {
        (self.size != 0 && self.base.0 % 4096 == 0 && self.size % 4096 == 0)
            .then(|| self.base.0.checked_add(self.size))
            .flatten()
    }
    fn contains(self, other: Self) -> bool {
        match (self.end(), other.end()) {
            (Some(end), Some(other_end)) => self.base <= other.base && other_end <= end,
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub address_space: Span,
    pub code: Span,
    pub heap: Span,
    pub stack: Span,
    pub alias: Span,
    pub remote: Span,
}
impl Layout {
    fn valid(self) -> bool {
        let mut regions = [self.code, self.heap, self.stack, self.alias, self.remote];
        if !regions.iter().all(|r| self.address_space.contains(*r)) {
            return false;
        }
        regions.sort_by_key(|r| r.base);
        regions
            .windows(2)
            .all(|w| w[0].end().is_some_and(|end| end <= w[1].base.0))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    StaleGeneration,
    Poisoned,
}

#[derive(Clone)]
pub struct ProcessRegions {
    memory: Arc<Mutex<ExclusiveMemory>>,
    authority: Arc<()>,
    layout: Layout,
}
impl ProcessRegions {
    /// All five reservations are explicit, nonempty, page-exact and disjoint.
    /// Heap must equal its existing capability's full reserved range. Every
    /// fixed mapping must fit code or stack; stack mappings must be RW. Alias
    /// and remote reserve uncommitted guest addresses, not host RAM or mappings.
    pub fn new(
        memory: Arc<Mutex<ExclusiveMemory>>,
        heap: &Heap,
        layout: Layout,
    ) -> Result<Self, Error> {
        if !layout.valid() {
            return Err(Error::Configuration);
        }
        let authority = {
            let m = memory.lock().map_err(|_| Error::Poisoned)?;
            if !m.validate_region_layout(&layout, heap) {
                return Err(Error::Configuration);
            }
            m.authority()
        };
        Ok(Self {
            memory,
            authority,
            layout,
        })
    }
    /// Return configured reservations only while the bound memory generation is
    /// live. Heap resize changes commitment, not these immutable bounds. No
    /// guest memory, reservations, owner slots or service registry is modified.
    pub fn snapshot(&self) -> Result<Layout, Error> {
        let m = self.memory.lock().map_err(|_| Error::Poisoned)?;
        if !m.has_authority(&self.authority) {
            return Err(Error::StaleGeneration);
        }
        Ok(self.layout)
    }
}

impl Memory {
    pub(crate) fn validate_region_layout(&self, layout: &Layout, heap: HeapSpec) -> bool {
        if self.page_accounting().is_none()
            || layout.heap
                != (Span {
                    base: heap.base,
                    size: heap.capacity as u64,
                })
        {
            return false;
        }
        let mut heaps = 0;
        for r in self.regions.values() {
            let span = Span {
                base: r.base,
                size: r.bytes.len() as u64,
            };
            if r.heap_visible.is_some() {
                heaps += 1;
                if span != layout.heap {
                    return false;
                }
            } else if layout.stack.contains(span) {
                if r.permissions != crate::Permissions::RW {
                    return false;
                }
            } else if !layout.code.contains(span) {
                return false;
            }
        }
        heaps == 1
    }
}
