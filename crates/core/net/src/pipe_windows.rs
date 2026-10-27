// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Windows side of [`NamedPipe`](crate::NamedPipe): a byte-mode duplex pipe used with
//! overlapped I/O, so that every wait has a timeout and notices `close`.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_NO_DATA, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_NOT_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use crate::NetError;
use crate::pipe::{pipe_error, slice_until};

/// The pipe's buffer size in each direction (a hint to the OS).
const BUFFER: u32 = 64 * 1024;

/// One end of `\\.\pipe\<name>`.
#[derive(Debug)]
pub(crate) struct Pipe {
    handle: OwnedHandle,
    /// For the creating end, whether a client is connected; `None` for a client.
    connected: Option<Mutex<bool>>,
}

/// How an overlapped operation ended.
enum Wait {
    Done(u32),
    TimedOut,
    Closed,
}

impl Pipe {
    pub(crate) fn create(name: &str, must_not_exist: bool) -> io::Result<Self> {
        let path = wide_path(name);
        let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        if must_not_exist {
            open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let pipe_mode =
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: `path` is a NUL-terminated UTF-16 string that outlives the call, and a
        // null security-attributes pointer is documented as "use the defaults".
        let handle = unsafe {
            CreateNamedPipeW(
                path.as_ptr(),
                open_mode,
                pipe_mode,
                PIPE_UNLIMITED_INSTANCES,
                BUFFER,
                BUFFER,
                0,
                ptr::null(),
            )
        };
        let handle = owned(handle).map_err(|e| {
            // FILE_FLAG_FIRST_PIPE_INSTANCE reports an existing pipe as "access denied".
            if must_not_exist && e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) {
                io::Error::from(io::ErrorKind::AlreadyExists)
            } else {
                e
            }
        })?;
        Ok(Self {
            handle,
            connected: Some(Mutex::new(false)),
        })
    }

    pub(crate) fn open(name: &str) -> io::Result<Self> {
        let path = wide_path(name);
        // SAFETY: `path` is a NUL-terminated UTF-16 string that outlives the call; null
        // security attributes and template handle are documented as "none".
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                ptr::null_mut(),
            )
        };
        Ok(Self {
            handle: owned(handle)?,
            connected: None,
        })
    }

    /// Reads what is available, waiting for some until `deadline`; 0 means it passed.
    pub(crate) fn read(
        &self,
        buf: &mut [u8],
        deadline: Option<Instant>,
        closed: &AtomicBool,
    ) -> Result<usize, NetError> {
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let data = buf.as_mut_ptr();
        self.transfer("read", deadline, closed, |h, ov| {
            // SAFETY: `data` points to `len` writable bytes of the caller's buffer, which
            // outlives `transfer`; `transfer` keeps `ov` in place until the read completes.
            unsafe { ReadFile(h, data, len, ptr::null_mut(), ov) }
        })
    }

    /// Writes what fits, waiting for room until `deadline`; 0 means it passed.
    pub(crate) fn write(
        &self,
        buf: &[u8],
        deadline: Option<Instant>,
        closed: &AtomicBool,
    ) -> Result<usize, NetError> {
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let data = buf.as_ptr();
        self.transfer("write", deadline, closed, |h, ov| {
            // SAFETY: `data` points to `len` readable bytes of the caller's buffer, which
            // outlives `transfer`; `transfer` keeps `ov` in place until the write completes.
            unsafe { WriteFile(h, data, len, ptr::null_mut(), ov) }
        })
    }

    /// Pending operations notice the closed flag within one slice and cancel themselves;
    /// the handle is released on drop.
    pub(crate) fn close(&self) {}

    fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle()
    }

    /// Connects (when this is the creating end) and runs one read or write.
    fn transfer(
        &self,
        operation: &'static str,
        deadline: Option<Instant>,
        closed: &AtomicBool,
        start: impl Fn(HANDLE, *mut OVERLAPPED) -> i32,
    ) -> Result<usize, NetError> {
        loop {
            if !self.ensure_connected(deadline, closed)? {
                return Ok(0);
            }
            match overlapped(self.raw(), deadline, closed, |ov| start(self.raw(), ov)) {
                Ok(Wait::Done(0)) => {}
                Ok(Wait::Done(n)) => return Ok(n as usize),
                Ok(Wait::TimedOut) => return Ok(0),
                Ok(Wait::Closed) => return Err(NetError::Closed),
                Err(e) if self.connected.is_some() && peer_gone(&e) => self.disconnect(),
                Err(e) => return Err(pipe_error(operation, &e)),
            }
            if slice_until(deadline).is_none() {
                return Ok(0);
            }
        }
    }

    /// For the creating end, waits until `deadline` for a client; false means it passed.
    fn ensure_connected(
        &self,
        deadline: Option<Instant>,
        closed: &AtomicBool,
    ) -> Result<bool, NetError> {
        let Some(flag) = &self.connected else {
            return Ok(true);
        };
        let Some(mut connected) = lock(flag, deadline, closed)? else {
            return Ok(false);
        };
        if *connected {
            return Ok(true);
        }
        // SAFETY: the handle is a pipe server handle and `overlapped` keeps `ov` in place
        // until the connect completes or is cancelled.
        let wait = overlapped(self.raw(), deadline, closed, |ov| unsafe {
            ConnectNamedPipe(self.raw(), ov)
        });
        *connected = match wait {
            Ok(Wait::Done(_)) => true,
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) => true,
            Ok(Wait::TimedOut) => false,
            Ok(Wait::Closed) => return Err(NetError::Closed),
            Err(e) => return Err(pipe_error("connect", &e)),
        };
        Ok(*connected)
    }

    /// Drops a client that went away, so the next call waits for a new one. When another
    /// thread holds the flag it is already connecting, and this does nothing.
    fn disconnect(&self) {
        let Some(m) = &self.connected else { return };
        let mut connected = match m.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(p)) => p.into_inner(),
            Err(TryLockError::WouldBlock) => return,
        };
        if *connected {
            // SAFETY: the handle is a pipe server handle owned by this value.
            unsafe { DisconnectNamedPipe(self.raw()) };
            *connected = false;
        }
    }
}

