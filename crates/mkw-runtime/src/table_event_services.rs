//! Explicit event-only reference handlers over owned object tables. No CreateEvent
//! or IPC reply is installed; actual producers must supply real endpoint objects.
use crate::{
    event_services::{
        Abi, CLEAR, CLOSE, INVALID_HANDLE, INVALID_STATE, OUT_OF_RANGE, RESET, SIGNAL, TIMED_OUT,
        WAIT,
    },
    events,
    host_call::HostFault,
    kernel_handles::{Error, Handle, Table},
    owned_events::{PollError, ReadEnd, WriteEnd},
    services::{Call, Outputs, Reply, Service, Services},
    GuestAddr,
};
use std::sync::Arc;
pub struct TableEventServices {
    table: Arc<Table>,
}
struct Handler {
    table: Arc<Table>,
    number: u16,
}
impl TableEventServices {
    /// Reference selection is not target authorization. The caller must separately
    /// authenticate/restrict process policy and establish handle producers.
    pub fn new(_abi: Abi, table: Arc<Table>) -> Self {
        Self { table }
    }
    pub fn registry(&self) -> Services {
        Services::new(
            [SIGNAL, CLEAR, RESET, CLOSE, WAIT]
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
        .expect("distinct owned event SVC numbers")
    }
}
fn result(value: u32) -> Result<Reply, HostFault> {
    Ok(Reply::Return(Outputs::default().write_w(0, value)?))
}
fn table_error(error: Error) -> Result<Reply, HostFault> {
    match error {
        Error::InvalidHandle | Error::WrongType => result(INVALID_HANDLE),
        _ => Err(HostFault::unsupported()),
    }
}
fn event_result(value: Result<(), events::Error>) -> Result<Reply, HostFault> {
    match value {
        Ok(()) => result(0),
        Err(events::Error::AlreadyClear) => result(INVALID_STATE),
        Err(events::Error::InvalidHandle) => result(INVALID_HANDLE),
        Err(_) => Err(HostFault::unsupported()),
    }
}
impl Service for Handler {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        let h = Handle::from_raw(a[0] as u32);
        match self.number {
            WAIT => self.poll(call),
            CLOSE => match self.table.close(h) {
                Ok(()) => result(0),
                Err(e) => table_error(e),
            },
            SIGNAL => match self.table.get::<WriteEnd>(h) {
                Ok(w) => event_result(w.signal()),
                Err(e) => table_error(e),
            },
            RESET => {
                if h.raw() == 0xffff8001 {
                    return Err(HostFault::unsupported());
                }
                match self.table.get::<ReadEnd>(h) {
                    Ok(r) => event_result(r.reset()),
                    Err(e) => table_error(e),
                }
            }
            CLEAR => match self.table.get::<ReadEnd>(h) {
                Ok(r) => event_result(r.clear()),
                Err(Error::WrongType) => match self.table.get::<WriteEnd>(h) {
                    Ok(w) => event_result(w.clear()),
                    Err(e) => table_error(e),
                },
                Err(e) => table_error(e),
            },
            _ => Err(HostFault::unsupported()),
        }
    }
}
impl Handler {
    fn poll(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        if a[3] != 0 {
            return Err(HostFault::unsupported());
        }
        let count = a[2] as u32 as i32;
        if !(0..=64).contains(&count) {
            return result(OUT_OF_RANGE);
        }
        if count == 0 {
            return result(TIMED_OUT);
        }
        let bytes = call.read_bytes(GuestAddr(a[1]), count as usize * 4)?;
        let handles = bytes
            .chunks_exact(4)
            .map(|b| Handle::from_raw(u32::from_le_bytes(b.try_into().unwrap())))
            .collect::<Vec<_>>();
        let reads = match self.table.get_many::<ReadEnd>(&handles) {
            Ok(r) => r,
            Err(e) => return table_error(e),
        };
        match ReadEnd::poll_same_authority(&reads) {
            Ok(Some(index)) => Ok(Reply::Return(
                Outputs::default().write_w(0, 0)?.write_w(1, index as u32)?,
            )),
            Ok(None) => result(TIMED_OUT),
            Err(PollError::Event(events::Error::InvalidHandle)) => result(INVALID_HANDLE),
            Err(_) => Err(HostFault::unsupported()),
        }
    }
}
