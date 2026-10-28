//! One explicitly configured synthetic guarded entry on the calling host thread.
//! No SDK TLS initializer, bootstrap replacement or original-execution authority.
use crate::{
    exclusive::ExclusiveMemory,
    exclusive_native::{Bridge, CpuV10},
    guarded::{Routed, Router},
    pauth::Backend,
    services::Services,
    thread_runner::{Outcome, Range, Registers, Worker},
};
use std::sync::{Arc, Mutex};

fn end(range: Range) -> Result<u64, String> {
    if range.bytes == 0 || range.bytes > 16 * 1024 * 1024 {
        return Err("guarded entry storage must contain 1..16 MiB".into());
    }
    range
        .start
        .0
        .checked_add(range.bytes as u64)
        .ok_or_else(|| "guarded storage overflow".into())
}

/// Execute one registered trusted synthetic entry with explicit caller-owned
/// storage and an explicitly selected diagnostic PAC backend. Storage contents
/// are preserved, not invented or initialized as an SDK thread structure.
/// Validate every range and entry before reserving a monitor owner or running
/// code. Completed guest effects remain on fault, budget exhaustion or wrong
/// return; callbacks/contexts never escape in the returned architectural snapshot.
///
/// The caller's thread is used; no host stack size, scheduling, affinity,
/// lifecycle, FP backend or actual target configuration is supplied. Native
/// safety and original-execution gates remain those of Target::load and
/// Backend::load. This API does not discharge them.
pub fn run(
    memory: Arc<Mutex<ExclusiveMemory>>,
    router: &Router<'_>,
    services: &Services,
    pac: &Backend<'_>,
    worker: Worker,
) -> Result<Outcome, String> {
    let module_base = router
        .module_base(worker.entry)
        .ok_or("unregistered guarded entry")?;
    if worker.return_pc.0 % 4 != 0 {
        return Err("guarded entry requires a registered target and aligned return PC".into());
    }
    let storage = worker.storage;
    let stack_end = end(storage.stack)?;
    let tls_end = end(storage.tls)?;
    let tlr_end = end(storage.tlr)?;
    if storage.stack.start.0 % 16 != 0
        || stack_end % 16 != 0
        || !(storage.tls.start.0..tls_end).contains(&storage.tpidr_el0.0)
        || !(storage.tlr.start.0..tlr_end).contains(&storage.tpidrro_el0.0)
    {
        return Err("invalid explicit guarded stack/thread-register storage".into());
    }
    let ranges = [
        (storage.stack, stack_end),
        (storage.tls, tls_end),
        (storage.tlr, tlr_end),
    ];
    {
        let state = memory.lock().map_err(|_| "guarded entry memory poisoned")?;
        for (index, (range, upper)) in ranges.iter().enumerate() {
            state
                .validate_access(range.start, range.bytes, false)
                .and_then(|_| state.validate_access(range.start, range.bytes, true))
                .map_err(|e| format!("guarded entry storage access: {e:?}"))?;
            if ranges[..index]
                .iter()
                .any(|(other, other_end)| range.start.0 < *other_end && other.start.0 < *upper)
            {
                return Err("guarded stack/TLS/TLR storage overlaps".into());
            }
        }
    }
    let mut bridge = Bridge::new(memory)?;
    let mut cpu = CpuV10::default();
    cpu.base.x[..8].copy_from_slice(&worker.arguments);
    cpu.base.x[30] = worker.return_pc.0;
    cpu.base.sp = stack_end;
    cpu.base.module_base = module_base;
    cpu.base
        .configure_thread_registers(storage.tpidr_el0.0, storage.tpidrro_el0.0);
    let mut budget = worker.budget;
    let status = {
        let mut auth = pac.attach(&mut cpu);
        let mut attachment = bridge.attach(auth.cpu_mut())?;
        let mut routed = Routed::attach(&mut attachment, router, services)?;
        routed.run(worker.entry, &mut budget)?
    };
    if status == 0 && cpu.base.pc != worker.return_pc.0 {
        return Err("guarded entry returned to an unexpected guest PC".into());
    }
    let c = cpu.base;
    Ok(Outcome {
        status,
        remaining_budget: budget,
        registers: Registers {
            x: c.x,
            sp: c.sp,
            pc: c.pc,
            module_base: c.module_base,
            nzcv: [c.n, c.z, c.c, c.v],
            vectors: c.vregs,
            fpcr: c.fpcr,
            fpsr: c.fpsr,
            tpidr_el0: c.tpidr_el0,
            tpidrro_el0: c.tpidrro_el0,
            fault_address: c.fault_address,
            fault_status: c.fault_status,
        },
    })
}
