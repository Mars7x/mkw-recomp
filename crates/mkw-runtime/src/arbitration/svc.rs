//! Explicit owned-actor SVC reference profile. Handler construction has no effects;
//! typed admission/completion owns state->memory ordering, never a public callback.
pub mod condition;
use super::{Actor, Arbitration, Error, Node, Phase, Policy, State, WAITERS};
use crate::{
    exclusive::{ExclusiveMemory, Width},
    exclusive_native::Context,
    host_call::HostFault,
    kernel_handles::{Error as HandleError, Handle, Table},
    services::{Call, Outputs, Reply, Service, Services},
    thread_queries::ThreadIdentity,
    GuestAddr,
};
use std::sync::Arc;
pub const LOCK: u16 = 0x1a;
pub const UNLOCK: u16 = 0x1b;
pub const INVALID_ADDRESS: u32 = (102 << 9) | 1;
pub const INVALID_CURRENT_MEMORY: u32 = (106 << 9) | 1;
pub const INVALID_HANDLE: u32 = (114 << 9) | 1;
/// Ordinary owned memory below 2^39; no shadow stacks, guest clocks, host
/// scheduling or inferred Switch 2 configuration. Unknown lifecycle/tag states
/// refuse. Host teardown never fabricates a kernel termination result.
pub enum Abi {
    Nx64OwnedArbitration39,
}
struct Binding {
    domain: Arc<Arbitration>,
    actor: u64,
    table: Arc<Table>,
}
#[derive(Clone, Copy)]
enum Action {
    Condition(condition::Action),
    Lock {
        handle: u32,
        address: GuestAddr,
        tag: u32,
    },
    Unlock {
        address: GuestAddr,
    },
}
pub(crate) struct Intent {
    binding: Arc<Binding>,
    action: Action,
}
struct Handler {
    binding: Arc<Binding>,
    number: u16,
}
impl Actor {
    pub fn arbitration_services(&self, _abi: Abi, table: Arc<Table>) -> Result<Services, Error> {
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
            [LOCK, UNLOCK]
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
        let action = if self.number == LOCK {
            Action::Lock {
                handle: a[0] as u32,
                address: GuestAddr(a[1]),
                tag: a[2] as u32,
            }
        } else {
            Action::Unlock {
                address: GuestAddr(a[0]),
            }
        };
        Ok(Reply::Return(Outputs::default().arbitrate(Intent {
            binding: self.binding.clone(),
            action,
        })?))
    }
}
fn refuse<T>(_: T) -> HostFault {
    HostFault::unsupported()
}
impl Binding {
    fn validate(&self, s: &State, m: &ExclusiveMemory, context: &Context) -> Result<(), HostFault> {
        self.domain.validate(s, m, self.actor).map_err(refuse)?;
        if s.policy != Policy::InheritedPriority
            || !Arc::ptr_eq(&self.domain.memory, &context.memory)
            || !s.actors[&self.actor].owner.same_identity(&context.owner)
        {
            return Err(HostFault::unsupported());
        }
        Ok(())
    }
    fn finish(
        m: &mut ExclusiveMemory,
        context: &Context,
        registers: &mut [u64; 31],
        result: u32,
    ) -> Result<i32, HostFault> {
        m.clear(&context.owner).map_err(refuse)?;
        registers[0] = u64::from(result);
        Ok(0)
    }
    fn wait(
        &self,
        context: &Context,
        registers: &mut [u64; 31],
        sequence: u64,
    ) -> Result<i32, HostFault> {
        let mut s = self.domain.state.lock().map_err(refuse)?;
        loop {
            if let Some(result) = s
                .nodes
                .get(&sequence)
                .ok_or_else(HostFault::unsupported)?
                .result
            {
                let result = match result {
                    Ok(()) => 0,
                    Err(Error::Memory) => INVALID_CURRENT_MEMORY,
                    Err(Error::InvalidState) => condition::INVALID_STATE,
                    _ => return Err(HostFault::unsupported()),
                };
                let mut m = self.domain.memory.lock().map_err(refuse)?;
                self.validate(&s, &m, context)?;
                return Self::finish(&mut m, context, registers, result);
            }
            s = self.domain.changed.wait(s).map_err(refuse)?;
        }
    }
}
/// Cleanup runs after wait's guards are released, including errors. It does not
/// undo a completed handoff or expose a cancellation token as a kernel result.
struct Pending {
    domain: Arc<Arbitration>,
    sequence: u64,
}
impl Drop for Pending {
    fn drop(&mut self) {
        let mut s = self.domain.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = s.nodes.remove(&self.sequence) {
            s.remove_order(self.sequence);
            if n.result.is_none() {
                if let Some(owner) = n.owner() {
                    s.restore_priority(owner);
                }
            }
        }
    }
}
impl Intent {
    pub(crate) fn execute(
        &self,
        context: &Context,
        registers: &mut [u64; 31],
    ) -> Result<i32, HostFault> {
        if let Action::Condition(action) = self.action {
            return action.execute(&self.binding, context, registers);
        }
        let binding = &self.binding;
        let d = &binding.domain;
        let mut s = d.state.lock().map_err(refuse)?;
        let mut m = d.memory.lock().map_err(refuse)?;
        binding.validate(&s, &m, context)?;
        if s.busy(binding.actor) {
            return Err(HostFault::unsupported());
        }
        let address = match self.action {
            Action::Lock { address, .. } | Action::Unlock { address } => address,
            Action::Condition(_) => unreachable!("delegated condition intent"),
        };
        // This explicit reference subset makes no kernel/high-VA or shadow-stack
        // address-policy claim. Refuse outside the selected ordinary guest window.
        if address.0 >= 1u64 << 39 {
            return Err(HostFault::unsupported());
        }
        if address.0 % 4 != 0 {
            return Binding::finish(&mut m, context, registers, INVALID_ADDRESS);
        }
        match self.action {
            Action::Condition(_) => unreachable!("delegated condition intent"),
            Action::Lock {
                handle,
                address,
                tag,
            } => {
                let word = match m.read(address, Width::Word) {
                    Ok(v) => v as u32,
                    Err(_) => {
                        return Binding::finish(&mut m, context, registers, INVALID_CURRENT_MEMORY)
                    }
                };
                // Kernel reference ordering: changed word completes before an
                // invalid owner handle or unsupported requested tag is inspected.
                if word != handle | WAITERS {
                    return Binding::finish(&mut m, context, registers, 0);
                }
                let thread = match binding
                    .table
                    .get::<ThreadIdentity>(Handle::from_raw(handle))
                {
                    Ok(t) => t,
                    Err(HandleError::InvalidHandle | HandleError::WrongType) => {
                        return Binding::finish(&mut m, context, registers, INVALID_HANDLE)
                    }
                    Err(e) => return Err(refuse(e)),
                };
                let owner = s
                    .actors
                    .iter()
                    .find(|(_, a)| a.thread.id() == thread.id())
                    .map(|(&id, _)| id)
                    .ok_or_else(HostFault::unsupported)?;
                d.validate(&s, &m, owner).map_err(refuse)?;
                if tag != s.actors[&binding.actor].tag {
                    return Err(HostFault::unsupported());
                }
                let mut at = owner;
                loop {
                    if at == binding.actor {
                        return Err(HostFault::unsupported());
                    }
                    match s
                        .nodes
                        .values()
                        .find(|n| n.caller == at && n.result.is_none())
                    {
                        Some(n) => match n.owner() {
                            Some(owner) => at = owner,
                            None => break,
                        },
                        None => break,
                    }
                }
                if s.nodes.len() >= d.limit {
                    return Err(HostFault::unsupported());
                }
                let sequence = s.allocate().map_err(refuse)?;
                s.nodes.insert(
                    sequence,
                    Node {
                        caller: binding.actor,
                        phase: Phase::Mutex(owner),
                        address,
                        result: None,
                    },
                );
                s.order.push(sequence);
                s.restore_priority(owner);
                // Predicate registration precedes releasing locks. Completion may
                // happen before wait() starts; its predicate retains that result.
                drop(m);
                drop(s);
                let _pending = Pending {
                    domain: d.clone(),
                    sequence,
                };
                binding.wait(context, registers, sequence)
            }
            Action::Unlock { address } => {
                let selected = s.select(binding.actor, address);
                let count = s
                    .nodes
                    .values()
                    .filter(|n| {
                        n.owner() == Some(binding.actor)
                            && n.address == address
                            && n.result.is_none()
                    })
                    .count();
                let next = if let Some((_, id)) = selected {
                    d.validate(&s, &m, id).map_err(refuse)?;
                    s.actors[&id].tag | if count > 1 { WAITERS } else { 0 }
                } else {
                    0
                };
                // Reference unlock uses the current owner's queue, not the word's
                // existing tag. A write failure still completes the selected wait
                // with InvalidCurrentMemory; the stronger host API is unchanged.
                let result = if m.write(address, &next.to_le_bytes()).is_ok() {
                    0
                } else {
                    INVALID_CURRENT_MEMORY
                };
                if let Some((sequence, id)) = selected {
                    for n in s.nodes.values_mut() {
                        if n.owner() == Some(binding.actor)
                            && n.address == address
                            && n.result.is_none()
                        {
                            n.phase = Phase::Mutex(id);
                        }
                    }
                    s.nodes.get_mut(&sequence).unwrap().result = Some(if result == 0 {
                        Ok(())
                    } else {
                        Err(Error::Memory)
                    });
                    s.remove_order(sequence);
                    s.restore_priority(binding.actor);
                    s.restore_priority(id);
                    d.changed.notify_all();
                }
                Binding::finish(&mut m, context, registers, result)
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
    fn setup() -> (Arc<Arbitration>, Arc<Table>, Actor, Actor) {
        let mut raw = Memory::default();
        raw.map(GuestAddr(0x2000), vec![0; 4096], Permissions::RW, "data")
            .unwrap();
        let m = Arc::new(std::sync::Mutex::new(
            ExclusiveMemory::new(
                raw,
                Config {
                    granule: 16,
                    max_owners: 8,
                },
            )
            .unwrap(),
        ));
        let d = Arbitration::with_policy(m.clone(), 4, Policy::InheritedPriority).unwrap();
        let t = Arc::new(Table::new(8).unwrap());
        let h = ThreadIdentity::register(&t, 50).unwrap();
        let a = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
        let h = ThreadIdentity::register(&t, 10).unwrap();
        let b = d.bind(Bridge::new(m).unwrap(), &t, h).unwrap();
        (d, t, a, b)
    }
    #[test]
    fn intents_are_lazy_and_reject_every_mixed_output_shape() {
        let (d, t, a, _) = setup();
        let binding = Arc::new(Binding {
            domain: d.clone(),
            actor: a.id,
            table: t,
        });
        let make = || Intent {
            binding: binding.clone(),
            action: Action::Unlock {
                address: GuestAddr(0x2000),
            },
        };
        d.memory
            .lock()
            .unwrap()
            .write(GuestAddr(0x2000), &7u32.to_le_bytes())
            .unwrap();
        let packet = || Outputs::default().arbitrate(make()).unwrap();
        assert!(packet().write_x(0, 7).is_err());
        assert!(packet().write_memory(GuestAddr(0x2000), vec![]).is_err());
        assert!(packet().arbitrate(make()).is_err());
        assert!(Outputs::default()
            .write_w(0, 0)
            .unwrap()
            .arbitrate(make())
            .is_err());
        assert!(Outputs::default()
            .write_memory(GuestAddr(0x2000), vec![])
            .unwrap()
            .arbitrate(make())
            .is_err());
        let (mem, h) = ExclusiveMemory::with_heap(
            Memory::default(),
            Config {
                granule: 16,
                max_owners: 1,
            },
            HeapSpec {
                base: GuestAddr(0x4000),
                capacity: 4096,
            },
        )
        .unwrap();
        let mem = Arc::new(std::sync::Mutex::new(mem));
        assert!(packet().resize_heap(mem.clone(), h.clone(), 0).is_err());
        assert!(Outputs::default()
            .resize_heap(mem, h, 0)
            .unwrap()
            .arbitrate(make())
            .is_err());
        drop(packet());
        assert_eq!(
            d.memory
                .lock()
                .unwrap()
                .read(GuestAddr(0x2000), Width::Word),
            Ok(7)
        );
        assert_eq!(d.pending(), Ok(0));
    }
    #[test]
    fn final_admission_checks_owner_namespace_generation_and_registry_lifetime() {
        let (d, t, a, b) = setup();
        let svc = a
            .arbitration_services(Abi::Nx64OwnedArbitration39, t.clone())
            .unwrap();
        let fifo = Arbitration::new(d.memory.clone(), 1).unwrap();
        let h = ThreadIdentity::register(&t, 44).unwrap();
        let fifo_actor = fifo
            .bind(Bridge::new(d.memory.clone()).unwrap(), &t, h)
            .unwrap();
        assert!(matches!(
            fifo_actor.arbitration_services(Abi::Nx64OwnedArbitration39, t.clone()),
            Err(Error::Configuration)
        ));

        let owner = &b.bridge.context().owner;
        d.memory
            .lock()
            .unwrap()
            .load(owner, GuestAddr(0x2000), Width::Word)
            .unwrap();
        let original = [0x2000u64; 31];
        let mut regs = original;
        assert!(svc
            .invoke(b.bridge.context(), UNLOCK as u32, &mut regs)
            .is_err());
        assert_eq!(regs, original);
        assert!(d
            .memory
            .lock()
            .unwrap()
            .store_conditional(owner, GuestAddr(0x2000), Width::Word, 9)
            .unwrap());
        drop(a);
        let fresh = Bridge::new(d.memory.clone()).unwrap();
        assert!(svc
            .invoke(fresh.context(), UNLOCK as u32, &mut regs)
            .is_err());
        assert_eq!(regs, original);
        let foreign = setup();
        assert!(svc
            .invoke(foreign.2.bridge.context(), UNLOCK as u32, &mut regs)
            .is_err());
        let current = b
            .arbitration_services(Abi::Nx64OwnedArbitration39, t)
            .unwrap();
        *d.memory.lock().unwrap() = ExclusiveMemory::new(
            Memory::default(),
            Config {
                granule: 16,
                max_owners: 2,
            },
        )
        .unwrap();
        assert!(current
            .invoke(b.bridge.context(), UNLOCK as u32, &mut regs)
            .is_err());
        assert_eq!(regs, original);
    }
    #[test]
    fn completed_reference_errors_clear_only_caller_but_refusals_preserve_monitor() {
        let (d, t, a, b) = setup();
        let svc = a
            .arbitration_services(Abi::Nx64OwnedArbitration39, t)
            .unwrap();
        assert_eq!(
            (INVALID_ADDRESS, INVALID_CURRENT_MEMORY, INVALID_HANDLE),
            (0xcc01, 0xd401, 0xe401)
        );
        for (number, args, word, result) in [
            (LOCK, [u32::MAX as u64, 0x2000, 0], 0, 0),
            (LOCK, [u32::MAX as u64, 0x2000, 0], u32::MAX, INVALID_HANDLE),
            (LOCK, [0, 0x2001, 0], 0, INVALID_ADDRESS),
            (LOCK, [0, 0x4000, 0], 0, INVALID_CURRENT_MEMORY),
            (UNLOCK, [0x4000, 0, 0], 0, INVALID_CURRENT_MEMORY),
        ] {
            let mut m = d.memory.lock().unwrap();
            m.write(GuestAddr(0x2000), &word.to_le_bytes()).unwrap();
            m.load(&a.bridge.context().owner, GuestAddr(0x2040), Width::Word)
                .unwrap();
            m.load(&b.bridge.context().owner, GuestAddr(0x2080), Width::Word)
                .unwrap();
            drop(m);
            let mut regs = [0x8877665544332211; 31];
            regs[..3].copy_from_slice(&args);
            let old = regs;
            assert_eq!(
                svc.invoke(a.bridge.context(), number as u32, &mut regs),
                Ok(0)
            );
            assert_eq!(regs[0], result as u64);
            assert_eq!(regs[1..], old[1..]);
            let mut m = d.memory.lock().unwrap();
            assert!(!m
                .store_conditional(&a.bridge.context().owner, GuestAddr(0x2040), Width::Word, 5)
                .unwrap());
            assert!(m
                .store_conditional(&b.bridge.context().owner, GuestAddr(0x2080), Width::Word, 6)
                .unwrap());
        }
        for restricted in [false, true] {
            let mut regs = [7u64; 31];
            regs[0] = 1 << 50;
            let old = regs;
            d.memory
                .lock()
                .unwrap()
                .load(&a.bridge.context().owner, GuestAddr(0x2040), Width::Word)
                .unwrap();
            let denied = svc.restricted_to(&Default::default());
            let selected = if restricted { &denied } else { &svc };
            assert!(selected
                .invoke(a.bridge.context(), UNLOCK as u32, &mut regs)
                .is_err());
            assert_eq!(regs, old);
            assert!(d
                .memory
                .lock()
                .unwrap()
                .store_conditional(&a.bridge.context().owner, GuestAddr(0x2040), Width::Word, 8)
                .unwrap());
        }
    }
    #[test]
    fn kernel_unlock_write_failure_transfers_waiters_and_delivers_reference_error() {
        let t = Arc::new(Table::new(3).unwrap());
        let h = ThreadIdentity::register(&t, 50).unwrap();
        let mut raw = Memory::default();
        raw.map(
            GuestAddr(0x4000),
            (h.raw() | WAITERS).to_le_bytes().to_vec(),
            Permissions::R,
            "readonly mutex",
        )
        .unwrap();
        let m = Arc::new(std::sync::Mutex::new(
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
        let a = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
        let h = ThreadIdentity::register(&t, 30).unwrap();
        let b = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
        let h = ThreadIdentity::register(&t, 10).unwrap();
        let c = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
        let wb = b.prepare_wait(&a, GuestAddr(0x4000)).unwrap().unwrap();
        let wc = c.prepare_wait(&a, GuestAddr(0x4000)).unwrap().unwrap();
        assert_eq!(a.unlock(GuestAddr(0x4000)), Err(Error::Memory));
        assert_eq!(d.pending(), Ok(2));
        let mut regs = [0u64; 31];
        regs[0] = 0x4000;
        let svc = a
            .arbitration_services(Abi::Nx64OwnedArbitration39, t.clone())
            .unwrap();
        assert_eq!(
            svc.invoke(a.bridge.context(), UNLOCK as u32, &mut regs),
            Ok(0)
        );
        assert_eq!(regs[0], INVALID_CURRENT_MEMORY as u64);
        assert_eq!(wc.wait(), Err(Error::Memory));
        assert_eq!(d.pending(), Ok(1));
        assert_eq!(a.priorities().unwrap().effective, 50);
        let svc = c
            .arbitration_services(Abi::Nx64OwnedArbitration39, t)
            .unwrap();
        regs[0] = 0x4000;
        assert_eq!(
            svc.invoke(c.bridge.context(), UNLOCK as u32, &mut regs),
            Ok(0)
        );
        assert_eq!(wb.wait(), Err(Error::Memory));
        assert_eq!(d.pending(), Ok(0));
        assert_eq!(
            m.lock().unwrap().read(GuestAddr(0x4000), Width::Word),
            Ok((a.tag() | WAITERS) as u128)
        );
    }
}
