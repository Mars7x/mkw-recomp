//! Explicit immutable process entropy reference profile. No guessed address,
//! memory, clock or other GetInfo response, no default service activation.
use crate::{
    host_call::HostFault,
    services::{Call, Outputs, Reply, Service, Services},
};
#[cfg(any(target_os = "linux", test))]
use std::io;
use std::sync::Arc;
pub const GET_INFO: u16 = 0x29;
pub const RANDOM_ENTROPY: u32 = 11;
pub const INVALID_HANDLE: u32 = (114 << 9) | 1;
pub const INVALID_COMBINATION: u32 = (116 << 9) | 1;
#[derive(Clone, Copy)]
pub enum Abi {
    Horizon64EntropyReference,
}
#[derive(Clone)]
pub struct ProcessEntropy {
    words: Arc<[u64; 4]>,
}
impl ProcessEntropy {
    /// Explicit deterministic fixtures only; not target authorization or a retail
    /// entropy source. Production callers must configure their process source.
    pub fn from_synthetic_words(_abi: Abi, words: [u64; 4]) -> Self {
        Self {
            words: Arc::new(words),
        }
    }
    /// Read exactly 32 host random bytes once, before bounded service invocation.
    /// Never logs values or silently replaces an unavailable source with zeros.
    #[cfg(target_os = "linux")]
    pub fn from_os(abi: Abi) -> io::Result<Self> {
        let mut bytes = [0u8; 32];
        fill(&mut bytes, |out| {
            // SAFETY: getrandom writes at most the writable slice length. No
            // guest pointer, source code or uninitialized storage is exposed.
            let n = unsafe { libc::getrandom(out.as_mut_ptr().cast(), out.len(), 0) };
            if n < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(n as usize)
            }
        })?;
        let mut words = [0; 4];
        for (word, b) in words.iter_mut().zip(bytes.chunks_exact(8)) {
            *word = u64::from_le_bytes(b.try_into().unwrap());
        }
        Ok(Self::from_synthetic_words(abi, words))
    }
    pub fn registry(&self) -> Services {
        Services::new(vec![(GET_INFO, Arc::new(self.clone()))]).expect("one entropy reference SVC")
    }
}
fn result(value: u32) -> Result<Reply, HostFault> {
    Ok(Reply::Return(Outputs::default().write_w(0, value)?))
}
impl Service for ProcessEntropy {
    fn invoke(&self, call: &Call<'_>) -> Result<Reply, HostFault> {
        let a = call.arguments();
        if a[1] as u32 != RANDOM_ENTROPY {
            return Err(HostFault::unsupported());
        }
        if a[2] as u32 != 0 {
            return result(INVALID_HANDLE);
        }
        if a[3] >= 4 {
            return result(INVALID_COMBINATION);
        }
        Ok(Reply::Return(
            Outputs::default()
                .write_w(0, 0)?
                .write_x(1, self.words[a[3] as usize])?,
        ))
    }
}
#[cfg(any(target_os = "linux", test))]
fn fill(
    mut out: &mut [u8],
    mut read: impl FnMut(&mut [u8]) -> io::Result<usize>,
) -> io::Result<()> {
    while !out.is_empty() {
        match read(out) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "entropy source returned no bytes",
                ))
            }
            Ok(n) if n <= out.len() => {
                out = &mut out[n..];
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "entropy source exceeded output bound",
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entropy_input_requires_complete_bytes_and_handles_short_reads_and_interrupts() {
        let mut out = [0u8; 32];
        let mut position = 0;
        let mut calls = 0;
        fill(&mut out, |dst| {
            calls += 1;
            if calls == 2 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = dst.len().min(3);
            for byte in &mut dst[..n] {
                *byte = position;
                position += 1;
            }
            Ok(n)
        })
        .unwrap();
        assert_eq!(out, (0u8..32).collect::<Vec<_>>().as_slice());
        assert_eq!(
            fill(&mut out, |_| Ok(0)).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            fill(&mut out, |b| Ok(b.len() + 1)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            fill(&mut out, |_| Err(io::ErrorKind::PermissionDenied.into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    #[test]
    fn entropy_clones_retain_one_immutable_process_source_without_logging_values() {
        let e = ProcessEntropy::from_synthetic_words(
            Abi::Horizon64EntropyReference,
            [0, 1, 2, u64::MAX],
        );
        let c = e.clone();
        assert!(Arc::ptr_eq(&e.words, &c.words));
        #[cfg(target_os = "linux")]
        {
            let os = ProcessEntropy::from_os(Abi::Horizon64EntropyReference).unwrap();
            assert!(Arc::ptr_eq(&os.words, &os.clone().words));
        }
    }
}
