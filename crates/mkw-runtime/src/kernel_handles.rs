//! Explicit host-owned object references and bounded diagnostic handle tables.
//! No process/thread/IPC service is synthesized. Handle values are not a retail
//! encoding claim. All event/table identities share one non-reusing allocator
//! below the reserved owner/waiter tag bits.
use std::{
    any::Any,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
};
static IDS: OnceLock<Arc<AtomicU32>> = OnceLock::new();
pub(crate) const FIRST: u32 = 0x1001;
pub(crate) const END: u32 = 0x40000000;
pub(crate) fn shared_ids() -> Arc<AtomicU32> {
    IDS.get_or_init(|| Arc::new(AtomicU32::new(FIRST))).clone()
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handle(u32);
impl Handle {
    pub fn from_raw(value: u32) -> Self {
        Self(value)
    }
    pub fn raw(self) -> u32 {
        self.0
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InvalidHandle,
    WrongType,
    Quota,
    IdentityExhausted,
    Poisoned,
    Closed,
}
/// A pinned object capability, not an entry in any destination table. Closing the
/// source handle after export does not destroy this owned reference. Host-only;
/// a real IPC implementation must check packet rights before creating/importing it.
#[derive(Clone)]
pub struct Exported(Arc<dyn Any + Send + Sync>);
struct State {
    entries: BTreeMap<u32, Arc<dyn Any + Send + Sync>>,
    closed: bool,
}
pub struct Table {
    state: Mutex<State>,
    limit: usize,
    ids: Arc<AtomicU32>,
}
fn reserve_from(ids: &AtomicU32, count: u32) -> Result<u32, Error> {
    if !(1..=2).contains(&count) {
        return Err(Error::Configuration);
    }
    let mut old = ids.load(Ordering::Relaxed);
    loop {
        let next = old
            .checked_add(count)
            .filter(|&n| old >= FIRST && n <= END)
            .ok_or(Error::IdentityExhausted)?;
        match ids.compare_exchange_weak(old, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(old),
            Err(observed) => old = observed,
        }
    }
}
impl Table {
    /// Explicit entry quota. Zero grants no handles; no guessed default capacity.
    pub fn new(limit: usize) -> Result<Self, Error> {
        if limit > 1023 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            state: Mutex::new(State {
                entries: BTreeMap::new(),
                closed: false,
            }),
            limit,
            ids: shared_ids(),
        })
    }
    pub fn insert<T: Any + Send + Sync>(&self, object: Arc<T>) -> Result<Handle, Error> {
        self.import(Exported(object))
    }
    pub fn import(&self, object: Exported) -> Result<Handle, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.closed {
            return Err(Error::Closed);
        }
        if state.entries.len() >= self.limit {
            return Err(Error::Quota);
        }
        let id = reserve_from(&self.ids, 1)?;
        state.entries.insert(id, object.0);
        Ok(Handle(id))
    }
    /// Both entries publish atomically. Failed allocation publishes neither;
    /// dropping passed object references preserves normal RAII cleanup.
    pub fn insert_pair<A: Any + Send + Sync, B: Any + Send + Sync>(
        &self,
        a: Arc<A>,
        b: Arc<B>,
    ) -> Result<(Handle, Handle), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.closed {
            return Err(Error::Closed);
        }
        if self.limit.saturating_sub(state.entries.len()) < 2 {
            return Err(Error::Quota);
        }
        let id = reserve_from(&self.ids, 2)?;
        state.entries.insert(id, a);
        state.entries.insert(id + 1, b);
        Ok((Handle(id), Handle(id + 1)))
    }
    pub fn export(&self, handle: Handle) -> Result<Exported, Error> {
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        state
            .entries
            .get(&handle.0)
            .cloned()
            .map(Exported)
            .ok_or(Error::InvalidHandle)
    }
    pub fn get<T: Any + Send + Sync>(&self, handle: Handle) -> Result<Arc<T>, Error> {
        self.export(handle)?
            .0
            .downcast()
            .map_err(|_| Error::WrongType)
    }
    /// Snapshot the entire input list under one table lock. A later invalid or
    /// wrong-kind entry cannot be bypassed by an earlier ready object.
    pub fn get_many<T: Any + Send + Sync>(&self, handles: &[Handle]) -> Result<Vec<Arc<T>>, Error> {
        if handles.len() > 64 {
            return Err(Error::Configuration);
        }
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        handles
            .iter()
            .map(|h| {
                state
                    .entries
                    .get(&h.0)
                    .cloned()
                    .ok_or(Error::InvalidHandle)?
                    .downcast()
                    .map_err(|_| Error::WrongType)
            })
            .collect()
    }
    /// Permanently stop insertion and remove every entry before dropping any
    /// payload. Exported/lookup pins remain live owned references. Explicit host
    /// process teardown must call this to break payload-to-table Arc cycles.
    pub fn shutdown(&self) -> Result<(), Error> {
        let removed = {
            let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
            state.closed = true;
            std::mem::take(&mut state.entries)
        };
        drop(removed);
        Ok(())
    }
    pub fn close(&self, handle: Handle) -> Result<(), Error> {
        // Destroy an arbitrary object only after releasing the table lock: its
        // destructor may close another handle or take a backend lock.
        let removed = self
            .state
            .lock()
            .map_err(|_| Error::Poisoned)?
            .entries
            .remove(&handle.0)
            .ok_or(Error::InvalidHandle)?;
        drop(removed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_pair_exhaustion_does_not_consume_the_remaining_single_or_wrap() {
        let ids = AtomicU32::new(END - 1);
        assert_eq!(reserve_from(&ids, 2), Err(Error::IdentityExhausted));
        assert_eq!(ids.load(Ordering::Relaxed), END - 1);
        assert_eq!(reserve_from(&ids, 1), Ok(END - 1));
        assert_eq!(reserve_from(&ids, 1), Err(Error::IdentityExhausted));
        for value in [0, FIRST - 1, u32::MAX] {
            ids.store(value, Ordering::Relaxed);
            assert_eq!(reserve_from(&ids, 1), Err(Error::IdentityExhausted));
            assert_eq!(ids.load(Ordering::Relaxed), value);
        }
        assert_eq!(reserve_from(&ids, 0), Err(Error::Configuration));
    }
}