/// The connection flag of the creating end, waiting until `deadline` for another thread
/// that is connecting; `Ok(None)` means the deadline passed.
fn lock<'a>(
    flag: &'a Mutex<bool>,
    deadline: Option<Instant>,
    closed: &AtomicBool,
) -> Result<Option<MutexGuard<'a, bool>>, NetError> {
    loop {
        if closed.load(Ordering::Acquire) {
            return Err(NetError::Closed);
        }
        match flag.try_lock() {
            Ok(guard) => return Ok(Some(guard)),
            Err(TryLockError::Poisoned(p)) => return Ok(Some(p.into_inner())),
            Err(TryLockError::WouldBlock) => match slice_until(deadline) {
                Some(slice) => std::thread::sleep(slice.min(Duration::from_millis(5))),
                None => return Ok(None),
            },
        }
    }
}

/// Starts an overlapped operation with `start` and waits for it in slices until it
/// completes, `deadline` passes or `closed` is set; in the last two cases it cancels the
/// operation and waits for the cancellation, so `start`'s buffer is free when it returns.
fn overlapped(
    handle: HANDLE,
    deadline: Option<Instant>,
    closed: &AtomicBool,
    start: impl FnOnce(*mut OVERLAPPED) -> i32,
) -> io::Result<Wait> {
    // SAFETY: a manual-reset, initially unsignalled, unnamed event with default security.
    let event = owned(unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) })?;
    let mut ov = OVERLAPPED {
        hEvent: event.as_raw_handle(),
        ..OVERLAPPED::default()
    };
    let ov_ptr = &raw mut ov;
    if start(ov_ptr) == 0 {
        let e = io::Error::last_os_error();
        if e.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
            return Err(e);
        }
    }
    let mut moved = 0u32;
    loop {
        let stop = if closed.load(Ordering::Acquire) {
            Some(Wait::Closed)
        } else {
            slice_until(deadline).map_or(Some(Wait::TimedOut), |_| None)
        };
        if let Some(stop) = stop {
            // SAFETY: `ov_ptr` is the operation started above on `handle`; waiting (TRUE)
            // for its result means the OS no longer uses `ov` or the buffer afterwards.
            let finished = unsafe {
                CancelIoEx(handle, ov_ptr);
                GetOverlappedResult(handle, ov_ptr, &mut moved, 1)
            };
            // Bytes that completed before the cancellation are not lost.
            return Ok(if finished != 0 && moved > 0 {
                Wait::Done(moved)
            } else {
                stop
            });
        }
        let slice = slice_until(deadline).unwrap_or_default();
        let ms = u32::try_from(slice.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: `event` is a valid event handle owned by this function.
        let woke = unsafe { WaitForSingleObject(event.as_raw_handle(), ms) };
        if woke == WAIT_OBJECT_0 {
            // SAFETY: the event is signalled, so the operation on `ov_ptr` has completed.
            let ok = unsafe { GetOverlappedResult(handle, ov_ptr, &mut moved, 0) };
            return if ok != 0 {
                Ok(Wait::Done(moved))
            } else {
                Err(io::Error::last_os_error())
            };
        }
        if woke != WAIT_TIMEOUT {
            return Err(io::Error::last_os_error());
        }
    }
}

fn peer_gone(e: &io::Error) -> bool {
    [ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED, ERROR_NO_DATA]
        .iter()
        .any(|&code| e.raw_os_error() == Some(code as i32))
}

fn wide_path(name: &str) -> Vec<u16> {
    format!(r"\\.\pipe\{name}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// Takes ownership of a handle returned by the OS, or reports its last error.
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `handle` is a valid handle that the caller just received from the OS and
    // owns exclusively.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
