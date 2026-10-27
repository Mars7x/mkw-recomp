//! Versioned v10 nested-call routing over a coherent owned memory attachment.
//! The exclusive-v2 table is explicit; ABI-v9 prefix calls remain uninstalled.
use crate::{
    exclusive_entry::Entry,
    exclusive_native::{Attachment, Context, CpuV10, Ops},
    host_call::{HostCall, HostReturn, HostService},
    services::Services,
    GuestAddr,
};
use libloading::Library;
use std::{cell::Cell, collections::BTreeMap, ffi::c_void, mem::size_of, sync::Arc};
pub const EXCLUSIVE_V2_VERSION: u32 = 2;
pub type Call = unsafe extern "C" fn(*mut c_void, *mut CpuV10, u64, *mut u64) -> i32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpsV2 {
    pub base: Ops,
    pub call: Option<Call>,
    pub call_context: *mut c_void,
}
pub const SERVICES_V3_VERSION: u32 = 3;
pub type Svc = unsafe extern "C" fn(*mut c_void, *mut CpuV10, u32) -> i32;
#[repr(C)]
pub struct OpsV3 {
    pub base: OpsV2,
    pub svc: Option<Svc>,
    pub svc_context: *mut c_void,
}
enum Table {
    V2(Box<OpsV2>),
    V3(Box<OpsV3>),
}
impl Table {
    fn base(&self) -> &Ops {
        match self {
            Self::V2(t) => &t.base,
            Self::V3(t) => &t.base.base,
        }
    }
}
enum Body<'code> {
    Native(Entry<'code>),
    Host(Arc<dyn HostService>),
}
pub struct Target<'code> {
    address: GuestAddr,
    module_base: u64,
    body: Body<'code>,
    profile: Option<u32>,
}
impl<'code> Target<'code> {
    /// Explicit named HLE registration only; no automatic SDK replacement.
    pub fn host(
        address: GuestAddr,
        module_base: u64,
        service: Arc<dyn HostService>,
    ) -> Result<Self, String> {
        if address.0 % 4 != 0 || module_base % 4096 != 0 || address.0 < module_base {
            return Err("invalid HLE target address/base".into());
        }
        Ok(Self {
            address,
            module_base,
            body: Body::Host(service),
            profile: None,
        })
    }

