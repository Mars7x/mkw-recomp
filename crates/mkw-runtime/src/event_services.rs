//! Explicit diagnostic event SVC binding. It is not target authorization and must
//! not be combined with other handle-producing kernel backends before handles
//! are unified. No wait/cancellation/clock/thread/process service is supplied.
use crate::{
    events::{Ends, Error, Events, Handle},
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services},
};
use std::sync::Arc;
/// Caller explicitly selects the inspected Horizon64 reference conventions;
/// selecting this profile does not verify the supplied Switch 2 kernel.
#[derive(Clone, Copy)]
pub enum Abi {
    Horizon64EventReference,
}
pub const CREATE: u16 = 0x45;
pub const SIGNAL: u16 = 0x11;
pub const CLEAR: u16 = 0x12;
pub const RESET: u16 = 0x17;
pub const CLOSE: u16 = 0x16;
const fn result(description: u32) -> u32 {
    (description << 9) | 1
}
pub const INVALID_HANDLE: u32 = result(114);
pub const INVALID_STATE: u32 = result(125);
pub const LIMIT_REACHED: u32 = result(132);
#[derive(Clone)]
pub struct EventServices {
    events: Arc<Events>,
}
#[derive(Clone, Copy)]
enum Op {
    Create,
    Signal,
    Clear,
    Reset,
    Close,
}
struct Handler {
    events: Arc<Events>,
    op: Op,
}
impl EventServices {
    pub fn new(_abi: Abi, limit: usize) -> Result<Self, Error> {
        Ok(Self {
            events: Arc::new(Events::new(limit)?),
        })
    }
    pub fn events(&self) -> &Events {
        &self.events
    }
    /// An explicit events-only registry. Every returned handler shares one process
    /// handle namespace. No implicit registration or guessed unsupported results.
    pub fn registry(&self) -> Services {
        let entries = [
            (CREATE, Op::Create),
            (SIGNAL, Op::Signal),
            (CLEAR, Op::Clear),
            (RESET, Op::Reset),
            (CLOSE, Op::Close),
        ]
        .into_iter()
        .map(|(n, op)| {
            (
                n,
                Arc::new(Handler {
                    events: self.events.clone(),
                    op,
                }) as Arc<dyn Service>,
            )
        })
        .collect();
        Services::new(entries).expect("five distinct event SVC numbers")
    }
}
fn failure(error: Error) -> Result<Reply, HostFault> {
    let value = match error {
        Error::InvalidHandle => INVALID_HANDLE,
        Error::AlreadyClear => INVALID_STATE,
        Error::Quota => LIMIT_REACHED,
        Error::Configuration | Error::IdentityExhausted | Error::Poisoned => {
            return Err(HostFault::unsupported())
        }
    };
    Ok(Reply::Return(Outputs::default().write_w(0, value)?))
}
impl Service for Handler {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let handle = Handle::from_raw(call.arguments()[0] as u32);
        let outputs = if matches!(self.op, Op::Create) {
            match self.events.create() {
                Ok(Ends { write, read }) => Outputs::default()
                    .write_w(1, write.raw())?
                    .write_w(2, read.raw())?,
                Err(error) => return failure(error),
            }
        } else {
            // A process pseudo-handle can be a valid ResetSignal target in the reference
            // kernel, but process state is absent here. Refuse rather than invent it.
            if matches!(self.op, Op::Reset) && handle.raw() == 0xffff8001 {
                return Err(HostFault::unsupported());
            }
            let outcome = match self.op {
                Op::Signal => self.events.signal(handle),
                Op::Clear => self.events.clear(handle),
                Op::Reset => self.events.reset(handle),
                Op::Close => self.events.close(handle),
                Op::Create => unreachable!(),
            };
            if let Err(error) = outcome {
                return failure(error);
            }
            Outputs::default()
        };
        Ok(Reply::Return(outputs.write_w(0, 0)?))
    }
}
