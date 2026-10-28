//! Real event endpoints stored as typed owned objects in explicit handle tables.
//! The attributed event primitive supplies state; no IPC message or service result
//! is fabricated. Host-only construction/transfer must precede any explicit binding.
use crate::{
    events::{self, Events},
    kernel_handles::{self, Handle, Table},
};
use std::sync::Arc;
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Event(events::Error),
    Handle(kernel_handles::Error),
}
impl From<events::Error> for Error {
    fn from(e: events::Error) -> Self {
        Self::Event(e)
    }
}
impl From<kernel_handles::Error> for Error {
    fn from(e: kernel_handles::Error) -> Self {
        Self::Handle(e)
    }
}
pub struct ReadEnd {
    backend: Arc<Events>,
    handle: events::Handle,
}
pub struct WriteEnd {
    backend: Arc<Events>,
    handle: events::Handle,
}
impl Drop for ReadEnd {
    fn drop(&mut self) {
        let _ = self.backend.close(self.handle);
    }
}
impl Drop for WriteEnd {
    fn drop(&mut self) {
        let _ = self.backend.close(self.handle);
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum PollError {
    Configuration,
    MixedAuthorities,
    Event(events::Error),
}
impl ReadEnd {
    /// All entries must already be pinned and typed. A common event authority
    /// supplies one atomic first-ready snapshot; independent authorities refuse.
    pub fn poll_same_authority(reads: &[Arc<Self>]) -> Result<Option<usize>, PollError> {
        if reads.is_empty() || reads.len() > 64 {
            return Err(PollError::Configuration);
        }
        let first = &reads[0].backend;
        if reads.iter().any(|r| !Arc::ptr_eq(first, &r.backend)) {
            return Err(PollError::MixedAuthorities);
        }
        let handles = reads.iter().map(|r| r.handle).collect::<Vec<_>>();
        first.poll(&handles).map_err(PollError::Event)
    }
    pub fn reset(&self) -> Result<(), events::Error> {
        self.backend.reset(self.handle)
    }
    pub fn clear(&self) -> Result<(), events::Error> {
        self.backend.clear(self.handle)
    }
    pub fn ready(&self) -> Result<bool, events::Error> {
        self.backend.poll(&[self.handle]).map(|v| v.is_some())
    }
}
impl WriteEnd {
    pub fn signal(&self) -> Result<(), events::Error> {
        self.backend.signal(self.handle)
    }
    pub fn clear(&self) -> Result<(), events::Error> {
        self.backend.clear(self.handle)
    }
}
#[derive(Clone)]
pub struct Factory {
    backend: Arc<Events>,
}
impl Factory {
    pub fn new(object_limit: usize) -> Result<Self, events::Error> {
        Ok(Self {
            backend: Arc::new(Events::new(object_limit)?),
        })
    }
    pub fn create(&self, table: &Table) -> Result<(Handle, Handle), Error> {
        let ends = self.backend.create()?;
        let w = Arc::new(WriteEnd {
            backend: self.backend.clone(),
            handle: ends.write,
        });
        let r = Arc::new(ReadEnd {
            backend: self.backend.clone(),
            handle: ends.read,
        });
        Ok(table.insert_pair(w, r)?)
    }
}
