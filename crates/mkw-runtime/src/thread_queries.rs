//! Owned logical thread identities and an explicit initial-state query profile.
//! No host launch, scheduler, SDK object initialization or default registration.
use crate::{
    host_call::HostFault,
    kernel_handles::{Error, Handle, Table},
    services::{Call, Outputs, Reply, Service, Services},
};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
pub const GET_PRIORITY: u16 = 0x0c;
pub const GET_ID: u16 = 0x25;
pub const INVALID_HANDLE: u32 = (114 << 9) | 1;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub enum Abi {
    Horizon64InitialThreadQueries,
}

/// Host-owned identity for an explicitly registered logical thread. Registration
/// does not start a host/guest thread or initialize an SDK ThreadType object.
pub struct ThreadIdentity {
    id: u64,
    priority: u8,
}
fn allocate_id(ids: &AtomicU64) -> Result<u64, Error> {
    let mut current = ids.load(Ordering::Relaxed);
    loop {
        let next = current.checked_add(1).ok_or(Error::IdentityExhausted)?;
        match ids.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Ok(current),
            Err(actual) => current = actual,
        }
    }
}
impl ThreadIdentity {
    /// Configure the initial query priority explicitly; no process-policy or
    /// host scheduling permission is inferred. IDs are never reused; a failed
    /// handle insertion may consume an ID but publishes no object/handle.
    pub fn register(table: &Table, priority: u8) -> Result<Handle, Error> {
        if priority > 63 {
            return Err(Error::Configuration);
        }
        table.insert(Arc::new(Self {
            id: allocate_id(&NEXT_ID)?,
            priority,
        }))
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn initial_priority(&self) -> u8 {
        self.priority
    }
}

pub struct ThreadQueries {
    table: Arc<Table>,
}
struct Handler {
    table: Arc<Table>,
    number: u16,
}
impl ThreadQueries {
    /// This fixed initial-priority reference profile is not a scheduler or
    /// Switch 2 target authorization. Only explicit real table handles work;
    /// current-thread/current-process pseudo resolution remains unsupported.
    pub fn new(_abi: Abi, table: Arc<Table>) -> Self {
        Self { table }
    }
    pub fn registry(&self) -> Services {
        Services::new(
            [GET_PRIORITY, GET_ID]
                .into_iter()
                .map(|number| {
                    (
                        number,
                        Arc::new(Handler {
                            table: self.table.clone(),
                            number,
                        }) as Arc<dyn Service>,
                    )
                })
                .collect(),
        )
        .expect("two distinct thread query numbers")
    }
}
impl Service for Handler {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let handle = call.arguments()[1] as u32;
        if matches!(handle, 0xffff8000 | 0xffff8001) {
            return Err(HostFault::unsupported());
        }
        let object = match self.table.get::<ThreadIdentity>(Handle::from_raw(handle)) {
            Ok(object) => object,
            Err(Error::InvalidHandle | Error::WrongType) => {
                // Explicit reference ABI: invalid handle clears the value output.
                return Ok(Reply::Return(
                    Outputs::default()
                        .write_w(0, INVALID_HANDLE)?
                        .write_x(1, 0)?,
                ));
            }
            Err(_) => return Err(HostFault::unsupported()),
        };
        let output = Outputs::default().write_w(0, 0)?;
        let output = match self.number {
            GET_PRIORITY => output.write_w(1, u32::from(object.initial_priority()))?,
            GET_ID => output.write_x(1, object.id())?,
            _ => return Err(HostFault::unsupported()),
        };
        Ok(Reply::Return(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wide_identity_survives_the_typed_service_packet() {
        use crate::{
            exclusive::{Config, ExclusiveMemory},
            exclusive_native::{Bridge, CpuV10},
            Memory,
        };
        use std::sync::Mutex;
        let table = Arc::new(Table::new(1).unwrap());
        let value = 0xfedc_ba98_7654_3210;
        let handle = table
            .insert(Arc::new(ThreadIdentity {
                id: value,
                priority: 44,
            }))
            .unwrap();
        let services = ThreadQueries::new(Abi::Horizon64InitialThreadQueries, table).registry();
        let memory = Arc::new(Mutex::new(
            ExclusiveMemory::new(
                Memory::default(),
                Config {
                    granule: 16,
                    max_owners: 1,
                },
            )
            .unwrap(),
        ));
        let mut bridge = Bridge::new(memory).unwrap();
        let mut cpu = CpuV10::default();
        let mut attachment = bridge.attach(&mut cpu).unwrap();
        let (_, context) = attachment.parts();
        let mut registers = [0; 31];
        registers[1] = u64::from(handle.raw());
        assert_eq!(
            services
                .invoke(context, u32::from(GET_ID), &mut registers)
                .unwrap(),
            0
        );
        assert_eq!(&registers[..2], &[0, value]);
    }
    #[test]
    fn identity_allocation_never_wraps_or_reuses_at_exhaustion() {
        let ids = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_id(&ids), Ok(u64::MAX - 1));
        assert_eq!(allocate_id(&ids), Err(Error::IdentityExhausted));
        assert_eq!(allocate_id(&ids), Err(Error::IdentityExhausted));
    }
}
