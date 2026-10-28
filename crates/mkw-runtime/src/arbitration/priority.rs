//! Logical donation/order mechanics. Original implementation; pinned primary
//! semantic references are recorded separately, with no copied kernel code.
use super::{GuestAddr, Policy, State};
impl State {
    pub(super) fn remove_order(&mut self, sequence: u64) {
        self.order.retain(|&s| s != sequence);
    }
    pub(super) fn restore_priority(&mut self, mut actor: u64) {
        loop {
            let Some(identity) = self.actors.get(&actor) else {
                return;
            };
            let mut effective = identity.priorities.base;
            if self.policy == Policy::InheritedPriority {
                for node in self
                    .nodes
                    .values()
                    .filter(|n| n.owner == actor && n.result.is_none())
                {
                    effective = effective.min(self.actors[&node.caller].priorities.effective);
                }
            }
            if effective == identity.priorities.effective {
                return;
            }
            self.actors.get_mut(&actor).unwrap().priorities.effective = effective;
            // A changed waiting actor is removed and reinserted after existing
            // equal-priority waiters. The ticket identity itself never changes.
            let outgoing = self
                .nodes
                .iter()
                .find(|(_, n)| n.caller == actor && n.result.is_none())
                .map(|(&s, n)| (s, n.owner));
            let Some((sequence, owner)) = outgoing else {
                return;
            };
            if self.policy == Policy::Fifo {
                return;
            }
            self.remove_order(sequence);
            self.order.push(sequence);
            actor = owner;
            // prepare_wait rejects cycles; each actor has at most one outgoing
            // edge, so this loop is bounded by the configured actor capacity.
        }
    }
    pub(super) fn select(&self, owner: u64, address: GuestAddr) -> Option<(u64, u64)> {
        if self.policy == Policy::Fifo {
            return self
                .nodes
                .iter()
                .find(|(_, n)| n.owner == owner && n.address == address && n.result.is_none())
                .map(|(&s, n)| (s, n.caller));
        }
        // Iterator order breaks equal-priority ties; do not use the immutable
        // ticket number here, since priority changes cause reinsertion.
        self.order
            .iter()
            .filter_map(|&sequence| {
                let n = &self.nodes[&sequence];
                (n.owner == owner && n.address == address && n.result.is_none())
                    .then_some((sequence, n.caller))
            })
            .min_by_key(|&(_, caller)| self.actors[&caller].priorities.effective)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        arbitration::{Arbitration, Policy, WAITERS},
        exclusive::{Config, ExclusiveMemory},
        exclusive_native::Bridge,
        kernel_handles::Table,
        thread_queries::ThreadIdentity,
        GuestAddr, Memory, Permissions,
    };
    use std::sync::{Arc, Mutex};
    #[test]
    fn identity_exhaustion_cannot_prevent_reordering_cancellation_or_restoration() {
        let mut raw = Memory::default();
        raw.map(GuestAddr(0x2000), vec![0; 4096], Permissions::RW, "locks")
            .unwrap();
        let m = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: 3,
                },
            )
            .unwrap(),
        ));
        let d = Arbitration::with_policy(m.clone(), 3, Policy::InheritedPriority).unwrap();
        let table = Table::new(3).unwrap();
        let actors = [50, 30, 10]
            .into_iter()
            .map(|p| {
                let h = ThreadIdentity::register(&table, p).unwrap();
                d.bind(Bridge::new(m.clone()).unwrap(), &table, h).unwrap()
            })
            .collect::<Vec<_>>();
        m.lock()
            .unwrap()
            .write(
                GuestAddr(0x2000),
                &(actors[0].tag() | WAITERS).to_le_bytes(),
            )
            .unwrap();
        let a = actors[1]
            .prepare_wait(&actors[0], GuestAddr(0x2000))
            .unwrap()
            .unwrap();
        let b = actors[2]
            .prepare_wait(&actors[0], GuestAddr(0x2000))
            .unwrap()
            .unwrap();
        d.state.lock().unwrap().next = u64::MAX;
        actors[1].set_base_priority(5).unwrap();
        assert_eq!(actors[0].priorities().unwrap().effective, 5);
        a.cancel();
        assert_eq!(actors[0].priorities().unwrap().effective, 10);
        b.cancel();
        assert_eq!(actors[0].priorities().unwrap().effective, 50);
        assert_eq!(d.state.lock().unwrap().next, u64::MAX);
        assert_eq!(d.pending(), Ok(0));
    }
}
