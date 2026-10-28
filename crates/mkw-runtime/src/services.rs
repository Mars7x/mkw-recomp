//! Explicit typed kernel-call transport. No Horizon handlers or default successes.
use crate::{
    exclusive_native::Context,
    host_call::{HostCall, HostFault},
    GuestAddr,
};
use std::{collections::BTreeMap, sync::Arc};
pub const EXIT_THREAD: i32 = 9;
const MAX_BYTES: usize = 1024 * 1024;
struct HeapResize {
    memory: Arc<std::sync::Mutex<crate::exclusive::ExclusiveMemory>>,
    heap: crate::exclusive::Heap,
    size: usize,
}
#[derive(Default)]
pub struct Outputs {
    registers: [Option<u64>; 8],
    writes: Vec<(GuestAddr, Vec<u8>)>,
    bytes: usize,
    heap_resize: Option<HeapResize>,
}
impl Outputs {
    pub(crate) fn resize_heap(
        mut self,
        memory: Arc<std::sync::Mutex<crate::exclusive::ExclusiveMemory>>,
        heap: crate::exclusive::Heap,
        size: usize,
    ) -> Result<Self, HostFault> {
        if self.heap_resize.is_some() || !self.writes.is_empty() {
            return Err(HostFault::unsupported());
        }
        self.heap_resize = Some(HeapResize { memory, heap, size });
        Ok(self)
    }
    pub fn write_x(mut self, index: usize, value: u64) -> Result<Self, HostFault> {
        let slot = self
            .registers
            .get_mut(index)
            .ok_or_else(HostFault::unsupported)?;
        if slot.is_some() {
            return Err(HostFault::unsupported());
        }
        *slot = Some(value);
        Ok(self)
    }
    pub fn write_w(self, index: usize, value: u32) -> Result<Self, HostFault> {
        self.write_x(index, u64::from(value))
    }
    pub fn write_memory(mut self, address: GuestAddr, bytes: Vec<u8>) -> Result<Self, HostFault> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(HostFault::unsupported)?;
        if self.heap_resize.is_some() || self.writes.len() >= 32 || total > MAX_BYTES {
            return Err(HostFault::unsupported());
        }
        self.bytes = total;
        self.writes.push((address, bytes));
        Ok(self)
    }
}
pub enum Reply {
    Return(Outputs),
    ThreadExit,
}
pub struct Call<'a> {
    context: &'a Context,
    args: [u64; 8],
}
impl Call<'_> {
    pub(crate) fn matches_owner(
        &self,
        memory: &Arc<std::sync::Mutex<crate::exclusive::ExclusiveMemory>>,
        owner: &crate::exclusive::Owner,
    ) -> bool {
        Arc::ptr_eq(memory, &self.context.memory)
            && self.context.owner.same_identity(owner)
            && memory.lock().is_ok_and(|m| m.validate_owner(owner).is_ok())
    }
    pub(crate) fn matches_memory(
        &self,
        memory: &Arc<std::sync::Mutex<crate::exclusive::ExclusiveMemory>>,
        authority: &Arc<()>,
    ) -> bool {
        Arc::ptr_eq(memory, &self.context.memory)
            && memory.lock().is_ok_and(|m| {
                m.has_authority(authority) && m.validate_owner(&self.context.owner).is_ok()
            })
    }
    pub fn arguments(&self) -> [u64; 8] {
        self.args
    }
    pub fn check_outputs(&self, ranges: &[(GuestAddr, usize)]) -> Result<(), HostFault> {
        self.host().check_outputs(ranges)
    }
    pub fn c_string(&self, address: GuestAddr, limit: usize) -> Result<Vec<u8>, HostFault> {
        self.host().c_string(address, limit)
    }
    pub fn read_bytes(&self, address: GuestAddr, size: usize) -> Result<Vec<u8>, HostFault> {
        if size > MAX_BYTES {
            return Err(HostFault::unsupported());
        }
        let memory = self
            .context
            .memory
            .lock()
            .map_err(|_| HostFault::unsupported())?;
        memory
            .validate_owner(&self.context.owner)
            .map_err(|_| HostFault::unsupported())?;
        let mut bytes = vec![0; size];
        memory
            .read_into(address, &mut bytes)
            .map_err(|_| HostFault { status: 5, address })?;
        Ok(bytes)
    }
    fn host(&self) -> HostCall<'_> {
        let mut x = [0; 31];
        x[..8].copy_from_slice(&self.args);
        HostCall::new(self.context, &x)
    }
}
pub trait Service: Send + Sync {
    /// Bounded, non-unwinding host work. Check output ranges before external
    /// effects. Return memory/register writes as one packet; no mutable CPU or
    /// recursive dispatcher is exposed. External effects are not rolled back.
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault>;
}
pub struct Services {
    handlers: BTreeMap<u16, Arc<dyn Service>>,
}
impl Services {
    pub fn new(entries: Vec<(u16, Arc<dyn Service>)>) -> Result<Self, String> {
        if entries.len() > 256 {
            return Err("service handler quota exceeded".into());
        }
        let mut handlers = BTreeMap::new();
        for (number, handler) in entries {
            if handlers.insert(number, handler).is_some() {
                return Err("duplicate SVC handler".into());
            }
        }
        Ok(Self { handlers })
    }
    /// Combine explicitly configured registries without overriding any handler.
    /// Duplicate numbers or the existing 256-handler quota reject the entire
    /// result. Compose first, then apply the final process allowlist separately.
    pub fn combined(&self, other: &Self) -> Result<Self, String> {
        Self::new(
            self.handlers
                .iter()
                .chain(other.handlers.iter())
                .map(|(&number, handler)| (number, handler.clone()))
                .collect(),
        )
    }
    /// Return a view containing only already registered handlers whose numbers
    /// are explicitly allowed. This can remove capabilities, never add them.
    /// The caller must separately authenticate/authorize any target policy.
    pub fn restricted_to(&self, allowed: &std::collections::BTreeSet<u16>) -> Self {
        Self {
            handlers: self
                .handlers
                .iter()
                .filter(|(number, _)| allowed.contains(number))
                .map(|(&number, handler)| (number, handler.clone()))
                .collect(),
        }
    }
    pub(crate) fn invoke(
        &self,
        context: &Context,
        number: u32,
        registers: &mut [u64; 31],
    ) -> Result<i32, HostFault> {
        let number = u16::try_from(number).map_err(|_| HostFault::unsupported())?;
        let handler = self
            .handlers
            .get(&number)
            .ok_or_else(HostFault::unsupported)?;
        let call = Call {
            context,
            args: registers[..8].try_into().unwrap(),
        };
        let reply = handler.invoke(&call)?;
        let empty = Outputs::default();
        let outputs = match &reply {
            Reply::Return(o) => o,
            Reply::ThreadExit => &empty,
        };
        let writes: Vec<_> = outputs
            .writes
            .iter()
            .map(|(a, b)| (*a, b.as_slice()))
            .collect();
        let mut memory = context
            .memory
            .lock()
            .map_err(|_| HostFault::unsupported())?;
        memory
            .validate_owner(&context.owner)
            .map_err(|_| HostFault::unsupported())?;
        for (address, bytes) in &writes {
            memory
                .validate_access(*address, bytes.len(), true)
                .map_err(|_| HostFault {
                    status: 5,
                    address: *address,
                })?;
        }
        if let Some(effect) = &outputs.heap_resize {
            // Resize and memory-output packets are disjoint. Validate namespace
            // and capability before mutation; no later output range can fault.
            if !writes.is_empty() || !Arc::ptr_eq(&effect.memory, &context.memory) {
                return Err(HostFault::unsupported());
            }
            memory
                .resize_heap(&effect.heap, effect.size)
                .map_err(|_| HostFault::unsupported())?;
        }
        memory
            .write_batch(&writes)
            .map_err(|_| HostFault::unsupported())?;
        // A completed exception return opens the local monitor. Refused/failed
        // packets preserve it, along with every register and output range.
        memory
            .clear(&context.owner)
            .map_err(|_| HostFault::unsupported())?;
        for (index, value) in outputs.registers.iter().enumerate() {
            if let Some(value) = value {
                registers[index] = *value
            }
        }
        Ok(if matches!(reply, Reply::ThreadExit) {
            EXIT_THREAD
        } else {
            0
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_packet_rejects_duplicate_registers_and_unbounded_outputs() {
        assert!(Outputs::default().write_x(8, 1).is_err());
        assert!(Outputs::default()
            .write_x(0, 1)
            .unwrap()
            .write_w(0, 2)
            .is_err());
        assert!(Outputs::default()
            .write_memory(GuestAddr(0), vec![0; MAX_BYTES + 1])
            .is_err());
        let mut o = Outputs::default();
        for _ in 0..32 {
            o = o.write_memory(GuestAddr(0), vec![]).unwrap()
        }
        assert!(o.write_memory(GuestAddr(0), vec![]).is_err());
        let o = Outputs::default()
            .write_w(0, u32::MAX)
            .unwrap()
            .write_x(1, u64::MAX)
            .unwrap();
        assert_eq!(o.registers[0], Some(0xffff_ffff));
        assert_eq!(o.registers[1], Some(u64::MAX));
    }
}
