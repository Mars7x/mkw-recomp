// SPDX-License-Identifier: MIT
// Event lifetime/manual-reset behavior adapted from seedds/nxnative hle_thread.c
// c8ab8166f68296dc712a4ec13b892535d9d790c6. Copyright (c) 2026 seedds;
// generic host ancestry Copyright (c) 2026 jhaizhou. Notices: third_party/nxnative/.
// Independent Rust storage/identity implementation; no Mach/native guest stacks.
// This primitive has no SVC registration, clock, blocking wait or thread model.
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};
const FIRST: u32 = crate::kernel_handles::FIRST;
const END: u32 = crate::kernel_handles::END;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle(u32);
impl Handle {
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
    pub fn raw(self) -> u32 {
        self.0
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Ends {
    pub write: Handle,
    pub read: Handle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InvalidHandle,
    AlreadyClear,
    Quota,
    IdentityExhausted,
    Poisoned,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Read,
    Write,
}
struct Event {
    signaled: bool,
    read_open: bool,
    write_open: bool,
}
#[derive(Default)]
struct State {
    objects: BTreeMap<u32, Event>,
    handles: BTreeMap<u32, (u32, Side)>,
}
pub struct Events {
    state: Mutex<State>,
    ids: Arc<AtomicU32>,
    limit: usize,
}
impl Events {
    pub fn new(limit: usize) -> Result<Self, Error> {
        Self::with_ids(limit, crate::kernel_handles::shared_ids())
    }
    fn with_ids(limit: usize, ids: Arc<AtomicU32>) -> Result<Self, Error> {
        if limit == 0 || limit > 1024 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            state: Mutex::new(State::default()),
            ids,
            limit,
        })
    }
    pub fn create(&self) -> Result<Ends, Error> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.objects.len() >= self.limit {
            return Err(Error::Quota);
        }
        let mut first = self.ids.load(Ordering::Relaxed);
        loop {
            let next = first
                .checked_add(2)
                .filter(|&n| first >= FIRST && n <= END)
                .ok_or(Error::IdentityExhausted)?;
            match self
                .ids
                .compare_exchange_weak(first, next, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(observed) => first = observed,
            }
        }
        state.objects.insert(
            first,
            Event {
                signaled: false,
                read_open: true,
                write_open: true,
            },
        );
        state.handles.insert(first, (first, Side::Write));
        state.handles.insert(first + 1, (first, Side::Read));
        Ok(Ends {
            write: Handle(first),
            read: Handle(first + 1),
        })
    }
    fn update(&self, handle: Handle, op: u8) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        let &(id, side) = state.handles.get(&handle.0).ok_or(Error::InvalidHandle)?;
        if (op == 0 && side != Side::Write) || (op == 2 && side != Side::Read) {
            return Err(Error::InvalidHandle);
        }
        let event = state
            .objects
            .get_mut(&id)
            .expect("live event handle owns its object");
        if op == 2 && !event.signaled {
            return Err(Error::AlreadyClear);
        }
        event.signaled = op == 0;
        Ok(())
    }
    pub fn signal(&self, write: Handle) -> Result<(), Error> {
        self.update(write, 0)
    }
    pub fn clear(&self, either: Handle) -> Result<(), Error> {
        self.update(either, 1)
    }
    pub fn reset(&self, read: Handle) -> Result<(), Error> {
        self.update(read, 2)
    }
    /// Manual-reset wait-any poll. Every handle is validated before selecting the
    /// first signaled index. Poll never consumes a signal and never supplies time.
    pub fn poll(&self, reads: &[Handle]) -> Result<Option<usize>, Error> {
        if reads.is_empty() || reads.len() > 64 {
            return Err(Error::Configuration);
        }
        let state = self.state.lock().map_err(|_| Error::Poisoned)?;
        let mut found = None;
        for (index, handle) in reads.iter().enumerate() {
            let &(id, side) = state.handles.get(&handle.0).ok_or(Error::InvalidHandle)?;
            if side != Side::Read {
                return Err(Error::InvalidHandle);
            }
            if state
                .objects
                .get(&id)
                .expect("live event handle owns its object")
                .signaled
                && found.is_none()
            {
                found = Some(index)
            }
        }
        Ok(found)
    }
    pub fn close(&self, handle: Handle) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        let (id, side) = state
            .handles
            .remove(&handle.0)
            .ok_or(Error::InvalidHandle)?;
        let e = state
            .objects
            .get_mut(&id)
            .expect("live event handle owns its object");
        match side {
            Side::Read => e.read_open = false,
            Side::Write => e.write_open = false,
        }
        if !e.read_open && !e.write_open {
            state.objects.remove(&id);
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paired_event_lifetime_rights_manual_reset_and_first_poll() {
        let e = Events::new(2).unwrap();
        let a = e.create().unwrap();
        let b = e.create().unwrap();
        assert_eq!(e.create().err(), Some(Error::Quota));
        assert_eq!(e.poll(&[a.read, b.read]), Ok(None));
        assert_eq!(e.signal(a.read), Err(Error::InvalidHandle));
        assert_eq!(e.reset(a.write), Err(Error::InvalidHandle));
        assert_eq!(e.reset(a.read), Err(Error::AlreadyClear));
        e.signal(b.write).unwrap();
        assert_eq!(e.poll(&[a.read, b.read]), Ok(Some(1)));
        assert_eq!(e.poll(&[b.read, b.read]), Ok(Some(0)));
        e.signal(a.write).unwrap();
        assert_eq!(e.poll(&[a.read, b.read]), Ok(Some(0)));
        e.reset(a.read).unwrap();
        e.clear(b.write).unwrap();
        assert_eq!(e.poll(&[a.read, b.read]), Ok(None));
        e.signal(a.write).unwrap();
        e.close(a.write).unwrap();
        assert_eq!(e.poll(&[a.read]), Ok(Some(0)));
        assert_eq!(e.create().err(), Some(Error::Quota));
        e.close(a.read).unwrap();
        let c = e.create().unwrap();
        assert_ne!(c.write, a.write);
        assert_ne!(c.read, a.read);
        assert_eq!(e.close(a.read), Err(Error::InvalidHandle));
        assert_eq!(e.signal(a.write), Err(Error::InvalidHandle));
        e.close(c.read).unwrap();
        e.signal(c.write).unwrap();
        e.close(c.write).unwrap();
    }
    #[test]
    fn event_ids_are_foreign_stale_safe_and_creation_failure_is_atomic() {
        let a = Events::new(1).unwrap();
        let b = Events::new(1).unwrap();
        let e = a.create().unwrap();
        let f = b.create().unwrap();
        assert_ne!(e.read, f.read);
        assert_eq!(b.signal(e.write), Err(Error::InvalidHandle));
        assert_eq!(a.poll(&[f.read]), Err(Error::InvalidHandle));
        a.signal(e.write).unwrap();
        assert_eq!(a.poll(&[e.read, Handle(0)]), Err(Error::InvalidHandle));
        assert_eq!(a.poll(&[e.write]), Err(Error::InvalidHandle));
        assert_eq!(a.poll(&[]), Err(Error::Configuration));
        assert_eq!(a.poll(&[e.read; 65]), Err(Error::Configuration));
        let ids = Arc::new(AtomicU32::new(END - 1));
        let z = Events::with_ids(1, ids.clone()).unwrap();
        assert_eq!(z.create().err(), Some(Error::IdentityExhausted));
        assert!(z.state.lock().unwrap().handles.is_empty());
        assert!(z.state.lock().unwrap().objects.is_empty());
        ids.store(END - 2, Ordering::Relaxed);
        let last = z.create().unwrap();
        assert_eq!(last.read.raw(), END - 1);
        z.close(last.read).unwrap();
        z.close(last.write).unwrap();
        assert_eq!(z.create().err(), Some(Error::IdentityExhausted));
        assert_eq!(Events::new(0).err(), Some(Error::Configuration));
    }
    #[test]
    fn concurrent_event_cycles_do_not_alias_handles_or_consume_peer_signals() {
        let events = Arc::new(Events::new(8).unwrap());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let events = events.clone();
            handles.push(std::thread::spawn(move || {
                let mut ids = Vec::new();
                for _ in 0..500 {
                    let e = events.create().unwrap();
                    events.signal(e.write).unwrap();
                    assert_eq!(events.poll(&[e.read]), Ok(Some(0)));
                    assert_eq!(events.poll(&[e.read]), Ok(Some(0)));
                    events.clear(e.read).unwrap();
                    assert_eq!(events.poll(&[e.read]), Ok(None));
                    events.close(e.read).unwrap();
                    events.close(e.write).unwrap();
                    ids.extend([e.read.raw(), e.write.raw()]);
                }
                ids
            }));
        }
        let ids: std::collections::BTreeSet<_> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        assert_eq!(ids.len(), 8000);
        assert!(events.state.lock().unwrap().objects.is_empty());
        assert!(events.state.lock().unwrap().handles.is_empty());
    }
}
