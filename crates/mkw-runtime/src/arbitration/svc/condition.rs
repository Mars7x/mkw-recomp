//! Explicit zero/infinite condition SVC subset, distinct from host atomicity.
use super::*;
use std::collections::BTreeMap;
pub const WAIT: u16 = 0x1c;
pub const SIGNAL: u16 = 0x1d;
pub const TIMED_OUT: u32 = (117 << 9) | 1;
pub const INVALID_STATE: u32 = (125 << 9) | 1;
pub enum Abi {
    /// Ordinary <2^39 addresses, timeout0/-1 and no signal donation path into
    /// pending condition membership. Not a verified retail target profile.
    Nx64OwnedUntimedCondition39,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Wait {
        address: GuestAddr,
        key: GuestAddr,
        tag: u32,
        timeout: i64,
    },
    Signal {
        key: GuestAddr,
        count: i32,
    },
}
struct Handler {
    binding: Arc<Binding>,
    number: u16,
}
impl Actor {
    pub fn condition_services(&self, _abi: Abi, table: Arc<Table>) -> Result<Services, Error> {
        let s = self.domain.state.lock().map_err(|_| Error::Poisoned)?;
        let m = self.domain.memory.lock().map_err(|_| Error::Poisoned)?;
        self.domain.validate(&s, &m, self.id)?;
        if s.policy != Policy::InheritedPriority {
            return Err(Error::Configuration);
        }
        let binding = Arc::new(Binding {
            domain: self.domain.clone(),
            actor: self.id,
            table,
        });
        Services::new(
            [WAIT, SIGNAL]
                .into_iter()
                .map(|number| {
                    (
                        number,
                        Arc::new(Handler {
                            binding: binding.clone(),
                            number,
                        }) as Arc<dyn Service>,
                    )
                })
                .collect(),
        )
        .map_err(|_| Error::Configuration)
    }
}
impl Service for Handler {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        let action = if self.number == WAIT {
            Action::Wait {
                address: GuestAddr(a[0]),
                key: GuestAddr(a[1]),
                tag: a[2] as u32,
                timeout: a[3] as i64,
            }
        } else {
            Action::Signal {
                key: GuestAddr(a[0]),
                count: a[1] as u32 as i32,
            }
        };
        Ok(Reply::Return(Outputs::default().arbitrate(Intent {
            binding: self.binding.clone(),
            action: super::Action::Condition(action),
        })?))
    }
}
impl Action {
    pub(super) fn execute(
        self,
        b: &Binding,
        context: &Context,
        registers: &mut [u64; 31],
    ) -> Result<i32, HostFault> {
        let d = &b.domain;
        let mut s = d.state.lock().map_err(refuse)?;
        let mut m = d.memory.lock().map_err(refuse)?;
        b.validate(&s, &m, context)?;
        if s.busy(b.actor) {
            return Err(HostFault::unsupported());
        }
        let key = match self {
            Self::Wait { key, .. } | Self::Signal { key, .. } => key,
        };
        if key.0 >= 1 << 39 {
            return Err(HostFault::unsupported());
        }
        let key = GuestAddr(key.0 & !3);
        match self {
            Self::Wait {
                address,
                tag,
                timeout,
                ..
            } => {
                if address.0 >= 1 << 39 || !matches!(timeout, 0 | -1) {
                    return Err(HostFault::unsupported());
                }
                if address.0 % 4 != 0 {
                    return Binding::finish(&mut m, context, registers, INVALID_ADDRESS);
                }
                if timeout == -1 && tag != s.actors[&b.actor].tag {
                    return Err(HostFault::unsupported());
                }
                let mut plan = s.clone();
                let ticket = if timeout == -1 {
                    if plan.nodes.len() >= d.limit {
                        return Err(HostFault::unsupported());
                    }
                    Some(plan.allocate().map_err(refuse)?)
                } else {
                    None
                };
                let selected = plan.select(b.actor, address);
                let count = plan
                    .nodes
                    .values()
                    .filter(|n| {
                        n.result.is_none() && n.owner() == Some(b.actor) && n.address == address
                    })
                    .count();
                let value = if let Some((_, actor)) = selected {
                    d.validate(&plan, &m, actor).map_err(refuse)?;
                    plan.actors[&actor].tag | if count > 1 { WAITERS } else { 0 }
                } else {
                    0
                };
                if let Some((sequence, actor)) = selected {
                    for n in plan.nodes.values_mut() {
                        if n.result.is_none() && n.owner() == Some(b.actor) && n.address == address
                        {
                            n.phase = Phase::Mutex(actor);
                        }
                    }
                    // Unlike SVC1b, this successor receives success even if the
                    // later caller's mutex write fails.
                    plan.nodes.get_mut(&sequence).unwrap().result = Some(Ok(()));
                    plan.remove_order(sequence);
                    plan.restore_priority(b.actor);
                    plan.restore_priority(actor);
                }
                let _ = m.write(key, &1u32.to_le_bytes());
                let written = m.write(address, &value.to_le_bytes()).is_ok();
                if written {
                    if let Some(sequence) = ticket {
                        plan.nodes.insert(
                            sequence,
                            Node {
                                caller: b.actor,
                                phase: Phase::Condition(key),
                                address,
                                result: None,
                            },
                        );
                        plan.order.push(sequence);
                    }
                }
                *s = plan;
                d.changed.notify_all();
                if !written {
                    return Binding::finish(&mut m, context, registers, INVALID_CURRENT_MEMORY);
                }
                let Some(sequence) = ticket else {
                    return Binding::finish(&mut m, context, registers, TIMED_OUT);
                };
                drop(m);
                drop(s);
                let _pending = Pending {
                    domain: d.clone(),
                    sequence,
                };
                b.wait(context, registers, sequence)
            }
            Self::Signal { count, .. } => {
                // No remaining condition priority may change under this subset.
                // Planning ensures a refusal has no partial broadcast effects.
                let mut plan = s.clone();
                let mut writes = BTreeMap::<u64, u32>::new();
                let mut selected = 0;
                while count <= 0 || selected < count {
                    let sequence = plan
                        .order
                        .iter()
                        .copied()
                        .filter(|id| {
                            let n = &plan.nodes[id];
                            n.result.is_none() && n.phase == Phase::Condition(key)
                        })
                        .min_by_key(|id| plan.actors[&plan.nodes[id].caller].priorities.effective);
                    let Some(sequence) = sequence else { break };
                    let n = &plan.nodes[&sequence];
                    let caller = n.caller;
                    let address = n.address;
                    d.validate(&plan, &m, caller).map_err(refuse)?;
                    if address.0 >= 1 << 39 || address.0 % 4 != 0 {
                        return Err(HostFault::unsupported());
                    }
                    let word = if m.validate_access(address, 4, true).is_err() {
                        None
                    } else {
                        writes
                            .get(&address.0)
                            .copied()
                            .or_else(|| m.read(address, Width::Word).ok().map(|v| v as u32))
                    };
                    plan.remove_order(sequence);
                    match word {
                        None => {
                            plan.nodes.get_mut(&sequence).unwrap().result = Some(Err(Error::Memory))
                        }
                        Some(0) => {
                            writes.insert(address.0, plan.actors[&caller].tag);
                            plan.nodes.get_mut(&sequence).unwrap().result = Some(Ok(()));
                        }
                        Some(word) => {
                            writes.insert(address.0, word | WAITERS);
                            let thread = match b
                                .table
                                .get::<ThreadIdentity>(Handle::from_raw(word & !WAITERS))
                            {
                                Ok(t) => Some(t),
                                Err(HandleError::InvalidHandle | HandleError::WrongType) => None,
                                Err(e) => return Err(refuse(e)),
                            };
                            if let Some(thread) = thread {
                                let owner = plan
                                    .actors
                                    .iter()
                                    .find(|(_, a)| a.thread.id() == thread.id())
                                    .map(|(&id, _)| id)
                                    .ok_or_else(HostFault::unsupported)?;
                                d.validate(&plan, &m, owner).map_err(refuse)?;
                                let mut at = owner;
                                loop {
                                    if at == caller {
                                        return Err(HostFault::unsupported());
                                    }
                                    match plan
                                        .nodes
                                        .values()
                                        .find(|n| n.caller == at && n.result.is_none())
                                    {
                                        Some(n) => match n.phase {
                                            Phase::Mutex(next) => at = next,
                                            Phase::Condition(_) => {
                                                return Err(HostFault::unsupported())
                                            }
                                        },
                                        None => break,
                                    }
                                }
                                plan.nodes.get_mut(&sequence).unwrap().phase = Phase::Mutex(owner);
                                plan.order.push(sequence);
                                plan.restore_priority(owner);
                            } else {
                                plan.nodes.get_mut(&sequence).unwrap().result =
                                    Some(Err(Error::InvalidState));
                            }
                        }
                    }
                    selected += 1;
                }
                if !plan
                    .nodes
                    .values()
                    .any(|n| n.result.is_none() && n.phase == Phase::Condition(key))
                    && m.validate_access(key, 4, true).is_ok()
                {
                    writes.insert(key.0, 0);
                }
                for (at, value) in writes {
                    m.write(GuestAddr(at), &value.to_le_bytes())
                        .expect("preflighted condition service write");
                }
                *s = plan;
                d.changed.notify_all();
                // Signal has a void reference ABI. Only the local monitor changes.
                m.clear(&context.owner).map_err(refuse)?;
                Ok(0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        exclusive::{Config, HeapSpec},
        exclusive_native::Bridge,
        Memory, Permissions,
    };
    use std::sync::Mutex;
    fn fixture() -> (
        Arc<Arbitration>,
        Arc<Table>,
        Vec<Actor>,
        crate::exclusive::Heap,
    ) {
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(0x2000),
            vec![0; 4096],
            Permissions::RW,
            "synthetic keys",
        )
        .unwrap();
        let (m, heap) = ExclusiveMemory::with_heap(
            raw,
            Config {
                granule: 16,
                max_owners: 4,
            },
            HeapSpec {
                base: GuestAddr(0x4000),
                capacity: 4096,
            },
        )
        .unwrap();
        let m = Arc::new(Mutex::new(m));
        m.lock().unwrap().resize_heap(&heap, 4096).unwrap();
        let d = Arbitration::with_policy(m.clone(), 4, Policy::InheritedPriority).unwrap();
        let t = Arc::new(Table::new(8).unwrap());
        let actors = [10, 30, 50]
            .into_iter()
            .map(|p| {
                let h = ThreadIdentity::register(&t, p).unwrap();
                d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap()
            })
            .collect();
        (d, t, actors, heap)
    }
    fn invoke(
        a: &Actor,
        t: &Arc<Table>,
        action: Action,
        regs: &mut [u64; 31],
    ) -> Result<i32, HostFault> {
        action.execute(
            &Binding {
                domain: a.domain.clone(),
                actor: a.id,
                table: t.clone(),
            },
            a.bridge.context(),
            regs,
        )
    }
    fn put(d: &Arbitration, at: u64, v: u32) {
        d.memory
            .lock()
            .unwrap()
            .write(GuestAddr(at), &v.to_le_bytes())
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
    fn release_write_failure_still_completes_existing_successor_successfully() {
        let (d, t, a, heap) = fixture();
        put(&d, 0x4000, a[0].tag() | WAITERS);
        let wait = a[1]
            .prepare_wait(&a[0], GuestAddr(0x4000))
            .unwrap()
            .unwrap();
        d.memory.lock().unwrap().resize_heap(&heap, 0).unwrap();
        let mut regs = [77; 31];
        assert_eq!(
            invoke(
                &a[0],
                &t,
                Action::Wait {
                    address: GuestAddr(0x4000),
                    key: GuestAddr(0x2003),
                    tag: 0,
                    timeout: 0
                },
                &mut regs
            )
            .unwrap(),
            0
        );
        assert_eq!(regs[0], u64::from(INVALID_CURRENT_MEMORY));
        assert_eq!(regs[1..], [77; 30]);
        assert_eq!(get(&d, 0x2000), 1);
        assert_eq!(wait.wait(), Ok(()));
        assert_eq!(d.pending(), Ok(0));
    }
    #[test]
    fn signal_delivers_handled_faults_without_overwriting_void_registers() {
        for fault in [false, true] {
            let (d, t, a, heap) = fixture();
            put(&d, 0x4000, a[0].tag());
            let wait = a[0]
                .prepare_condition_wait(GuestAddr(0x4000), GuestAddr(0x2000))
                .unwrap();
            if fault {
                d.memory.lock().unwrap().resize_heap(&heap, 0).unwrap();
            } else {
                put(&d, 0x4000, 0x123456);
            }
            {
                let mut m = d.memory.lock().unwrap();
                m.load(&a[2].bridge.context().owner, GuestAddr(0x2040), Width::Word)
                    .unwrap();
                m.load(&a[1].bridge.context().owner, GuestAddr(0x2080), Width::Word)
                    .unwrap();
            }
            let mut regs = [0xdeadbeef00112233; 31];
            invoke(
                &a[2],
                &t,
                Action::Signal {
                    key: GuestAddr(0x2003),
                    count: -1,
                },
                &mut regs,
            )
            .unwrap();
            assert_eq!(regs, [0xdeadbeef00112233; 31]);
            {
                let mut m = d.memory.lock().unwrap();
                assert!(!m
                    .store_conditional(
                        &a[2].bridge.context().owner,
                        GuestAddr(0x2040),
                        Width::Word,
                        1
                    )
                    .unwrap());
                assert!(m
                    .store_conditional(
                        &a[1].bridge.context().owner,
                        GuestAddr(0x2080),
                        Width::Word,
                        1
                    )
                    .unwrap());
            }
            assert_eq!(get(&d, 0x2000), 0);
            assert_eq!(
                wait.wait(),
                Err(if fault {
                    Error::Memory
                } else {
                    Error::InvalidState
                })
            );
            if !fault {
                assert_eq!(get(&d, 0x4000), 0x123456 | WAITERS);
            }
        }
    }
    #[test]
    fn unsupported_broadcast_graphs_refuse_before_any_effects() {
        for kind in 0..3 {
            let (d, t, a, _) = fixture();
            put(&d, 0x4000, a[0].tag());
            put(&d, 0x4010, a[1].tag());
            let first = a[0]
                .prepare_condition_wait(GuestAddr(0x4000), GuestAddr(0x2000))
                .unwrap();
            let second = a[1]
                .prepare_condition_wait(GuestAddr(0x4010), GuestAddr(0x2000))
                .unwrap();
            let owner = match kind {
                0 => a[1].tag(),
                1 => a[1].tag(),
                _ => ThreadIdentity::register(&t, 40).unwrap().raw(),
            };
            // The first free acquisition must roll back when the later one is
            // outside the profile (condition-chain, cycle or valid-unbound owner).
            put(&d, if kind == 0 { 0x4000 } else { 0x4010 }, owner);
            let mut regs = [55; 31];
            let context = a[2].bridge.context();
            d.memory
                .lock()
                .unwrap()
                .load(&context.owner, GuestAddr(0x4000), Width::Word)
                .unwrap();
            assert!(invoke(
                &a[2],
                &t,
                Action::Signal {
                    key: GuestAddr(0x2000),
                    count: 0
                },
                &mut regs
            )
            .is_err());
            assert_eq!(regs, [55; 31]);
            assert_eq!(get(&d, 0x4000), if kind == 0 { owner } else { 0 });
            assert_eq!(get(&d, 0x4010), if kind == 0 { 0 } else { owner });
            assert_eq!(get(&d, 0x2000), 1);
            assert_eq!(d.pending(), Ok(2));
            assert!(d
                .memory
                .lock()
                .unwrap()
                .store_conditional(&context.owner, GuestAddr(0x4000), Width::Word, 0)
                .unwrap());
            drop((first, second));
        }
    }
    #[test]
    fn zero_timeout_ignores_unused_tag_and_ticket_exhaustion_but_checks_identity() {
        let (d, t, a, _) = fixture();
        let mut regs = [88; 31];
        d.state.lock().unwrap().next = u64::MAX;
        let action = Action::Wait {
            address: GuestAddr(0x4000),
            key: GuestAddr(0x2001),
            tag: u32::MAX,
            timeout: 0,
        };
        invoke(&a[0], &t, action, &mut regs).unwrap();
        assert_eq!(regs[0], u64::from(TIMED_OUT));
        assert_eq!(get(&d, 0x4000), 0);
        assert_eq!(d.pending(), Ok(0));
        let bound = Binding {
            domain: d.clone(),
            actor: a[0].id,
            table: t.clone(),
        };
        let mut regs = [99; 31];
        assert!(action
            .execute(&bound, a[1].bridge.context(), &mut regs)
            .is_err());
        assert_eq!(regs, [99; 31]);
        assert!(invoke(
            &a[0],
            &t,
            Action::Wait {
                address: GuestAddr(0x4000),
                key: GuestAddr(0x2000),
                tag: a[0].tag(),
                timeout: -1
            },
            &mut regs
        )
        .is_err());
        assert_eq!(regs, [99; 31]);
        d.shutdown().unwrap();
        assert!(invoke(&a[0], &t, action, &mut regs).is_err());
        assert_eq!(regs, [99; 31]);
    }
}
