//! Explicit host memory-state partition; no kernel query handler or discovery.
use crate::{
    exclusive::{ExclusiveMemory, Heap},
    process_regions::Span,
    GuestAddr, Memory, Permissions,
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Free,
    Code,
    CodeData,
    Normal,
    Stack,
    ThreadLocal,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declaration {
    pub span: Span,
    pub state: State,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub span: Span,
    pub state: State,
    pub permissions: Permissions,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    StaleGeneration,
    Poisoned,
}
const NONE: Permissions = Permissions {
    read: false,
    write: false,
    execute: false,
};
#[derive(Clone)]
pub struct MemoryCatalog {
    memory: Arc<Mutex<ExclusiveMemory>>,
    authority: Arc<()>,
    address_space: Span,
    declarations: Vec<Declaration>,
}
impl MemoryCatalog {
    pub(crate) fn matches_budget(
        &self,
        budget: &crate::owned_memory_budget::OwnedMemoryBudget,
    ) -> bool {
        budget.matches_binding(&self.memory, &self.authority)
    }
    pub(crate) fn agrees_with_regions(&self, layout: crate::process_regions::Layout) -> bool {
        self.address_space == layout.address_space
            && self.declarations.iter().all(|d| {
                if d.state == State::Stack {
                    layout.stack.contains(d.span)
                } else {
                    layout.code.contains(d.span)
                }
            })
    }

    pub(crate) fn address_space(&self) -> Span {
        self.address_space
    }
    pub(crate) fn matches_call(&self, call: &crate::services::Call<'_>) -> bool {
        call.matches_memory(&self.memory, &self.authority)
    }

    /// Every fixed mapping must have one exact declaration. Hidden heap backing
    /// reserves addresses but is Free until committed. The declared address
    /// space must contain all backing, including the entire heap capacity.
    pub fn new(
        memory: Arc<Mutex<ExclusiveMemory>>,
        heap: &Heap,
        address_space: Span,
        mut declarations: Vec<Declaration>,
    ) -> Result<Self, Error> {
        declarations.sort_by_key(|d| d.span.base);
        let authority = {
            let m = memory.lock().map_err(|_| Error::Poisoned)?;
            m.heap_size(heap).map_err(|_| Error::Configuration)?;
            m.catalog_entries(address_space, &declarations)
                .ok_or(Error::Configuration)?;
            m.authority()
        };
        Ok(Self {
            memory,
            authority,
            address_space,
            declarations,
        })
    }
    /// One coherent partition of the entire declared span. Adjacent identical
    /// states/permissions coalesce; no kernel attributes or reference counts
    /// are inferred. Outside addresses have no catalog entry.
    pub fn snapshot(&self) -> Result<Vec<Entry>, Error> {
        let m = self.memory.lock().map_err(|_| Error::Poisoned)?;
        if !m.has_authority(&self.authority) {
            return Err(Error::StaleGeneration);
        }
        m.catalog_entries(self.address_space, &self.declarations)
            .ok_or(Error::Configuration)
    }
}
impl Memory {
    pub(crate) fn catalog_entries(
        &self,
        space: Span,
        declarations: &[Declaration],
    ) -> Option<Vec<Entry>> {
        self.page_accounting()?;
        if !space.contains(space) {
            return None;
        }
        if declarations
            .windows(2)
            .any(|w| w[0].span.base >= w[1].span.base)
        {
            return None;
        }
        let mut fixed = 0;
        let mut heaps = 0;
        let mut entries = Vec::new();
        let mut cursor = space.base.0;
        let end = space.base.0.checked_add(space.size)?;
        fn append(
            out: &mut Vec<Entry>,
            base: u64,
            size: u64,
            state: State,
            permissions: Permissions,
        ) {
            if size == 0 {
                return;
            }
            if let Some(last) = out.last_mut() {
                if last.span.base.0 + last.span.size == base
                    && last.state == state
                    && last.permissions == permissions
                {
                    last.span.size += size;
                    return;
                }
            }
            out.push(Entry {
                span: Span {
                    base: GuestAddr(base),
                    size,
                },
                state,
                permissions,
            });
        }
        for r in self.regions.values() {
            let span = Span {
                base: r.base,
                size: r.bytes.len() as u64,
            };
            if !space.contains(span) {
                return None;
            }
            let state = if r.heap_visible.is_some() {
                heaps += 1;
                if r.permissions != Permissions::RW {
                    return None;
                }
                State::Normal
            } else {
                let d = declarations.get(fixed)?;
                fixed += 1;
                if d.span != span {
                    return None;
                }
                let valid = match d.state {
                    State::Code => {
                        !r.permissions.write && (r.permissions.read || r.permissions.execute)
                    }
                    State::CodeData => {
                        r.permissions == Permissions::R || r.permissions == Permissions::RW
                    }
                    State::Normal | State::Stack | State::ThreadLocal => {
                        r.permissions == Permissions::RW
                    }
                    State::Free => false,
                };
                if !valid {
                    return None;
                }
                d.state
            };
            let visible = r.visible_len() as u64;
            if visible != 0 {
                append(
                    &mut entries,
                    cursor,
                    r.base.0.checked_sub(cursor)?,
                    State::Free,
                    NONE,
                );
                append(&mut entries, r.base.0, visible, state, r.permissions);
                cursor = r.base.0.checked_add(visible)?;
            }
        }
        if heaps != 1 || fixed != declarations.len() {
            return None;
        }
        append(
            &mut entries,
            cursor,
            end.checked_sub(cursor)?,
            State::Free,
            NONE,
        );
        Some(entries)
    }
}
