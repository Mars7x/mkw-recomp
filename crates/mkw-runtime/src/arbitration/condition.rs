//! Bounded host condition waits. Strong failure atomicity, not a kernel SVC ABI.
use super::{Actor, Error, Node, Phase, Policy, State, Wait, WAITERS};
use crate::{exclusive::Width, GuestAddr};
use std::collections::BTreeMap;

impl State {
    fn condition_next(&self, key: GuestAddr) -> Option<u64> {
        self.order
            .iter()
            .copied()
            .filter(|id| {
                let n = &self.nodes[id];
                n.result.is_none() && n.phase == Phase::Condition(key)
            })
            .min_by_key(|id| self.actors[&self.nodes[id].caller].priorities.effective)
    }
    fn condition_pending(&self, key: GuestAddr) -> bool {
        self.nodes
            .values()
            .any(|n| n.result.is_none() && n.phase == Phase::Condition(key))
    }
}
impl Actor {
    /// Atomically release an owned mutex and register an indefinite condition
    /// wait. Uses the explicit priority policy, aligned distinct words and the
    /// same bounded ticket pool as mutex waits. No clock or SVC result is supplied.
    /// Every refusal leaves guest memory, reservations and queue state unchanged.
    /// A successful signal must reacquire the mutex before this ticket completes.
    pub fn prepare_condition_wait(
        &self,
        address: GuestAddr,
        key: GuestAddr,
    ) -> Result<Wait<'_>, Error> {
        if address.0 % 4 != 0 || key.0 % 4 != 0 {
            return Err(Error::Alignment);
        }
        if address == key {
            return Err(Error::Configuration);
        }
        let d = &self.domain;
        let mut s = d.state.lock().map_err(|_| Error::Poisoned)?;
        let mut m = d.memory.lock().map_err(|_| Error::Poisoned)?;
        d.validate(&s, &m, self.id)?;
        if s.policy != Policy::InheritedPriority {
            return Err(Error::Configuration);
        }
        if s.busy(self.id) {
            return Err(Error::Busy);
        }
        if s.nodes.len() >= d.limit {
            return Err(Error::Capacity);
        }
        // Both writes are admitted before any queue or memory effects.
        for at in [address, key] {
            m.validate_access(at, 4, true).map_err(|_| Error::Memory)?;
        }
        let word = m.read(address, Width::Word).map_err(|_| Error::Memory)? as u32;
        if word & !WAITERS != self.tag {
            return Err(Error::Ownership);
        }
        let mut plan = s.clone();
        let sequence = plan.allocate()?;
        let selected = plan.select(self.id, address);
        let count = plan
            .nodes
            .values()
            .filter(|n| n.result.is_none() && n.owner() == Some(self.id) && n.address == address)
            .count();
        let next = if let Some((_, actor)) = selected {
            d.validate(&plan, &m, actor)?;
            plan.actors[&actor].tag | if count > 1 { WAITERS } else { 0 }
        } else {
            0
        };
        plan.nodes.insert(
            sequence,
            Node {
                caller: self.id,
                phase: Phase::Condition(key),
                address,
                result: None,
            },
        );
        plan.order.push(sequence);
        if let Some((ticket, actor)) = selected {
            for n in plan.nodes.values_mut() {
                if n.result.is_none() && n.owner() == Some(self.id) && n.address == address {
                    n.phase = Phase::Mutex(actor);
                }
            }
            plan.nodes.get_mut(&ticket).unwrap().result = Some(Ok(()));
            plan.remove_order(ticket);
            plan.restore_priority(actor);
        }
        plan.restore_priority(self.id);
        // The same memory guard excludes mapping changes after preflight. Hints
        // become visible before release; queue registration precedes notification.
        m.write(key, &1u32.to_le_bytes())
            .expect("preflighted condition hint");
        m.write(address, &next.to_le_bytes())
            .expect("preflighted condition release");
        *s = plan;
        d.changed.notify_all();
        Ok(Wait {
            actor: self,
            sequence,
        })
    }

    /// Select up to `count` waiters (zero means all), highest current effective
    /// priority first and stable/reinserted order for ties. Each selected waiter
    /// acquires a free mutex or joins its active owner's queue with donation.
    /// Returns the number selected, not the number that completed. This host
    /// policy is not the kernel's mutable tree-iterator broadcast contract.
    /// Unknown owners, cycles and memory faults refuse the entire operation.
    pub fn signal_condition(&self, key: GuestAddr, count: usize) -> Result<usize, Error> {
        if key.0 % 4 != 0 {
            return Err(Error::Alignment);
        }
        let d = &self.domain;
        let mut s = d.state.lock().map_err(|_| Error::Poisoned)?;
        let mut m = d.memory.lock().map_err(|_| Error::Poisoned)?;
        d.validate(&s, &m, self.id)?;
        if s.policy != Policy::InheritedPriority {
            return Err(Error::Configuration);
        }
        if s.busy(self.id) {
            return Err(Error::Busy);
        }
        m.validate_access(key, 4, true).map_err(|_| Error::Memory)?;
        let mut plan = s.clone();
        let mut writes = BTreeMap::<u64, u32>::new();
        let mut selected = 0;
        while count == 0 || selected < count {
            let Some(sequence) = plan.condition_next(key) else {
                break;
            };
            let n = &plan.nodes[&sequence];
            let caller = n.caller;
            let address = n.address;
            d.validate(&plan, &m, caller)?;
            // A word overlay makes subsequent acquisitions in a broadcast see
            // earlier planned ownership, without changing live guest memory.
            m.validate_access(address, 4, true)
                .map_err(|_| Error::Memory)?;
            let word = match writes.get(&address.0) {
                Some(v) => *v,
                None => m.read(address, Width::Word).map_err(|_| Error::Memory)? as u32,
            };
            plan.remove_order(sequence);
            if word == 0 {
                writes.insert(address.0, plan.actors[&caller].tag);
                plan.nodes.get_mut(&sequence).unwrap().result = Some(Ok(()));
            } else {
                let owner = plan
                    .actors
                    .iter()
                    .find(|(_, a)| a.tag == word & !WAITERS)
                    .map(|(&id, _)| id)
                    .ok_or(Error::InvalidThread)?;
                d.validate(&plan, &m, owner)?;
                let mut at = owner;
                loop {
                    if at == caller {
                        return Err(Error::Cycle);
                    }
                    match plan
                        .nodes
                        .values()
                        .find(|n| n.caller == at && n.result.is_none())
                        .and_then(Node::owner)
                    {
                        Some(next) => at = next,
                        None => break,
                    }
                }
                writes.insert(address.0, word | WAITERS);
                plan.nodes.get_mut(&sequence).unwrap().phase = Phase::Mutex(owner);
                plan.order.push(sequence);
                plan.restore_priority(owner);
            }
            selected += 1;
        }
        writes.insert(key.0, u32::from(plan.condition_pending(key)));
        // No fallible operations remain; mappings cannot change under this guard.
        for (address, value) in writes {
            m.write(GuestAddr(address), &value.to_le_bytes())
                .expect("preflighted condition signal");
        }
        *s = plan;
        d.changed.notify_all();
        Ok(selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        arbitration::{Arbitration, Policy},
        exclusive::{Config, ExclusiveMemory},
        exclusive_native::Bridge,
        kernel_handles::Table,
        thread_queries::ThreadIdentity,
        Memory, Permissions,
    };
    use std::sync::{Arc, Mutex};
    fn setup(priorities: &[u8], limit: usize) -> (Arc<Arbitration>, Vec<Actor>) {
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(0x2000),
            vec![0; 4096],
            Permissions::RW,
            "synthetic locks",
        )
        .unwrap();
        raw.map(
            GuestAddr(0x3000),
            vec![0; 4096],
            Permissions::R,
            "synthetic readonly",
        )
        .unwrap();
        let memory = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: priorities.len(),
                },
            )
            .unwrap(),
        ));
        let d = Arbitration::with_policy(memory.clone(), limit, Policy::InheritedPriority).unwrap();
        let t = Table::new(priorities.len()).unwrap();
        let actors = priorities
            .iter()
            .map(|&p| {
                let h = ThreadIdentity::register(&t, p).unwrap();
                d.bind(Bridge::new(memory.clone()).unwrap(), &t, h).unwrap()
            })
            .collect();
        (d, actors)
    }
    fn put(d: &Arbitration, at: u64, value: u32) {
        d.memory
            .lock()
            .unwrap()
            .write(GuestAddr(at), &value.to_le_bytes())
            .unwrap();
    }
    fn get(d: &Arbitration, at: u64) -> u32 {
        d.memory
            .lock()
            .unwrap()
            .read(GuestAddr(at), Width::Word)
            .unwrap() as u32
    }
    #[test]
    fn condition_priority_reinsertion_matches_independent_queue_order() {
        // 3^3 initial priority combinations, each actor changed to each priority.
        for code in 0..27 {
            for changed in 0..3 {
                for replacement in [10, 20, 30] {
                    let priorities = [
                        10 + (code % 3) * 10,
                        10 + (code / 3 % 3) * 10,
                        10 + (code / 9) * 10,
                        40,
                    ];
                    let (d, a) = setup(&priorities, 4);
                    let mut waits = Vec::new();
                    for (i, actor) in a[..3].iter().enumerate() {
                        let address = GuestAddr(0x2000 + i as u64 * 16);
                        put(&d, address.0, actor.tag());
                        waits.push(Some(
                            actor
                                .prepare_condition_wait(address, GuestAddr(0x2080))
                                .unwrap(),
                        ));
                    }
                    a[changed].set_base_priority(replacement).unwrap();
                    let mut order = vec![0, 1, 2];
                    if replacement != priorities[changed] {
                        order.retain(|&v| v != changed);
                        order.push(changed);
                    }
                    let mut current = priorities;
                    current[changed] = replacement;
                    order.sort_by_key(|&i| current[i]);
                    for i in order {
                        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 1), Ok(1));
                        assert_eq!(get(&d, 0x2000 + i as u64 * 16), a[i].tag());
                        assert_eq!(waits[i].take().unwrap().wait(), Ok(()));
                    }
                    assert_eq!((d.pending().unwrap(), get(&d, 0x2080)), (0, 0));
                }
            }
        }
    }
    #[test]
    fn donations_reorder_condition_waiters_and_survive_reacquisition() {
        let (d, a) = setup(&[50, 30, 10, 40], 4);
        put(&d, 0x2000, a[0].tag());
        put(&d, 0x2010, a[1].tag());
        put(&d, 0x2020, a[0].tag() | WAITERS);
        let first = a[0]
            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
            .unwrap();
        let second = a[1]
            .prepare_condition_wait(GuestAddr(0x2010), GuestAddr(0x2080))
            .unwrap();
        let donor = a[2]
            .prepare_wait(&a[0], GuestAddr(0x2020))
            .unwrap()
            .unwrap();
        assert_eq!(a[0].priorities().unwrap().effective, 10);
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 1), Ok(1));
        assert_eq!(first.wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[0].tag());
        assert_eq!(get(&d, 0x2010), 0);
        a[0].unlock(GuestAddr(0x2020)).unwrap();
        assert_eq!(donor.wait(), Ok(()));
        assert_eq!(a[0].priorities().unwrap().effective, 50);
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 0), Ok(1));
        assert_eq!(second.wait(), Ok(()));
    }
    #[test]
    fn broadcast_reacquires_one_shared_mutex_then_transfers_remaining_waiters() {
        let (d, a) = setup(&[30, 10, 20, 50], 4);
        let mut waits = Vec::new();
        for actor in &a[..3] {
            put(&d, 0x2000, actor.tag());
            waits.push(Some(
                actor
                    .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                    .unwrap(),
            ));
        }
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 0), Ok(3));
        assert_eq!(get(&d, 0x2080), 0);
        assert_eq!(get(&d, 0x2000), a[1].tag() | WAITERS);
        assert_eq!(waits[1].take().unwrap().wait(), Ok(()));
        assert_eq!(d.pending(), Ok(2));
        a[1].unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(waits[2].take().unwrap().wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[2].tag() | WAITERS);
        a[2].unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(waits[0].take().unwrap().wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[0].tag());
        assert_eq!(d.pending(), Ok(0));
    }
    #[test]
    fn failed_prepare_and_signal_preserve_memory_queues_tickets_and_monitors() {
        let (d, a) = setup(&[10, 20, 30], 3);
        put(&d, 0x2000, a[0].tag());
        let before = d.state.lock().unwrap().next;
        for (address, key, error) in [
            (0x2001, 0x2080, Error::Alignment),
            (0x2000, 0x2081, Error::Alignment),
            (0x2000, 0x2000, Error::Configuration),
            (0x2000, 0x3000, Error::Memory),
            (0x3000, 0x2080, Error::Memory),
            (0x2010, 0x2080, Error::Ownership),
        ] {
            assert_eq!(
                a[0].prepare_condition_wait(GuestAddr(address), GuestAddr(key))
                    .err(),
                Some(error)
            );
            assert_eq!(get(&d, 0x2000), a[0].tag());
            assert_eq!(d.pending(), Ok(0));
            assert_eq!(d.state.lock().unwrap().next, before);
        }
        let first = a[0]
            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
            .unwrap();
        put(&d, 0x2010, a[1].tag());
        let second = a[1]
            .prepare_condition_wait(GuestAddr(0x2010), GuestAddr(0x2080))
            .unwrap();
        put(&d, 0x2010, 0xdead); // First planned acquisition must not commit when second fails.
        let before = d.state.lock().unwrap().next;
        {
            let mut m = d.memory.lock().unwrap();
            m.load(&a[2].bridge.context().owner, GuestAddr(0x2000), Width::Word)
                .unwrap();
        }
        assert_eq!(
            a[2].signal_condition(GuestAddr(0x2080), 0),
            Err(Error::InvalidThread)
        );
        assert_eq!(
            (get(&d, 0x2000), get(&d, 0x2080), d.pending().unwrap()),
            (0, 1, 2)
        );
        assert_eq!(d.state.lock().unwrap().next, before);
        assert!(d
            .memory
            .lock()
            .unwrap()
            .store_conditional(
                &a[2].bridge.context().owner,
                GuestAddr(0x2000),
                Width::Word,
                0
            )
            .unwrap());
        put(&d, 0x2010, a[1].tag());
        assert_eq!(
            a[2].signal_condition(GuestAddr(0x2080), 0),
            Err(Error::Cycle)
        );
        assert_eq!(
            (get(&d, 0x2000), get(&d, 0x2080), d.pending().unwrap()),
            (0, 1, 2)
        );
        drop((first, second));
        d.state.lock().unwrap().next = u64::MAX;
        put(&d, 0x2000, a[0].tag());
        assert_eq!(
            a[0].prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                .err(),
            Some(Error::Exhausted)
        );
        assert_eq!(get(&d, 0x2000), a[0].tag());
    }
    #[test]
    fn condition_cancellation_owner_loss_shutdown_and_stale_generation_remain_bounded() {
        for mode in 0..5 {
            let (d, mut a) = setup(&[10, 50], 2);
            let signaller = a.pop().unwrap();
            let waiter = &a[0];
            put(&d, 0x2000, waiter.tag());
            let wait = waiter
                .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                .unwrap();
            match mode {
                0 => {
                    let cancel = wait.cancellation();
                    assert!(cancel.cancel().unwrap());
                    assert!(!cancel.cancel().unwrap());
                    assert_eq!(wait.wait(), Err(Error::Cancelled));
                }
                1 => {
                    put(&d, 0x2000, signaller.tag());
                    signaller.signal_condition(GuestAddr(0x2080), 1).unwrap();
                    assert_eq!(signaller.priorities().unwrap().effective, 10);
                    let cancel = wait.cancellation();
                    assert!(cancel.cancel().unwrap());
                    assert_eq!(signaller.priorities().unwrap().effective, 50);
                    assert_eq!(wait.wait(), Err(Error::Cancelled));
                }
                2 => {
                    put(&d, 0x2000, signaller.tag());
                    signaller.signal_condition(GuestAddr(0x2080), 1).unwrap();
                    drop(signaller);
                    assert_eq!(wait.wait(), Err(Error::OwnerGone));
                }
                3 => {
                    d.shutdown().unwrap();
                    assert_eq!(wait.wait(), Err(Error::Shutdown));
                }
                4 => {
                    *d.memory.lock().unwrap() = ExclusiveMemory::new(
                        Memory::default(),
                        Config {
                            granule: 16,
                            max_owners: 2,
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        signaller.signal_condition(GuestAddr(0x2080), 1),
                        Err(Error::Stale)
                    );
                    d.shutdown().unwrap();
                    assert_eq!(wait.wait(), Err(Error::Stale));
                }
                _ => unreachable!(),
            }
            assert_eq!(d.pending(), Ok(0));
            assert!(d.state.lock().unwrap().nodes.is_empty());
        }
    }
    #[test]
    fn condition_tickets_keep_capacity_and_fifo_stays_opt_out() {
        let (d, a) = setup(&[10, 20], 2);
        put(&d, 0x2000, a[0].tag());
        let first = a[0]
            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
            .unwrap();
        a[1].signal_condition(GuestAddr(0x2080), 0).unwrap();
        // Completed-but-unconsumed ticket still occupies one slot.
        let second = a[0]
            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
            .unwrap();
        put(&d, 0x2010, a[1].tag());
        assert_eq!(
            a[1].prepare_condition_wait(GuestAddr(0x2010), GuestAddr(0x2090))
                .err(),
            Some(Error::Capacity)
        );
        assert_eq!(get(&d, 0x2010), a[1].tag());
        drop((first, second));
        d.state.lock().unwrap().policy = Policy::Fifo;
        put(&d, 0x2000, a[0].tag());
        assert_eq!(
            a[0].prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
                .err(),
            Some(Error::Configuration)
        );
        assert_eq!(
            a[1].signal_condition(GuestAddr(0x2080), 0),
            Err(Error::Configuration)
        );
    }
    #[test]
    fn condition_release_transfers_existing_mutex_waiters_before_reacquisition() {
        let (d, a) = setup(&[50, 30, 10, 60], 4);
        put(&d, 0x2000, a[0].tag() | WAITERS);
        let b = a[1]
            .prepare_wait(&a[0], GuestAddr(0x2000))
            .unwrap()
            .unwrap();
        let c = a[2]
            .prepare_wait(&a[0], GuestAddr(0x2000))
            .unwrap()
            .unwrap();
        assert_eq!(a[0].priorities().unwrap().effective, 10);
        assert_eq!(
            a[0].prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x3000))
                .err(),
            Some(Error::Memory)
        );
        assert_eq!(get(&d, 0x2000), a[0].tag() | WAITERS);
        assert_eq!(d.pending(), Ok(2));
        let condition = a[0]
            .prepare_condition_wait(GuestAddr(0x2000), GuestAddr(0x2080))
            .unwrap();
        assert_eq!(c.wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[2].tag() | WAITERS);
        assert_eq!(a[0].priorities().unwrap().effective, 50);
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 0), Ok(1));
        a[2].unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(b.wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[1].tag() | WAITERS);
        a[1].unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(condition.wait(), Ok(()));
        assert_eq!(get(&d, 0x2000), a[0].tag());
    }
    #[test]
    fn signal_memory_fault_after_heap_shrink_is_atomic_and_retryable() {
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(0x2000),
            vec![0; 4096],
            Permissions::RW,
            "condition hints",
        )
        .unwrap();
        let (memory, heap) = ExclusiveMemory::with_heap(
            raw,
            Config {
                granule: 16,
                max_owners: 2,
            },
            crate::exclusive::HeapSpec {
                base: GuestAddr(0x4000),
                capacity: 4096,
            },
        )
        .unwrap();
        let memory = Arc::new(Mutex::new(memory));
        memory.lock().unwrap().resize_heap(&heap, 4096).unwrap();
        let d = Arbitration::with_policy(memory.clone(), 2, Policy::InheritedPriority).unwrap();
        let t = Table::new(2).unwrap();
        let a = [10, 20]
            .into_iter()
            .map(|p| {
                let h = ThreadIdentity::register(&t, p).unwrap();
                d.bind(Bridge::new(memory.clone()).unwrap(), &t, h).unwrap()
            })
            .collect::<Vec<_>>();
        put(&d, 0x4000, a[0].tag());
        let wait = a[0]
            .prepare_condition_wait(GuestAddr(0x4000), GuestAddr(0x2000))
            .unwrap();
        memory.lock().unwrap().resize_heap(&heap, 0).unwrap();
        let next = d.state.lock().unwrap().next;
        memory
            .lock()
            .unwrap()
            .load(&a[1].bridge.context().owner, GuestAddr(0x2000), Width::Word)
            .unwrap();
        assert_eq!(
            a[1].signal_condition(GuestAddr(0x2000), 0),
            Err(Error::Memory)
        );
        assert_eq!(
            (
                get(&d, 0x2000),
                d.pending().unwrap(),
                d.state.lock().unwrap().next
            ),
            (1, 1, next)
        );
        assert!(memory
            .lock()
            .unwrap()
            .store_conditional(
                &a[1].bridge.context().owner,
                GuestAddr(0x2000),
                Width::Word,
                1
            )
            .unwrap());
        memory.lock().unwrap().resize_heap(&heap, 4096).unwrap();
        assert_eq!(a[1].signal_condition(GuestAddr(0x2000), 0), Ok(1));
        assert_eq!(wait.wait(), Ok(()));
        assert_eq!(get(&d, 0x4000), a[0].tag());
    }
    #[test]
    fn signal_reselects_after_donation_to_another_condition_waiter() {
        let (d, a) = setup(&[10, 50, 30, 60], 4);
        let mut waits = Vec::new();
        for (i, actor) in a[..3].iter().enumerate() {
            let address = GuestAddr(0x2000 + i as u64 * 16);
            put(&d, address.0, actor.tag());
            waits.push(Some(
                actor
                    .prepare_condition_wait(address, GuestAddr(0x2080))
                    .unwrap(),
            ));
        }
        // B acquired A's released mutex while continuing to wait on its own
        // condition. Signaling A donates to B and changes the next selection.
        put(&d, 0x2000, a[1].tag());
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 2), Ok(2));
        assert_eq!(a[1].priorities().unwrap().effective, 10);
        assert_eq!(get(&d, 0x2010), a[1].tag());
        assert_eq!(get(&d, 0x2020), 0);
        assert_eq!(get(&d, 0x2080), 1);
        assert_eq!(waits[1].take().unwrap().wait(), Ok(()));
        a[1].unlock(GuestAddr(0x2000)).unwrap();
        assert_eq!(waits[0].take().unwrap().wait(), Ok(()));
        assert_eq!(a[1].priorities().unwrap().effective, 50);
        assert_eq!(a[3].signal_condition(GuestAddr(0x2080), 0), Ok(1));
        assert_eq!(waits[2].take().unwrap().wait(), Ok(()));
    }
}
