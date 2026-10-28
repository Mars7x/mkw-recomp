//! Explicit host arbitration over coherent memory. Not a Horizon SVC, host
//! scheduler or target configuration. Priority inheritance requires opt-in.
mod priority;
pub mod svc;
use crate::{
    exclusive::{ExclusiveMemory, Owner, Width},
    exclusive_native::{Attachment, Bridge, CpuV10},
    kernel_handles::{Handle, Table},
    thread_queries::ThreadIdentity,
    GuestAddr,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
};

pub const WAITERS: u32 = 0x4000_0000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    Fifo,
    /// Logical priority donation and ordering only; no host scheduling changes.
    InheritedPriority,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Priorities {
    pub base: u8,
    pub effective: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Namespace,
    Stale,
    InvalidThread,
    Duplicate,
    Capacity,
    Exhausted,
    Alignment,
    Memory,
    Ownership,
    Busy,
    Cycle,
    Cancelled,
    OwnerGone,
    Shutdown,
    Poisoned,
}
struct Identity {
    priorities: Priorities,
    owner: Owner,
    thread: Arc<ThreadIdentity>,
    tag: u32,
}
struct Node {
    caller: u64,
    owner: u64,
    address: GuestAddr,
    result: Option<Result<(), Error>>,
}
struct State {
    policy: Policy,
    order: Vec<u64>,
    closed: bool,
    next: u64,
    actors: BTreeMap<u64, Identity>,
    nodes: BTreeMap<u64, Node>,
}
pub struct Arbitration {
    memory: Arc<Mutex<ExclusiveMemory>>,
    generation: Arc<()>,
    limit: usize,
    state: Mutex<State>,
    changed: Condvar,
}
/// Owns the bridge so dropping this actor cancels its dependents before releasing
/// execution ownership. The pinned thread remains valid after its handle closes.
pub struct Actor {
    domain: Arc<Arbitration>,
    id: u64,
    tag: u32,
    bridge: Bridge,
}
/// Registration is already complete. Drop cancels this ticket without changing
/// guest memory. A waiter borrows its actor, so it cannot outlive that owner.
pub struct Wait<'a> {
    actor: &'a Actor,
    sequence: u64,
}
/// Revocable wait capability; sequence identities are never reused. A completed
/// handoff cannot be rolled back by cancellation.
#[derive(Clone)]
pub struct Cancellation {
    domain: Arc<Arbitration>,
    sequence: u64,
}
impl Cancellation {
    pub fn cancel(&self) -> Result<bool, Error> {
        let mut s = self.domain.state.lock().map_err(|_| Error::Poisoned)?;
        let Some(n) = s.nodes.get_mut(&self.sequence) else {
            return Ok(false);
        };
        if n.result.is_some() {
            return Ok(false);
        }
        n.result = Some(Err(Error::Cancelled));
        let owner = n.owner;
        s.remove_order(self.sequence);
        s.restore_priority(owner);
        self.domain.changed.notify_all();
        Ok(true)
    }
}
impl State {
    fn allocate(&mut self) -> Result<u64, Error> {
        let next = self.next.checked_add(1).ok_or(Error::Exhausted)?;
        let id = self.next;
        self.next = next;
        Ok(id)
    }
    fn busy(&self, actor: u64) -> bool {
        self.nodes
            .values()
            .any(|n| n.caller == actor && n.result.is_none())
    }
}
impl Arbitration {
    pub fn new(memory: Arc<Mutex<ExclusiveMemory>>, limit: usize) -> Result<Arc<Self>, Error> {
        Self::with_policy(memory, limit, Policy::Fifo)
    }
    pub fn with_policy(
        memory: Arc<Mutex<ExclusiveMemory>>,
        limit: usize,
        policy: Policy,
    ) -> Result<Arc<Self>, Error> {
        if limit == 0 || limit > 1024 {
            return Err(Error::Configuration);
        }
        let generation = memory.lock().map_err(|_| Error::Poisoned)?.authority();
        Ok(Arc::new(Self {
            memory,
            generation,
            limit,
            state: Mutex::new(State {
                policy,
                order: Vec::new(),
                closed: false,
                next: 1,
                actors: BTreeMap::new(),
                nodes: BTreeMap::new(),
            }),
            changed: Condvar::new(),
        }))
    }
    /// A bridge and a logical thread may each have only one live binding here.
    /// This consumes the bridge even on error; no raw-pointer ownership escapes.
    pub fn bind(
        self: &Arc<Self>,
        bridge: Bridge,
        table: &Table,
        handle: Handle,
    ) -> Result<Actor, Error> {
        let thread = table
            .get::<ThreadIdentity>(handle)
            .map_err(|_| Error::InvalidThread)?;
        let context = bridge.context();
        if !Arc::ptr_eq(&self.memory, &context.memory) {
            return Err(Error::Namespace);
        }
        let mut state = self.state.lock().map_err(|_| Error::Poisoned)?;
        if state.closed {
            return Err(Error::Shutdown);
        }
        let memory = self.memory.lock().map_err(|_| Error::Poisoned)?;
        if !memory.has_authority(&self.generation) || memory.validate_owner(&context.owner).is_err()
        {
            return Err(Error::Stale);
        }
        if state
            .actors
            .values()
            .any(|a| a.thread.id() == thread.id() || a.owner.same_identity(&context.owner))
        {
            return Err(Error::Duplicate);
        }
        if state.actors.len() >= self.limit {
            return Err(Error::Capacity);
        }
        let id = state.allocate()?;
        state.actors.insert(
            id,
            Identity {
                priorities: Priorities {
                    base: thread.initial_priority(),
                    effective: thread.initial_priority(),
                },
                owner: context.owner.clone(),
                thread,
                tag: handle.raw(),
            },
        );
        drop(memory);
        drop(state);
        Ok(Actor {
            domain: self.clone(),
            id,
            tag: handle.raw(),
            bridge,
        })
    }
    /// Shutdown is explicit, idempotent and wakes every pending ticket. No guest
    /// word is fabricated or cleared on shutdown; no further work is admitted.
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut s = self.state.lock().map_err(|_| Error::Poisoned)?;
        s.closed = true;
        for n in s.nodes.values_mut() {
            if n.result.is_none() {
                n.result = Some(Err(Error::Shutdown));
            }
        }
        s.order.clear();
        for a in s.actors.values_mut() {
            a.priorities.effective = a.priorities.base;
        }
        self.changed.notify_all();
        Ok(())
    }
    pub fn pending(&self) -> Result<usize, Error> {
        Ok(self
            .state
            .lock()
            .map_err(|_| Error::Poisoned)?
            .nodes
            .values()
            .filter(|n| n.result.is_none())
            .count())
    }
    fn validate(&self, s: &State, m: &ExclusiveMemory, id: u64) -> Result<(), Error> {
        if s.closed {
            return Err(Error::Shutdown);
        }
        let a = s.actors.get(&id).ok_or(Error::Stale)?;
        if !m.has_authority(&self.generation) || m.validate_owner(&a.owner).is_err() {
            return Err(Error::Stale);
        }
        Ok(())
    }
}
impl Actor {
    /// Domain-local logical values, independent of the initial query registry.
    pub fn priorities(&self) -> Result<Priorities, Error> {
        let s = self.domain.state.lock().map_err(|_| Error::Poisoned)?;
        let m = self.domain.memory.lock().map_err(|_| Error::Poisoned)?;
        self.domain.validate(&s, &m, self.id)?;
        Ok(s.actors[&self.id].priorities)
    }
    /// Explicit host control, including for a blocked actor. No guest scheduling
    /// permission, termination/pinning policy or host OS priority is inferred.
    pub fn set_base_priority(&self, priority: u8) -> Result<(), Error> {
        if priority > 63 {
            return Err(Error::Configuration);
        }
        let mut s = self.domain.state.lock().map_err(|_| Error::Poisoned)?;
        let m = self.domain.memory.lock().map_err(|_| Error::Poisoned)?;
        self.domain.validate(&s, &m, self.id)?;
        s.actors.get_mut(&self.id).unwrap().priorities.base = priority;
        s.restore_priority(self.id);
        Ok(())
    }
    pub fn tag(&self) -> u32 {
        self.tag
    }
    pub fn attach<'a>(&'a mut self, cpu: &'a mut CpuV10) -> Result<Attachment<'a>, String> {
        {
            let s = self
                .domain
                .state
                .lock()
                .map_err(|_| "arbitration state poisoned")?;
            let m = self
                .domain
                .memory
                .lock()
                .map_err(|_| "arbitration memory poisoned")?;
            self.domain
                .validate(&s, &m, self.id)
                .map_err(|e| format!("arbitration actor: {e:?}"))?;
            if s.busy(self.id) {
                return Err("actor is waiting".into());
            }
        }
        self.bridge.attach(cpu)
    }
    /// Check and register under state->memory locks. Native atomics cannot race
    /// between the predicate read and queue insertion. A changed tag needs no wait.
    /// Uses real actor tags and the explicitly selected ordering policy; no deadlines.
    pub fn prepare_wait(
        &self,
        owner: &Actor,
        address: GuestAddr,
    ) -> Result<Option<Wait<'_>>, Error> {
        if !Arc::ptr_eq(&self.domain, &owner.domain) {
            return Err(Error::Namespace);
        }
        if address.0 % 4 != 0 {
            return Err(Error::Alignment);
        }
        let d = &self.domain;
        let mut s = d.state.lock().map_err(|_| Error::Poisoned)?;
        let m = d.memory.lock().map_err(|_| Error::Poisoned)?;
        d.validate(&s, &m, self.id)?;
        d.validate(&s, &m, owner.id)?;
        if s.busy(self.id) {
            return Err(Error::Busy);
        }
        let word = m.read(address, Width::Word).map_err(|_| Error::Memory)? as u32;
        if word != owner.tag | WAITERS {
            return Ok(None);
        }
        let mut at = owner.id;
        loop {
            if at == self.id {
                return Err(Error::Cycle);
            }
            match s
                .nodes
                .values()
                .find(|n| n.caller == at && n.result.is_none())
            {
                Some(n) => at = n.owner,
                None => break,
            }
        }
        if s.nodes.len() >= d.limit {
            return Err(Error::Capacity);
        }
        let sequence = s.allocate()?;
        s.nodes.insert(
            sequence,
            Node {
                caller: self.id,
                owner: owner.id,
                address,
                result: None,
            },
        );
        if s.policy == Policy::InheritedPriority {
            s.order.push(sequence);
        }
        s.restore_priority(owner.id);
        Ok(Some(Wait {
            actor: self,
            sequence,
        }))
    }
    /// Atomically write the selected actor's tag and transfer remaining waiters.
    /// Stricter host contract: a foreign tag or failed write leaves all state
    /// intact. This is not the kernel's error-delivery ABI.
    pub fn unlock(&self, address: GuestAddr) -> Result<(), Error> {
        if address.0 % 4 != 0 {
            return Err(Error::Alignment);
        }
        let d = &self.domain;
        let mut s = d.state.lock().map_err(|_| Error::Poisoned)?;
        let mut m = d.memory.lock().map_err(|_| Error::Poisoned)?;
        d.validate(&s, &m, self.id)?;
        if s.busy(self.id) {
            return Err(Error::Busy);
        }
        let word = m.read(address, Width::Word).map_err(|_| Error::Memory)? as u32;
        if word & !WAITERS != self.tag {
            return Err(Error::Ownership);
        }
        let selected = s.select(self.id, address);
        let count = s
            .nodes
            .values()
            .filter(|n| n.owner == self.id && n.address == address && n.result.is_none())
            .count();
        let next = if let Some((_, id)) = selected {
            d.validate(&s, &m, id)?;
            s.actors[&id].tag | if count > 1 { WAITERS } else { 0 }
        } else {
            0
        };
        m.write(address, &next.to_le_bytes())
            .map_err(|_| Error::Memory)?;
        if let Some((seq, id)) = selected {
            for n in s.nodes.values_mut() {
                if n.owner == self.id && n.address == address && n.result.is_none() {
                    n.owner = id;
                }
            }
            s.nodes.get_mut(&seq).unwrap().result = Some(Ok(()));
            s.remove_order(seq);
            s.restore_priority(self.id);
            s.restore_priority(id);
        }
        d.changed.notify_all();
        Ok(())
    }
}
impl Drop for Actor {
    fn drop(&mut self) {
        // Recover the poisoned guard only to terminate waits; never mutate guest
        // memory or resume successful work through a poisoned domain.
        let mut s = self.domain.state.lock().unwrap_or_else(|e| e.into_inner());
        s.actors.remove(&self.id);
        let mut affected = Vec::new();
        let mut removed = Vec::new();
        for (&seq, n) in &mut s.nodes {
            if n.result.is_none() && (n.owner == self.id || n.caller == self.id) {
                n.result = Some(Err(Error::OwnerGone));
                affected.push(n.owner);
                removed.push(seq);
            }
        }
        for seq in removed {
            s.remove_order(seq);
        }
        for owner in affected {
            s.restore_priority(owner);
        }
        self.domain.changed.notify_all();
    }
}
impl Wait<'_> {
    pub fn cancellation(&self) -> Cancellation {
        Cancellation {
            domain: self.actor.domain.clone(),
            sequence: self.sequence,
        }
    }
    pub fn wait(self) -> Result<(), Error> {
        let d = &self.actor.domain;
        let mut s = d.state.lock().map_err(|_| Error::Poisoned)?;
        loop {
            if let Some(result) = s.nodes.get(&self.sequence).ok_or(Error::Stale)?.result {
                // Completed ownership transfer is not rolled back if the host
                // later replaces memory. Such replacement refuses continuation.
                let m = d.memory.lock().map_err(|_| Error::Poisoned)?;
                if !m.has_authority(&d.generation) {
                    return Err(Error::Stale);
                }
                return result;
            }
            s = d.changed.wait(s).map_err(|_| Error::Poisoned)?;
        }
    }
    pub fn cancel(self) {} // Drop removes the registered wait without a guest write.
}
impl Drop for Wait<'_> {
    fn drop(&mut self) {
        let mut s = self
            .actor
            .domain
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(n) = s.nodes.remove(&self.sequence) {
            s.remove_order(self.sequence);
            if n.result.is_none() {
                s.restore_priority(n.owner);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{exclusive::Config, Memory, Permissions};
    use std::{sync::mpsc, time::Duration};
    fn memory() -> Arc<Mutex<ExclusiveMemory>> {
        let mut m = Memory::default();
        m.map(GuestAddr(0x2000), vec![0; 4096], Permissions::RW, "locks")
            .unwrap();
        Arc::new(Mutex::new(
            ExclusiveMemory::new(
                m,
                Config {
                    granule: 16,
                    max_owners: 16,
                },
            )
            .unwrap(),
        ))
    }
    fn actor(d: &Arc<Arbitration>, table: &Table) -> Actor {
        let h = ThreadIdentity::register(table, 44).unwrap();
        d.bind(Bridge::new(d.memory.clone()).unwrap(), table, h)
            .unwrap()
    }
    fn word(d: &Arbitration, at: u64, value: u32) {
        d.memory
            .lock()
            .unwrap()
            .write(GuestAddr(at), &value.to_le_bytes())
            .unwrap();
    }
    fn read(d: &Arbitration, at: u64) -> u32 {
        d.memory
            .lock()
            .unwrap()
            .read(GuestAddr(at), Width::Word)
            .unwrap() as u32
    }
    struct Stop(Arc<Arbitration>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.shutdown().unwrap();
        }
    }

    #[test]
    fn fifo_handoff_reparents_remaining_waiters_and_separates_addresses() {
        let d = Arbitration::new(memory(), 8).unwrap();
        let t = Table::new(8).unwrap();
        let a = actor(&d, &t);
        let b = actor(&d, &t);
        let c = actor(&d, &t);
        let other = actor(&d, &t);
        word(&d, 0x2000, a.tag | WAITERS);
        word(&d, 0x2004, a.tag | WAITERS);
        let wb = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        let wc = c.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        let wo = other.prepare_wait(&a, GuestAddr(0x2004)).unwrap().unwrap();
        assert_eq!(d.pending(), Ok(3));
        assert_eq!(b.unlock(GuestAddr(0x2000)), Err(Error::Busy));
        a.unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(read(&d, 0x2000), b.tag | WAITERS);
        // Transfer happened before the host enters wait: notification cannot be lost.
        wb.wait().unwrap();
        assert_eq!(d.pending(), Ok(2));
        assert_eq!(a.unlock(GuestAddr(0x2000)), Err(Error::Ownership));
        drop(a);
        assert_eq!(wo.wait(), Err(Error::OwnerGone)); // wc belongs to b now.
        b.unlock(GuestAddr(0x2000)).unwrap();
        wc.wait().unwrap();
        assert_eq!(read(&d, 0x2000), c.tag);
        c.unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(read(&d, 0x2000), 0);
    }
    #[test]
    fn real_blocking_wait_releases_memory_and_shutdown_wakes_without_guest_write() {
        let d = Arbitration::new(memory(), 4).unwrap();
        let t = Table::new(4).unwrap();
        let a = actor(&d, &t);
        let b = actor(&d, &t);
        word(&d, 0x2000, a.tag | WAITERS);
        let ticket = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        std::thread::scope(|scope| {
            let _stop = Stop(d.clone());
            let (tx, rx) = mpsc::channel();
            scope.spawn(move || tx.send(ticket.wait()).unwrap());
            // Even an unselected/spurious notification does not complete a wait.
            d.changed.notify_all();
            assert!(rx.try_recv().is_err());
            word(&d, 0x2010, 7); // Would deadlock if wait held coherent memory lock.
            d.shutdown().unwrap();
            assert_eq!(
                rx.recv_timeout(Duration::from_secs(5)).unwrap(),
                Err(Error::Shutdown)
            );
        });
        assert_eq!(read(&d, 0x2000), a.tag | WAITERS);
        assert_eq!(d.pending(), Ok(0));
        assert_eq!(a.unlock(GuestAddr(0x2000)), Err(Error::Shutdown));
    }
    #[test]
    fn cancellations_cycles_and_owner_exit_preserve_guest_words() {
        let d = Arbitration::new(memory(), 4).unwrap();
        let t = Table::new(4).unwrap();
        let a = actor(&d, &t);
        let b = actor(&d, &t);
        word(&d, 0x2000, a.tag | WAITERS);
        word(&d, 0x2004, b.tag | WAITERS);
        assert!(matches!(
            a.prepare_wait(&a, GuestAddr(0x2000)),
            Err(Error::Cycle)
        ));
        let w = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        assert!(matches!(
            a.prepare_wait(&b, GuestAddr(0x2004)),
            Err(Error::Cycle)
        ));
        assert!(matches!(
            b.prepare_wait(&a, GuestAddr(0x2000)),
            Err(Error::Busy)
        ));
        w.cancel();
        assert_eq!(d.pending(), Ok(0));
        assert_eq!(read(&d, 0x2000), a.tag | WAITERS);
        let w = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        drop(a);
        assert_eq!(w.wait(), Err(Error::OwnerGone));
        assert_eq!(d.pending(), Ok(0));
    }
    #[test]
    fn bindings_namespace_generation_and_nonreused_sequence_are_checked() {
        let m = memory();
        assert!(matches!(
            Arbitration::new(m.clone(), 0),
            Err(Error::Configuration)
        ));
        assert!(matches!(
            Arbitration::new(m.clone(), 1025),
            Err(Error::Configuration)
        ));
        let d = Arbitration::new(m.clone(), 2).unwrap();
        let t = Table::new(8).unwrap();
        let h = ThreadIdentity::register(&t, 44).unwrap();
        let a = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
        assert!(matches!(
            d.bind(Bridge::new(m.clone()).unwrap(), &t, h),
            Err(Error::Duplicate)
        ));
        assert!(matches!(
            d.bind(Bridge::new(memory()).unwrap(), &t, h),
            Err(Error::Namespace)
        ));
        let b = actor(&d, &t);
        let h2 = ThreadIdentity::register(&t, 44).unwrap();
        assert!(matches!(
            d.bind(Bridge::new(m.clone()).unwrap(), &t, h2),
            Err(Error::Capacity)
        ));
        t.close(h).unwrap();
        word(&d, 0x2000, a.tag | WAITERS); // pinned identity survives close.
        d.state.lock().unwrap().next = u64::MAX;
        assert!(matches!(
            b.prepare_wait(&a, GuestAddr(0x2000)),
            Err(Error::Exhausted)
        ));
        assert_eq!(read(&d, 0x2000), a.tag | WAITERS);
        assert_eq!(d.pending(), Ok(0));
        *m.lock().unwrap() = Arc::try_unwrap(memory()).unwrap().into_inner().unwrap();
        assert_eq!(a.unlock(GuestAddr(0x2000)), Err(Error::Stale));
        assert!(matches!(
            d.bind(Bridge::new(m).unwrap(), &t, h2),
            Err(Error::Stale)
        ));
    }
    #[test]
    fn invalid_access_and_foreign_owner_cannot_consume_a_waiter() {
        let mut m = Memory::default();
        m.map(GuestAddr(0x2000), vec![0; 4096], Permissions::RW, "rw")
            .unwrap();
        // Construct readonly tag once the actor's handle is known.
        let t = Table::new(4).unwrap();
        let h = ThreadIdentity::register(&t, 44).unwrap();
        m.map(
            GuestAddr(0x4000),
            (h.raw() | WAITERS).to_le_bytes().to_vec(),
            Permissions::R,
            "ro",
        )
        .unwrap();
        let m = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                m,
                Config {
                    granule: 16,
                    max_owners: 4,
                },
            )
            .unwrap(),
        ));
        let d = Arbitration::new(m.clone(), 4).unwrap();
        let a = d.bind(Bridge::new(m).unwrap(), &t, h).unwrap();
        let b = actor(&d, &t);
        let c = actor(&d, &t);
        let w = b.prepare_wait(&a, GuestAddr(0x4000)).unwrap().unwrap();
        assert_eq!(a.unlock(GuestAddr(0x4000)), Err(Error::Memory));
        assert_eq!(d.pending(), Ok(1));
        assert_eq!(c.unlock(GuestAddr(0x4000)), Err(Error::Ownership));
        assert_eq!(a.unlock(GuestAddr(0x4001)), Err(Error::Alignment));
        assert!(matches!(
            c.prepare_wait(&a, GuestAddr(0x5000)),
            Err(Error::Memory)
        ));
        assert!(c.prepare_wait(&a, GuestAddr(0x2000)).unwrap().is_none());
        assert_eq!(d.pending(), Ok(1));
        w.cancel();
    }
    #[test]
    fn bounded_completion_slots_and_spurious_notifications_do_not_reuse_tickets() {
        let d = Arbitration::new(memory(), 2).unwrap();
        let t = Table::new(4).unwrap();
        let a = actor(&d, &t);
        let b = actor(&d, &t);
        word(&d, 0x2000, a.tag | WAITERS);
        let first = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        a.unlock(GuestAddr(0x2000)).unwrap();
        word(&d, 0x2000, a.tag | WAITERS);
        let second = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        a.unlock(GuestAddr(0x2000)).unwrap();
        word(&d, 0x2000, a.tag | WAITERS);
        assert!(matches!(
            b.prepare_wait(&a, GuestAddr(0x2000)),
            Err(Error::Capacity)
        ));
        first.wait().unwrap();
        let third = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        d.changed.notify_all();
        assert_eq!(d.pending(), Ok(1));
        second.wait().unwrap();
        assert_eq!(d.pending(), Ok(1));
        third.cancel();
        assert_eq!(d.pending(), Ok(0));
    }
    #[test]
    fn cancellation_and_handoff_races_have_one_winner_and_no_lost_waiters() {
        for _ in 0..64 {
            let d = Arbitration::new(memory(), 2).unwrap();
            let t = Table::new(2).unwrap();
            let a = actor(&d, &t);
            let b = actor(&d, &t);
            word(&d, 0x2000, a.tag | WAITERS);
            let w = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
            let cancel = w.cancellation();
            let cancelled = std::thread::scope(|scope| {
                let gate = Arc::new(std::sync::Barrier::new(2));
                let g = gate.clone();
                let c = cancel.clone();
                let task = scope.spawn(move || {
                    g.wait();
                    c.cancel().unwrap()
                });
                gate.wait();
                a.unlock(GuestAddr(0x2000)).unwrap();
                task.join().unwrap()
            });
            assert_eq!(
                w.wait(),
                if cancelled {
                    Err(Error::Cancelled)
                } else {
                    Ok(())
                }
            );
            assert_eq!(read(&d, 0x2000), if cancelled { 0 } else { b.tag });
            assert!(!cancel.cancel().unwrap());
            assert_eq!(d.pending(), Ok(0));
        }
    }
    #[test]
    fn pending_generation_replacement_cancels_without_touching_new_memory() {
        let m = memory();
        let d = Arbitration::new(m.clone(), 2).unwrap();
        let t = Table::new(2).unwrap();
        let a = actor(&d, &t);
        let b = actor(&d, &t);
        word(&d, 0x2000, a.tag | WAITERS);
        let w = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
        let cancel = w.cancellation();
        *m.lock().unwrap() = Arc::try_unwrap(memory()).unwrap().into_inner().unwrap();
        assert_eq!(a.unlock(GuestAddr(0x2000)), Err(Error::Stale));
        assert!(cancel.cancel().unwrap());
        assert_eq!(w.wait(), Err(Error::Stale));
        assert_eq!(read(&d, 0x2000), 0);
        assert_eq!(d.pending(), Ok(0));
    }
}
