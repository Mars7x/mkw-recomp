//! Scoped synthetic translated workers on host threads, not Horizon SVC emulation.
//! Caller-owned stack/TLS/TLR layouts are explicit; original startup stays gated.
//! Lifecycle/ownership design was compared with nxnative thread.c at
//! c8ab8166f68296dc712a4ec13b892535d9d790c6. This Rust adapter does not copy its
//! native ARM64 context machinery, Mach hooks or fixed Switch 1 TLS constants.
use crate::{
    exclusive::ExclusiveMemory,
    exclusive_calls::{Routed, Router},
    exclusive_native::{Bridge, CpuV10},
    GuestAddr,
};
use std::sync::{Arc, Condvar, Mutex};
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub start: GuestAddr,
    pub bytes: usize,
}
impl Range {
    fn end(self) -> Result<u64, String> {
        if self.bytes == 0 || self.bytes > 16 * 1024 * 1024 {
            return Err("worker storage must contain 1..16 MiB".into());
        }
        self.start
            .0
            .checked_add(self.bytes as u64)
            .ok_or_else(|| "worker storage address overflow".into())
    }
    fn contains(self, pointer: GuestAddr) -> Result<bool, String> {
        Ok(pointer.0 >= self.start.0 && pointer.0 < self.end()?)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Storage {
    pub stack: Range,
    pub tls: Range,
    pub tlr: Range,
    pub tpidr_el0: GuestAddr,
    pub tpidrro_el0: GuestAddr,
}
#[derive(Clone, Copy, Debug)]
pub struct Worker {
    pub entry: GuestAddr,
    pub return_pc: GuestAddr,
    pub arguments: [u64; 8],
    pub budget: u64,
    pub storage: Storage,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub workers: usize,
    pub host_stack_bytes: usize,
}
/// Architectural values only: no callback/attachment/context pointers cross a join.
/// This diagnostic snapshot is not a Horizon ThreadContext layout.
#[derive(Clone, Debug)]
pub struct Registers {
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub module_base: u64,
    pub nzcv: [u32; 4],
    pub vectors: [[u64; 2]; 32],
    pub fpcr: u32,
    pub fpsr: u32,
    pub tpidr_el0: u64,
    pub tpidrro_el0: u64,
    pub fault_address: u64,
    pub fault_status: i32,
}
#[derive(Clone, Debug)]
pub struct Outcome {
    pub status: i32,
    pub remaining_budget: u64,
    pub registers: Registers,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Decision {
    Pending,
    Run,
    Abort,
}
struct Launch {
    state: Mutex<Decision>,
    changed: Condvar,
}
impl Launch {
    fn new() -> Self {
        Self {
            state: Mutex::new(Decision::Pending),
            changed: Condvar::new(),
        }
    }
    fn decide(&self, decision: Decision) {
        // Private launch state has no guest callbacks. Even an unexpected host
        // unwind must wake waiters rather than deadlock scope cleanup.
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if *state == Decision::Pending {
            *state = decision;
            self.changed.notify_all();
        }
    }
    fn wait(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "worker launch poisoned")?;
        while *state == Decision::Pending {
            state = self
                .changed
                .wait(state)
                .map_err(|_| "worker launch wait poisoned")?;
        }
        if *state == Decision::Run {
            Ok(())
        } else {
            Err("worker launch aborted before native entry".into())
        }
    }
}
struct StartGuard(Arc<Launch>);
impl Drop for StartGuard {
    fn drop(&mut self) {
        self.0.decide(Decision::Abort);
    }
}
fn validate(
    memory: &ExclusiveMemory,
    router: &Router<'_>,
    workers: &[Worker],
    limits: Limits,
) -> Result<(), String> {
    if limits.workers == 0
        || limits.workers > 64
        || workers.is_empty()
        || workers.len() > limits.workers
        || !(128 * 1024..=64 * 1024 * 1024).contains(&limits.host_stack_bytes)
    {
        return Err("invalid translated worker/host stack quota".into());
    }
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    for worker in workers {
        if router.native_base(worker.entry).is_none() || worker.return_pc.0 % 4 != 0 {
            return Err("worker requires a registered native entry and aligned return PC".into());
        }
        let s = worker.storage;
        if s.stack.start.0 % 16 != 0
            || s.stack.end()? % 16 != 0
            || !s.tls.contains(s.tpidr_el0)?
            || !s.tlr.contains(s.tpidrro_el0)?
        {
            return Err("invalid explicit guest stack/thread-register storage".into());
        }
        for range in [s.stack, s.tls, s.tlr] {
            let end = range.end()?;
            memory
                .validate_access(range.start, range.bytes, false)
                .and_then(|_| memory.validate_access(range.start, range.bytes, true))
                .map_err(|e| format!("worker storage access: {e:?}"))?;
            if ranges
                .iter()
                .any(|&(start, other_end)| range.start.0 < other_end && start < end)
            {
                return Err("worker stack/TLS/TLR storage overlaps".into());
            }
            ranges.push((range.start.0, end));
        }
    }
    Ok(())
}
/// Run registered synthetic workers and join every started host thread. Validate
/// the entire batch and reserve every monitor owner before launch. No worker
/// enters native code unless all host spawns succeed. Faulting batches retain
/// completed memory effects; every worker still joins and releases its owner.
/// Native code retains Target::load's trust/identity/ABI obligations. This does
/// handles a predeclared batch only, not dynamic CreateThread/StartThread,
/// waits or a guest scheduler. It does not allocate SDK TLS, run constructors,
/// emulate priority/affinity, install
/// PAC/FP helpers, supply clocks or authorize original-game execution.
///
/// # Safety
/// Every registered native target reachable by a worker must support concurrent
/// invocation without host data races or non-thread-safe global state, including
/// nested callees/helpers. Target::load's ABI checks alone do not prove this.
/// The caller must also uphold its existing trusted code/identity obligations.
pub unsafe fn run(
    memory: Arc<Mutex<ExclusiveMemory>>,
    router: &Router<'_>,
    workers: Vec<Worker>,
    limits: Limits,
) -> Result<Vec<Outcome>, String> {
    {
        let memory = memory.lock().map_err(|_| "worker memory poisoned")?;
        validate(&memory, router, &workers, limits)?;
    }
    let mut slots = Vec::new();
    for worker in workers {
        slots.push((worker, Bridge::new(memory.clone())?));
    }
    std::thread::scope(|scope| {
        let launch = Arc::new(Launch::new());
        let guard = StartGuard(launch.clone());
        let mut handles = Vec::new();
        let mut failure = None;
        for (index, (worker, bridge)) in slots.into_iter().enumerate() {
            let latch = launch.clone();
            match std::thread::Builder::new()
                .name(format!("mkw-translated-{index}"))
                .stack_size(limits.host_stack_bytes)
                .spawn_scoped(scope, move || {
                    latch.wait()?;
                    execute(router, worker, bridge)
                }) {
                Ok(handle) => handles.push(handle),
                Err(error) => {
                    failure = Some(format!("host worker spawn failed: {error}"));
                    break;
                }
            }
        }
        if failure.is_none() {
            launch.decide(Decision::Run);
        }
        drop(guard); // Abort pending workers after partial host-spawn failure.
        let mut outcomes = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok(outcome)) => outcomes.push(outcome),
                Ok(Err(error)) => {
                    failure.get_or_insert(error);
                }
                Err(_) => {
                    failure.get_or_insert_with(|| "host worker panicked".into());
                }
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(outcomes),
        }
    })
}
fn execute(router: &Router<'_>, worker: Worker, mut bridge: Bridge) -> Result<Outcome, String> {
    let mut cpu = CpuV10::default();
    cpu.base.x[..8].copy_from_slice(&worker.arguments);
    cpu.base.x[30] = worker.return_pc.0;
    cpu.base.sp = worker.storage.stack.end()?;
    cpu.base.module_base = router
        .native_base(worker.entry)
        .ok_or("lost worker target")?;
    cpu.base
        .configure_thread_registers(worker.storage.tpidr_el0.0, worker.storage.tpidrro_el0.0);
    let mut budget = worker.budget;
    let status = {
        let mut attachment = bridge.attach(&mut cpu)?;
        let mut routed = Routed::attach(&mut attachment, router)?;
        routed.run(worker.entry, &mut budget)?
    };
    if status == 0 && cpu.base.pc != worker.return_pc.0 {
        return Err("native worker completed with an invalid return PC".into());
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_abort_on_unwind_wakes_and_joins_every_waiter() {
        let launch = Arc::new(Launch::new());
        let guard = StartGuard(launch.clone());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let launch = launch.clone();
            handles.push(std::thread::spawn(move || launch.wait()));
        }
        let unwind = std::panic::catch_unwind(|| {
            let _guard = guard;
            panic!("author-created coordinator unwind");
        });
        assert!(unwind.is_err());
        for handle in handles {
            assert!(handle.join().unwrap().is_err());
        }
    }
}
