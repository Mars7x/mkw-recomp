//! Explicit zero-filled bounded heap service; no default registration or VM manager.
use crate::{
    exclusive::{ExclusiveMemory, Heap},
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services},
};
use std::sync::{Arc, Mutex};

pub const SET_HEAP_SIZE: u16 = 1;
pub const INVALID_SIZE: u32 = (101 << 9) | 1;
pub const OUT_OF_MEMORY: u32 = (104 << 9) | 1;
const ALIGNMENT: u64 = 2 * 1024 * 1024;
const REFERENCE_MAX: u64 = 8 * 1024 * 1024 * 1024;

pub enum Abi {
    /// Pinned NX reference limits, not verified Switch 2 configuration.
    Nx64ZeroedBoundedHeap,
}
pub struct HeapServices {
    memory: Arc<Mutex<ExclusiveMemory>>,
    heap: Heap,
}
impl HeapServices {
    pub fn new(_abi: Abi, memory: Arc<Mutex<ExclusiveMemory>>, heap: Heap) -> Result<Self, String> {
        if heap.spec().capacity as u64 % ALIGNMENT != 0 {
            return Err("heap service capacity must be a multiple of 2 MiB".into());
        }
        memory
            .lock()
            .map_err(|_| "heap memory poisoned")?
            .heap_size(&heap)
            .map_err(|e| format!("heap service capability: {e:?}"))?;
        Ok(Self { memory, heap })
    }
    pub fn registry(self) -> Services {
        Services::new(vec![(SET_HEAP_SIZE, Arc::new(self))]).expect("one heap service")
    }
}
impl Service for HeapServices {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        if !call.matches_memory(&self.memory, self.heap.authority()) {
            return Err(HostFault::unsupported());
        }
        let size = call.arguments()[1];
        let result = if size % ALIGNMENT != 0 || size >= REFERENCE_MAX {
            INVALID_SIZE
        } else if size > self.heap.spec().capacity as u64 {
            OUT_OF_MEMORY
        } else {
            0
        };
        let out = Outputs::default().write_w(0, result)?;
        // The selected reference error ABI preserves X1, including its high bits.
        if result != 0 {
            return Ok(Reply::Return(out));
        }
        Ok(Reply::Return(
            out.write_x(1, self.heap.spec().base.0)?.resize_heap(
                self.memory.clone(),
                self.heap.clone(),
                size as usize,
            )?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        exclusive::{Config, HeapSpec, Width},
        exclusive_native::Bridge,
        GuestAddr, Memory, Permissions,
    };
    fn owned() -> (Arc<Mutex<ExclusiveMemory>>, Heap) {
        let mut m = Memory::default();
        m.map(GuestAddr(0x2000), vec![0; 4096], Permissions::RW, "output")
            .unwrap();
        let (m, h) = ExclusiveMemory::with_heap(
            m,
            Config {
                granule: 16,
                max_owners: 2,
            },
            HeapSpec {
                base: GuestAddr(0x4000),
                capacity: 8192,
            },
        )
        .unwrap();
        (Arc::new(Mutex::new(m)), h)
    }
    #[test]
    fn resize_intents_are_lazy_and_reject_mixed_or_duplicate_effects() {
        let (m, h) = owned();
        let packet = || {
            Outputs::default()
                .resize_heap(m.clone(), h.clone(), 4096)
                .unwrap()
        };
        assert!(packet().write_memory(GuestAddr(0x2000), vec![]).is_err());
        assert!(packet().resize_heap(m.clone(), h.clone(), 8192).is_err());
        assert!(Outputs::default()
            .write_memory(GuestAddr(0x2000), vec![1])
            .unwrap()
            .resize_heap(m.clone(), h.clone(), 4096)
            .is_err());
        drop(packet());
        assert_eq!(m.lock().unwrap().heap_size(&h), Ok(0));
    }
    struct Intent {
        memory: Arc<Mutex<ExclusiveMemory>>,
        heap: Heap,
        size: usize,
    }
    impl Service for Intent {
        fn invoke(&self, _: &Call<'_>) -> Result<Reply, HostFault> {
            Ok(Reply::Return(
                Outputs::default().write_w(0, 99)?.resize_heap(
                    self.memory.clone(),
                    self.heap.clone(),
                    self.size,
                )?,
            ))
        }
    }
    #[test]
    fn final_transaction_checks_fail_without_heap_register_or_monitor_effects() {
        for mode in 0..4 {
            let (m, h) = owned();
            let (other, foreign) = owned();
            let (source, cap, size) = match mode {
                0 => (m.clone(), h.clone(), 4096),
                1 => (other.clone(), foreign.clone(), 4096),
                2 => (m.clone(), foreign, 4096),
                _ => (m.clone(), h.clone(), 8193),
            };
            let bridge = Bridge::new(m.clone()).unwrap();
            let owner = &bridge.context().owner;
            m.lock()
                .unwrap()
                .load(owner, GuestAddr(0x2000), Width::Double)
                .unwrap();
            let services = Services::new(vec![(
                1,
                Arc::new(Intent {
                    memory: source,
                    heap: cap,
                    size,
                }),
            )])
            .unwrap();
            let mut regs = [77; 31];
            let result = services.invoke(bridge.context(), 1, &mut regs);
            if mode == 0 {
                assert_eq!(result.unwrap(), 0);
                assert_eq!(regs[0], 99);
                assert_eq!(m.lock().unwrap().heap_size(&h), Ok(4096));
            } else {
                assert_eq!(result.unwrap_err().status, 3);
                assert_eq!(regs, [77; 31]);
                assert_eq!(m.lock().unwrap().heap_size(&h), Ok(0));
            }
            assert_eq!(
                m.lock()
                    .unwrap()
                    .store_conditional(owner, GuestAddr(0x2000), Width::Double, 11)
                    .unwrap(),
                mode != 0
            );
        }
    }
}
