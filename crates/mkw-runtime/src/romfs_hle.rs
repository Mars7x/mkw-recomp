// SPDX-License-Identifier: MIT
// Adapted from seedds/nxnative dm/host/hle_fs.c at
// c8ab8166f68296dc712a4ec13b892535d9d790c6: read-only OpenFile, ReadFile,
// ReadFileExact, GetFileSize, CloseFile and Result mapping. Copyright (c) 2026
// seedds. Generic host ancestry: d2r-mobile, Copyright (c) 2026 jhaizhou.
// Full notices and adaptation details: third_party/nxnative/.
// Port differences: checked guest addresses; opaque non-reused handles;
// fd-relative no-symlink Linux paths; bounded host reads; atomic output batches.
use crate::{
    exclusive_calls::Target,
    host_call::{HostCall, HostFault, HostReturn, HostService},
    GuestAddr,
};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{ffi::OsStrExt, fs::FileExt},
    },
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
static NEXT_NAMESPACE: AtomicU64 = AtomicU64::new(1);
pub const RESULT_OK: u32 = 0;
pub const RESULT_FAIL: u32 = 0x2a2;
pub const RESULT_PATH_NOT_FOUND: u32 = 0x202;
pub const OPEN: &str = "_ZN2nn2fs8OpenFileEPNS0_10FileHandleEPKci";
pub const READ: &str = "_ZN2nn2fs8ReadFileEPmNS0_10FileHandleElPvm";
pub const READ_EXACT: &str = "_ZN2nn2fs8ReadFileENS0_10FileHandleElPvm";
pub const SIZE: &str = "_ZN2nn2fs11GetFileSizeEPlNS0_10FileHandleE";
pub const CLOSE: &str = "_ZN2nn2fs9CloseFileENS0_10FileHandleE";
pub const SYMBOLS: [&str; 5] = [OPEN, READ, READ_EXACT, SIZE, CLOSE];
#[derive(Clone, Copy)]
pub struct Limits {
    pub handles: usize,
    pub read_bytes: usize,
}
struct State {
    files: BTreeMap<u64, File>,
    next: u32,
}
pub struct ReadOnlyFs {
    root: File,
    namespace: u64,
    mount: Vec<u8>,
    limits: Limits,
    state: Mutex<State>,
}
#[derive(Clone, Copy)]
enum Op {
    Open,
    Read,
    Exact,
    Size,
    Close,
}
struct Handler {
    fs: Arc<ReadOnlyFs>,
    op: Op,
}
fn file_from_fd(fd: libc::c_int) -> io::Result<File> {
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: successful open/openat returned a new owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn result(error: &io::Error) -> HostReturn {
    HostReturn::U32(
        if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ENOTDIR) {
            RESULT_PATH_NOT_FOUND
        } else {
            RESULT_FAIL
        },
    )
}
impl ReadOnlyFs {
    pub fn new(root: &Path, mount: &str, limits: Limits) -> io::Result<Arc<Self>> {
        if mount.is_empty()
            || mount.len() > 31
            || !mount
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || limits.handles == 0
            || limits.handles > 4096
            || limits.read_bytes == 0
            || limits.read_bytes > 16 * 1024 * 1024
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid explicit read-only filesystem limits/mount",
            ));
        }
        let path = CString::new(root.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in root"))?;
        // SAFETY: path is a live NUL-terminated host path. No guest pointer is used.
        let root = file_from_fd(unsafe {
            libc::open(
                path.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let mut candidate = NEXT_NAMESPACE.load(Ordering::Relaxed);
        let namespace = loop {
            if candidate >= u32::MAX as u64 {
                return Err(io::Error::other("filesystem namespace quota exhausted"));
            }
            match NEXT_NAMESPACE.compare_exchange_weak(
                candidate,
                candidate + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break candidate,
                Err(observed) => candidate = observed,
            }
        };
        Ok(Arc::new(Self {
            root,
            namespace,
            mount: mount.as_bytes().to_vec(),
            limits,
            state: Mutex::new(State {
                files: BTreeMap::new(),
                next: 1,
            }),
        }))
    }
    pub fn target<'code>(
        self: &Arc<Self>,
        symbol: &str,
        address: GuestAddr,
        module_base: u64,
    ) -> Result<Target<'code>, String> {
        let op = match symbol {
            OPEN => Op::Open,
            READ => Op::Read,
            READ_EXACT => Op::Exact,
            SIZE => Op::Size,
            CLOSE => Op::Close,
            _ => return Err("filesystem symbol is not implemented; no fallback".into()),
        };
        Target::host(
            address,
            module_base,
            Arc::new(Handler {
                fs: self.clone(),
                op,
            }),
        )
    }
    fn open(&self, path: &[u8]) -> io::Result<File> {
        let Some(colon) = path.iter().position(|&b| b == b':') else {
            return Err(io::ErrorKind::NotFound.into());
        };
        if path[..colon] != self.mount || path.get(colon + 1) != Some(&b'/') {
            return Err(io::ErrorKind::NotFound.into());
        }
        let relative = &path[colon + 2..];
        let parts: Vec<_> = relative.split(|&b| b == b'/').collect();
        if parts
            .iter()
            .any(|p| p.is_empty() || *p == b"." || *p == b".." || p.len() > 255)
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut dir = self.root.try_clone()?;
        for (i, part) in parts.iter().enumerate() {
            let name = CString::new(*part).map_err(|_| io::ErrorKind::InvalidInput)?;
            let last = i + 1 == parts.len();
            let flags = libc::O_RDONLY
                | libc::O_NOFOLLOW
                | libc::O_CLOEXEC
                | if last {
                    libc::O_NONBLOCK
                } else {
                    libc::O_DIRECTORY
                };
            // SAFETY: dir is an owned live directory fd; name is one checked component.
            let next =
                file_from_fd(unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) })?;
            if last {
                if !next.metadata()?.is_file() {
                    return Err(io::ErrorKind::NotFound.into());
                }
                return Ok(next);
            }
            dir = next;
        }
        Err(io::ErrorKind::NotFound.into())
    }
}
impl HostService for Handler {
    fn invoke(&self, call: &mut HostCall<'_>) -> Result<HostReturn, HostFault> {
        let a = call.arguments();
        let mut state = self.fs.state.lock().map_err(|_| HostFault::unsupported())?;
        match self.op {
            Op::Open => {
                call.check_outputs(&[(GuestAddr(a[0]), 8)])?;
                let path = call.c_string(GuestAddr(a[1]), 0x300)?;
                let mode = a[2] as u32;
                if mode & !7 != 0 || mode == 0 {
                    return Err(HostFault::unsupported());
                }
                if mode != 1 {
                    return Ok(HostReturn::U32(RESULT_FAIL));
                }
                if state.files.len() >= self.fs.limits.handles {
                    return Err(HostFault::unsupported());
                }
                let next = state
                    .next
                    .checked_add(1)
                    .ok_or_else(HostFault::unsupported)?;
                let file = match self.fs.open(&path) {
                    Ok(file) => file,
                    Err(e) => return Ok(result(&e)),
                };
                let handle = (self.fs.namespace << 32) | u64::from(state.next);
                call.write_outputs(&[(GuestAddr(a[0]), &handle.to_le_bytes())])?;
                state.files.insert(handle, file);
                state.next = next;
                Ok(HostReturn::U32(RESULT_OK))
            }
            Op::Size => {
                call.check_outputs(&[(GuestAddr(a[0]), 8)])?;
                let Some(file) = state.files.get(&a[1]) else {
                    return Ok(HostReturn::U32(RESULT_FAIL));
                };
                let size = match file.metadata() {
                    Ok(m) => m.len(),
                    Err(e) => return Ok(result(&e)),
                };
                if size > i64::MAX as u64 {
                    return Err(HostFault::unsupported());
                }
                call.write_outputs(&[(GuestAddr(a[0]), &size.to_le_bytes())])?;
                Ok(HostReturn::U32(RESULT_OK))
            }
            Op::Close => {
                if a[0] == 0 {
                    return Ok(HostReturn::Void);
                }
                if state.files.remove(&a[0]).is_none() {
                    return Err(HostFault::unsupported());
                }
                Ok(HostReturn::Void)
            }
            Op::Read | Op::Exact => {
                let exact = matches!(self.op, Op::Exact);
                let (h, offset, dest, size) = if exact {
                    (a[0], a[1], a[2], a[3])
                } else {
                    (a[1], a[2], a[3], a[4])
                };
                let size = usize::try_from(size).map_err(|_| HostFault::unsupported())?;
                if size > self.fs.limits.read_bytes {
                    return Err(HostFault::unsupported());
                }
                let mut ranges = vec![(GuestAddr(dest), size)];
                if !exact {
                    ranges.push((GuestAddr(a[0]), 8));
                }
                call.check_outputs(&ranges)?;
                if h == 0 {
                    if !exact {
                        call.write_outputs(&[(GuestAddr(a[0]), &0u64.to_le_bytes())])?;
                    }
                    return Ok(HostReturn::U32(RESULT_FAIL));
                }
                let Some(file) = state.files.get(&h) else {
                    return Ok(HostReturn::U32(RESULT_FAIL));
                };
                if size > 0
                    && ((offset as i64) < 0
                        || offset
                            .checked_add(size as u64)
                            .is_none_or(|end| end > i64::MAX as u64))
                {
                    return Ok(HostReturn::U32(RESULT_FAIL));
                }
                let mut data = vec![0; size];
                let mut got = 0;
                let mut failed = false;
                while got < size {
                    match file.read_at(&mut data[got..], offset + got as u64) {
                        Ok(0) => break,
                        Ok(n) => got += n,
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                let count = (got as u64).to_le_bytes();
                let mut writes = vec![(GuestAddr(dest), &data[..got])];
                if !exact && !failed {
                    writes.push((GuestAddr(a[0]), &count));
                }
                call.write_outputs(&writes)?;
                Ok(HostReturn::U32(if failed || (exact && got != size) {
                    RESULT_FAIL
                } else {
                    RESULT_OK
                }))
            }
        }
    }
}
