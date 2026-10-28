//! Explicit table-v4 routed source contracts for TRUSTED synthetic native code.
//! Origin queries detect mismatches, not malicious machine code. Original target
//! configuration/identity/execute gates remain independent and unfulfilled.
use crate::{
    exclusive_calls::{OpsV2, OpsV3},
    exclusive_native::{Attachment, Context, CpuV10, Ops},
    services::Services,
    GuestAddr,
};
use libloading::{Library, Symbol};
use mkw_arm64::terminal::{conditional::Effect, contract::Contract};
use std::{cell::Cell, collections::BTreeMap, ffi::c_void, mem::size_of};
pub const VERSION: u32 = 4;
pub type Preflight = unsafe extern "C" fn(*mut c_void, *mut CpuV10, *const u8, u64) -> i32;
#[repr(C)]
pub struct OpsV4 {
    pub base: OpsV3,
    pub preflight: Option<Preflight>,
    pub guard_context: *mut c_void,
}
type Function = unsafe extern "C" fn(*mut CpuV10, *mut u64) -> i32;
pub struct Target<'code> {
    entry: Symbol<'code, Function>,
    contract: Contract,
    origin: [u8; 32],
    base: u64,
    address: GuestAddr,
}
impl<'code> Target<'code> {
    /// # Safety
    /// Library/build/queries must be trusted, reproducibly generated for this exact
    /// source contract/producer and implement v10/table-v4 with reentrant/concurrent-safe helpers and without unwinding, pointer
    /// retention or table/frame/budget corruption. Origin self-report is NOT native
    /// authentication. Retain helpers and libraries. Synthetic code only until the
    /// separate original identity/configuration/execute gates are fulfilled.
    pub unsafe fn load(
        library: &'code Library,
        name: &[u8],
        contract: Contract,
        producer: [u8; 32],
        module_base: u64,
    ) -> Result<Self, String> {
        let n = name.strip_suffix(&[0]).unwrap_or(name);
        if n.is_empty() || n.contains(&0) || module_base % 4096 != 0 {
            return Err("invalid guarded symbol/base".into());
        }
        module_base
            .checked_add(contract.source().entry)
            .and_then(|entry| entry.checked_add(contract.source().bytes as u64))
            .ok_or("guarded source mapping overflow")?;
        for e in contract.effects().iter().chain(contract.claim()) {
            for r in e.requirements() {
                module_base
                    .checked_add(r.thunk().slot())
                    .ok_or("guarded slot mapping overflow")?;
            }
        }
        let address = GuestAddr(
            module_base
                .checked_add(contract.source().entry)
                .ok_or("guarded address overflow")?,
        );
        for (suffix, expected) in [
            (b"_abi_version\0".as_slice(), 10u32),
            (b"_exclusive_version\0".as_slice(), VERSION),
        ] {
            let query = [n, suffix].concat();
            let f = unsafe { library.get::<unsafe extern "C" fn() -> u32>(&query) }
                .map_err(|e| e.to_string())?;
            if unsafe { f() } != expected {
                return Err("guarded native profile mismatch".into());
            }
        }
        let mut origin = [0u8; 32];
        let query = [n, b"_origin_v4\0"].concat();
        let f = unsafe { library.get::<unsafe extern "C" fn(*mut u8)>(&query) }
            .map_err(|e| e.to_string())?;
        unsafe { f(origin.as_mut_ptr()) };
        if origin != contract.origin(producer) {
            return Err("guarded native origin mismatch".into());
        }
        let symbol = [n, b"\0"].concat();
        let entry = unsafe { library.get::<Function>(&symbol) }.map_err(|e| e.to_string())?;
        Ok(Self {
            entry,
            contract,
            origin,
            base: module_base,
            address,
        })
    }
}
pub struct Router<'code> {
    targets: BTreeMap<GuestAddr, Target<'code>>,
    max_depth: u16,
}
impl<'code> Router<'code> {
    pub fn new(targets: Vec<Target<'code>>, max_depth: u16) -> Result<Self, String> {
        if !(1..=128).contains(&max_depth) {
            return Err("guarded depth must be1..128".into());
        }
        let mut map = BTreeMap::new();
        for t in targets {
            if map.insert(t.address, t).is_some() {
                return Err("duplicate guarded target".into());
            }
        }
        for target in map.values() {
            for e in target
                .contract
                .effects()
                .iter()
                .chain(target.contract.alias_next())
            {
                let at = GuestAddr(
                    target
                        .base
                        .checked_add(e.source().entry)
                        .ok_or("guarded dependency overflow")?,
                );
                let provider = map.get(&at).ok_or("missing guarded proof dependency")?;
                if !provider.contract.matches(e) {
                    return Err("guarded dependency source/proof mismatch".into());
                }
            }
        }
        Ok(Self {
            targets: map,
            max_depth,
        })
    }
}
struct CallContext<'a, 'code> {
    router: &'a Router<'code>,
    memory: &'a Context,
    cpu: *mut CpuV10,
    ops: *const Ops,
    prefix: Ops,
    services: &'a Services,
    depth: Cell<u16>,
    active: Cell<Option<GuestAddr>>,
}
pub struct Routed<'a, 'code> {
    cpu: &'a mut CpuV10,
    table: Box<OpsV4>,
    context: Box<CallContext<'a, 'code>>,
    previous: *const Ops,
}
impl<'a, 'code> Routed<'a, 'code> {
    pub fn attach(
        attachment: &'a mut Attachment<'_>,
        router: &'a Router<'code>,
        services: &'a Services,
    ) -> Result<Self, String> {
        attachment.validate()?;
        let (cpu, memory) = attachment.parts();
        let previous = cpu.exclusive;
        // SAFETY: Attachment validated the exact live immutable v1 table.
        let prefix = unsafe { *previous };
        let mut context = Box::new(CallContext {
            router,
            memory,
            cpu,
            ops: std::ptr::null(),
            prefix,
            services,
            depth: Cell::new(0),
            active: Cell::new(None),
        });
        let pointer = std::ptr::addr_of_mut!(*context).cast();
        let mut base = prefix;
        base.version = VERSION;
        base.size = size_of::<OpsV4>() as u32;
        let table = Box::new(OpsV4 {
            base: OpsV3 {
                base: OpsV2 {
                    base,
                    call: Some(call),
                    call_context: pointer,
                },
                svc: Some(svc),
                svc_context: pointer,
            },
            preflight: Some(preflight),
            guard_context: pointer,
        });
        context.ops = &table.base.base.base;
        cpu.exclusive = context.ops;
        Ok(Self {
            cpu,
            table,
            context,
            previous,
        })
    }
    pub fn table(&self) -> &OpsV4 {
        &self.table
    }
    pub fn cpu_mut(&mut self) -> &mut CpuV10 {
        self.cpu
    }
    pub fn run(&mut self, target: GuestAddr, budget: &mut u64) -> Result<i32, String> {
        validate(&self.context, self.cpu)?;
        Ok(route(&self.context, self.cpu, target, budget, None))
    }
}
impl Drop for Routed<'_, '_> {
    fn drop(&mut self) {
        self.cpu.exclusive = self.previous;
    }
}
fn validate(ctx: &CallContext<'_, '_>, cpu: &CpuV10) -> Result<(), String> {
    macro_rules! same {
        ($a:expr,$b:expr) => {
            $a.map(|f| f as usize) == $b.map(|f| f as usize)
        };
    }
    let memory = std::ptr::from_ref(ctx.memory).cast_mut().cast();
    let pointer = std::ptr::from_ref(ctx).cast_mut().cast();
    if !std::ptr::eq(cpu, ctx.cpu)
        || cpu.exclusive != ctx.ops
        || cpu.exclusive_context != memory
        || cpu.base.memory_context != memory
        || cpu.base.call.is_some()
        || !cpu.base.call_context.is_null()
        || !same!(cpu.base.memory, ctx.prefix.memory)
        || !same!(cpu.base.ordered_memory, ctx.prefix.ordered_memory)
        || !same!(cpu.base.atomic_memory, ctx.prefix.atomic_memory)
        || !same!(cpu.base.barrier, ctx.prefix.barrier)
    {
        return Err("invalid guarded coherent frame".into());
    }
    // SAFETY: private context points at the live boxed table retained by Routed.
    let table = unsafe { &*ctx.ops.cast::<OpsV4>() };
    let ops = &table.base.base.base;
    if ops.version != VERSION
        || ops.size != size_of::<OpsV4>() as u32
        || !same!(ops.load, ctx.prefix.load)
        || !same!(ops.store, ctx.prefix.store)
        || !same!(ops.clear, ctx.prefix.clear)
        || !same!(ops.memory, ctx.prefix.memory)
        || !same!(ops.ordered_memory, ctx.prefix.ordered_memory)
        || !same!(ops.atomic_memory, ctx.prefix.atomic_memory)
        || !same!(ops.barrier, ctx.prefix.barrier)
        || table.base.base.call.map(|f| f as usize) != Some(call as *const () as usize)
        || table.base.base.call_context != pointer
        || table.base.svc.map(|f| f as usize) != Some(svc as *const () as usize)
        || table.base.svc_context != pointer
        || table.preflight.map(|f| f as usize) != Some(preflight as *const () as usize)
        || table.guard_context != pointer
    {
        return Err("invalid guarded table identity".into());
    }
    ctx.memory
        .memory
        .lock()
        .map_err(|_| "guarded memory lock poisoned")?
        .validate_owner(&ctx.memory.owner)
        .map_err(|e| format!("guarded memory owner:{e:?}"))
}
fn fault(cpu: &mut CpuV10, status: i32, address: u64) -> i32 {
    cpu.base.fault_status = status;
    cpu.base.fault_address = address;
    status
}
struct Active<'a>(&'a Cell<Option<GuestAddr>>, Option<GuestAddr>);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
struct Depth<'a>(&'a Cell<u16>, u16);
impl Drop for Depth<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}
fn expected(target: &Target<'_>, address: GuestAddr) -> Option<Effect> {
    address
        .0
        .checked_sub(target.base)
        .and_then(|relative| {
            target
                .contract
                .effects()
                .iter()
                .find(|e| e.source().entry == relative)
        })
        .cloned()
}
fn route(
    ctx: &CallContext<'_, '_>,
    cpu: &mut CpuV10,
    start: GuestAddr,
    budget: &mut u64,
    mut premise: Option<Effect>,
) -> i32 {
    let depth = ctx.depth.get();
    if depth >= ctx.router.max_depth {
        return fault(cpu, 6, start.0);
    }
    ctx.depth.set(depth + 1);
    let _depth = Depth(&ctx.depth, depth);
    let saved_base = cpu.base.module_base;
    let mut next = start;
    let result = loop {
        if validate(ctx, cpu).is_err() {
            break fault(cpu, 3, next.0);
        }
        let Some(target) = ctx.router.targets.get(&next) else {
            break fault(cpu, 3, next.0);
        };
        if premise
            .as_ref()
            .is_some_and(|e| !target.contract.matches(e))
        {
            break fault(cpu, 3, next.0);
        }
        cpu.base.module_base = target.base;
        cpu.base.pc = target.address.0;
        let before = *budget;
        let status = {
            let old = ctx.active.replace(Some(target.address));
            let _active = Active(&ctx.active, old);
            // SAFETY: load's trusted source/build/ABI/lifetime contract, preflight and
            // retained coherent attachment/metadata cover this exact registered entry.
            unsafe { (target.entry)(cpu, budget) }
        };
        if validate(ctx, cpu).is_err() {
            break fault(cpu, 3, next.0);
        }
        if status != crate::abi::EXIT_TAIL {
            if status == 0 && (premise.is_some() || target.contract.claim().is_some()) {
                break fault(cpu, 6, next.0);
            }
            break status;
        }
        if *budget >= before {
            break fault(cpu, 6, next.0);
        }
        if *budget == 0 {
            break 2;
        }
        let requested = GuestAddr(cpu.base.pc);
        premise = if let Some(e) = target.contract.alias_next() {
            let Some(address) = target.base.checked_add(e.source().entry) else {
                break fault(cpu, 3, requested.0);
            };
            if requested.0 != address {
                break fault(cpu, 3, requested.0);
            }
            Some(e.clone())
        } else {
            let e = expected(target, requested);
            if target.contract.claim().is_some() && e.is_none() {
                break fault(cpu, 3, requested.0);
            }
            e
        };
        next = requested;
    };
    cpu.base.module_base = saved_base;
    result
}
unsafe fn context<'a>(
    pointer: *mut c_void,
    cpu: *mut CpuV10,
) -> Result<&'a CallContext<'a, 'a>, i32> {
    if pointer.is_null()
        || !pointer.cast::<CallContext<'_, '_>>().is_aligned()
        || cpu.is_null()
        || !cpu.is_aligned()
    {
        return Err(7);
    }
    // SAFETY: callbacks require the live installed context; reject foreign frames
    // before reading the frame or any producer-supplied origin byte pointer.
    let ctx = unsafe { &*pointer.cast::<CallContext<'_, '_>>() };
    if ctx.cpu != cpu {
        return Err(3);
    }
    Ok(ctx)
}
unsafe extern "C" fn preflight(
    pointer: *mut c_void,
    cpu: *mut CpuV10,
    origin: *const u8,
    source_entry: u64,
) -> i32 {
    let ctx = match unsafe { context(pointer, cpu) } {
        Ok(c) => c,
        Err(s) => return s,
    };
    let c = unsafe { &mut *cpu };
    if origin.is_null() || validate(ctx, c).is_err() {
        return 3;
    }
    let Some(active) = ctx.active.get().and_then(|a| ctx.router.targets.get(&a)) else {
        return 3;
    };
    if active.contract.source().entry != source_entry
        || active.base != c.base.module_base
        || active.address.0 != c.base.pc
    {
        return 3;
    }
    // SAFETY: trusted generated v4 entries pass their valid 32-byte constant; no
    // arbitrary/untrusted pointer is supported by this unsafe native ABI.
    let bytes = unsafe { std::slice::from_raw_parts(origin, 32) };
    if bytes != active.origin {
        return 3;
    }
    0
}
unsafe extern "C" fn call(
    pointer: *mut c_void,
    cpu: *mut CpuV10,
    target: u64,
    budget: *mut u64,
) -> i32 {
    let ctx = match unsafe { context(pointer, cpu) } {
        Ok(c) => c,
        Err(s) => return s,
    };
    if budget.is_null() || !budget.is_aligned() {
        return 7;
    }
    let c = unsafe { &mut *cpu };
    if validate(ctx, c).is_err() {
        return 3;
    }
    let Some(active) = ctx.active.get().and_then(|a| ctx.router.targets.get(&a)) else {
        return 3;
    };
    let premise = expected(active, GuestAddr(target));
    route(ctx, c, GuestAddr(target), unsafe { &mut *budget }, premise)
}
unsafe extern "C" fn svc(pointer: *mut c_void, cpu: *mut CpuV10, number: u32) -> i32 {
    let ctx = match unsafe { context(pointer, cpu) } {
        Ok(c) => c,
        Err(s) => return s,
    };
    let c = unsafe { &mut *cpu };
    if validate(ctx, c).is_err() || ctx.active.get().is_none() {
        return 3;
    }
    match ctx.services.invoke(ctx.memory, number, &mut c.base.x) {
        Ok(s) => s,
        Err(f) => fault(c, f.status, f.address.0),
    }
}