    /// # Safety
    /// The trusted entry must implement CPU-v10/exclusive-v2, preserve installed
    /// frame/table/context identities, share budgets and never unwind or retain
    /// pointers. Its version queries must be truthful. Synthetic code only until
    /// the project's original execution identity/configuration gates are met.
    pub unsafe fn load(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
    ) -> Result<Self, String> {
        let entry = unsafe {
            Entry::load_table(library, symbol, address, module_base, EXCLUSIVE_V2_VERSION)
        }?;
        Ok(Self {
            address,
            module_base,
            body: Body::Native(entry),
            profile: Some(EXCLUSIVE_V2_VERSION),
        })
    }
}
impl<'code> Target<'code> {
    /// # Safety
    /// Same trusted code/lifetime/identity obligations as load, with CPU-v10 and
    /// services_v3.h instead of the v2 table. Queries must be truthful; never
    /// unwind/retain frame pointers. Original target execution remains gated.
    pub unsafe fn load_services(
        library: &'code Library,
        symbol: &[u8],
        address: GuestAddr,
        module_base: u64,
    ) -> Result<Self, String> {
        let entry = unsafe {
            Entry::load_table(library, symbol, address, module_base, SERVICES_V3_VERSION)
        }?;
        Ok(Self {
            address,
            module_base,
            body: Body::Native(entry),
            profile: Some(SERVICES_V3_VERSION),
        })
    }
}
pub struct Router<'code> {
    targets: BTreeMap<GuestAddr, Target<'code>>,
    max_depth: u16,
}
impl<'code> Router<'code> {
    pub(crate) fn native_base_profile(&self, address: GuestAddr, profile: u32) -> Option<u64> {
        self.targets
            .get(&address)
            .and_then(|t| (t.profile == Some(profile)).then_some(t.module_base))
    }
    fn supports(&self, profile: u32) -> bool {
        self.targets
            .values()
            .all(|t| t.profile.is_none_or(|p| p == profile))
    }
    pub fn new(targets: Vec<Target<'code>>, max_depth: u16) -> Result<Self, String> {
        if !(1..=128).contains(&max_depth) {
            return Err("exclusive-v2 depth must be 1..=128".into());
        }
        let mut entries = BTreeMap::new();
        let mut profile = None;
        for target in targets {
            if let Some(p) = target.profile {
                if profile.is_some_and(|old| old != p) {
                    return Err("mixed native table profiles".into());
                }
                profile = Some(p);
            }
            if entries.insert(target.address, target).is_some() {
                return Err("duplicate exclusive-v2 target".into());
            }
        }
        Ok(Self {
            targets: entries,
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
    depth: Cell<u16>,
    profile: u32,
    services: Option<&'a Services>,
}
pub struct Routed<'a, 'code> {
    cpu: &'a mut CpuV10,
    table: Table,
    context: Box<CallContext<'a, 'code>>,
    previous: *const Ops,
}
impl<'a, 'code> Routed<'a, 'code> {
    pub fn attach(
        attachment: &'a mut Attachment<'_>,
        router: &'a Router<'code>,
    ) -> Result<Self, String> {
        Self::attach_inner(attachment, router, None)
    }
    pub fn attach_services(
        attachment: &'a mut Attachment<'_>,
        router: &'a Router<'code>,
        services: &'a Services,
    ) -> Result<Self, String> {
        Self::attach_inner(attachment, router, Some(services))
    }
    fn attach_inner(
        attachment: &'a mut Attachment<'_>,
        router: &'a Router<'code>,
        services: Option<&'a Services>,
    ) -> Result<Self, String> {
        let profile = if services.is_some() {
            SERVICES_V3_VERSION
        } else {
            EXCLUSIVE_V2_VERSION
        };
        if !router.supports(profile) {
            return Err("native target/attachment table profile mismatch".into());
        }
        attachment.validate()?;
        let (cpu, memory) = attachment.parts();
        let previous = cpu.exclusive;
        // SAFETY: validate establishes the exact live static v1 table before reading it.
        let prefix = unsafe { *previous };
        let mut context = Box::new(CallContext {
            router,
            memory,
            cpu,
            ops: std::ptr::null(),
            prefix,
            depth: Cell::new(0),
            profile,
            services,
        });
        let mut base = prefix;
        base.version = profile;
        base.size = if services.is_some() {
            size_of::<OpsV3>()
        } else {
            size_of::<OpsV2>()
        } as u32;
        let pointer = std::ptr::addr_of_mut!(*context).cast();
        let calls = OpsV2 {
            base,
            call: Some(call),
            call_context: pointer,
        };
        let table = if services.is_some() {
            Table::V3(Box::new(OpsV3 {
                base: calls,
                svc: Some(svc),
                svc_context: pointer,
            }))
        } else {
            Table::V2(Box::new(calls))
        };
        context.ops = table.base();
        cpu.exclusive = context.ops;
        Ok(Self {
            cpu,
            table,
            context,
            previous,
        })
    }
    pub fn cpu_mut(&mut self) -> &mut CpuV10 {
        self.cpu
    }
    pub fn run(&mut self, target: GuestAddr, budget: &mut u64) -> Result<i32, String> {
        validate(&self.context, self.cpu)?;
        debug_assert!(std::ptr::eq(self.cpu.exclusive, self.table.base()));
        Ok(route(&self.context, self.cpu, target, budget))
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
    let expected = std::ptr::from_ref(ctx.memory).cast_mut().cast();
    if !std::ptr::eq(cpu, ctx.cpu)
        || cpu.exclusive != ctx.ops
        || cpu.exclusive_context != expected
        || cpu.base.memory_context != expected
        || !same!(cpu.base.memory, ctx.prefix.memory)
        || !same!(cpu.base.ordered_memory, ctx.prefix.ordered_memory)
        || !same!(cpu.base.atomic_memory, ctx.prefix.atomic_memory)
        || !same!(cpu.base.barrier, ctx.prefix.barrier)
        || cpu.base.call.is_some()
        || !cpu.base.call_context.is_null()
    {
        return Err("invalid exclusive-v2 coherent frame".into());
    }
    ctx.memory
        .memory
        .lock()
        .map_err(|_| "exclusive memory lock poisoned")?
        .validate_owner(&ctx.memory.owner)
        .map_err(|e| format!("invalid exclusive-v2 owner: {e:?}"))
}
fn route(ctx: &CallContext<'_, '_>, cpu: &mut CpuV10, target: GuestAddr, budget: &mut u64) -> i32 {
    let depth = ctx.depth.get();
    if depth >= ctx.router.max_depth {
        cpu.base.fault_status = 6;
        return 6;
    }
    ctx.depth.set(depth + 1);
    struct DepthGuard<'a>(&'a Cell<u16>, u16);
    impl Drop for DepthGuard<'_> {
        fn drop(&mut self) {
            self.0.set(self.1);
        }
    }
    let _depth_guard = DepthGuard(&ctx.depth, depth);
    let saved_base = cpu.base.module_base;
    let mut next = target;
    let result = loop {
        if validate(ctx, cpu).is_err() {
            cpu.base.fault_status = 3;
            break 3;
        }
        cpu.base.fault_address = next.0;
        let Some(target) = ctx.router.targets.get(&next) else {
            cpu.base.fault_status = 3;
            break 3;
        };
        cpu.base.module_base = target.module_base;
        cpu.base.pc = target.address.0;
        let before = *budget;
        // SAFETY: Target attests both native/table ABIs; Routed retains code/context,
        // exact CPU identity and the coherent memory authority for this nested call.
        let status = match &target.body {
            Body::Native(entry) => unsafe { (entry.function)(cpu, budget) },
            Body::Host(service) => {
                // HLE executes no original instructions. The translated caller
                // charged BL/BLR; refuse an exhausted caller without host work.
                // Bounded host work does not invent a guest instruction count.
                if *budget == 0 {
                    break 2;
                }
                let mut call = HostCall::new(ctx.memory, &cpu.base.x);
                match service.invoke(&mut call) {
                    Ok(result) => {
                        if let HostReturn::U32(value) = result {
                            cpu.base.x[0] = u64::from(value);
                        }
                        cpu.base.pc = cpu.base.x[30];
                        0
                    }
                    Err(fault) => {
                        cpu.base.fault_status = fault.status;
                        cpu.base.fault_address = fault.address.0;
                        fault.status
                    }
                }
            }
        };
        if status != crate::abi::EXIT_TAIL {
            break status;
        }
        if *budget >= before {
            cpu.base.fault_status = 6;
            break 6;
        }
        if *budget == 0 {
            break 2;
        }
        next = GuestAddr(cpu.base.pc);
    };
    cpu.base.module_base = saved_base;
    result
}
unsafe extern "C" fn call(
    context: *mut c_void,
    cpu: *mut CpuV10,
    target: u64,
    budget: *mut u64,
) -> i32 {
    if context.is_null()
        || !context.cast::<CallContext<'_, '_>>().is_aligned()
        || cpu.is_null()
        || !cpu.is_aligned()
        || budget.is_null()
        || !budget.is_aligned()
    {
        return 7;
    }
    // SAFETY: the explicit v2 caller must supply pointers from its live Routed
    // attachment. The frame identity check rejects another valid frame first.
    let ctx = unsafe { &*context.cast::<CallContext<'_, '_>>() };
    if ctx.cpu != cpu {
        return 3;
    }
    let cpu = unsafe { &mut *cpu };
    if validate(ctx, cpu).is_err() {
        cpu.base.fault_status = 3;
        return 3;
    }
    route(ctx, cpu, GuestAddr(target), unsafe { &mut *budget })
}

unsafe extern "C" fn svc(context: *mut c_void, cpu: *mut CpuV10, number: u32) -> i32 {
    if context.is_null()
        || !context.cast::<CallContext<'_, '_>>().is_aligned()
        || cpu.is_null()
        || !cpu.is_aligned()
    {
        return 7;
    }
    // SAFETY: v3 ABI requires the live installed context and a valid frame. Frame
    // identity is checked before dereferencing a foreign CPU argument.
    let ctx = unsafe { &*context.cast::<CallContext<'_, '_>>() };
    if ctx.cpu != cpu || ctx.profile != SERVICES_V3_VERSION {
        return 3;
    }
    let cpu = unsafe { &mut *cpu };
    if validate(ctx, cpu).is_err() {
        cpu.base.fault_status = 3;
        return 3;
    }
    let Some(services) = ctx.services else {
        cpu.base.fault_status = 3;
        return 3;
    };
    match services.invoke(ctx.memory, number, &mut cpu.base.x) {
        Ok(status) => status,
        Err(fault) => {
            cpu.base.fault_status = fault.status;
            cpu.base.fault_address = fault.address.0;
            fault.status
        }
    }
}
