// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! The Unix side of [`NamedPipe`](crate::NamedPipe): two FIFOs, one per direction.

use std::ffi::CString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::NetError;
use crate::pipe::{pipe_error, slice_until};
use crate::socket::{poll_fd, retryable};

/// One end: the FIFO it reads and the FIFO it writes, both open for reading and writing
/// (so neither open blocks, data waits for a late peer, and writes never raise `SIGPIPE`).
#[derive(Debug)]
pub(crate) struct Pipe {
    reader: File,
    writer: File,
    /// The FIFOs to remove on close; only the creating end owns them.
    owned: Option<[PathBuf; 2]>,
}

impl Pipe {
    pub(crate) fn create(name: &str, must_not_exist: bool) -> io::Result<Self> {
        let (to_creator, to_opener) = paths(name)?;
        if must_not_exist && (exists(&to_creator) || exists(&to_opener)) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        for path in [&to_creator, &to_opener] {
            match make_fifo(path) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && !must_not_exist => {}
                other => other?,
            }
        }
        let pipe = Self {
            reader: open_fifo(&to_creator)?,
            writer: open_fifo(&to_opener)?,
            owned: Some([to_creator, to_opener]),
        };
        Ok(pipe)
    }

    pub(crate) fn open(name: &str) -> io::Result<Self> {
        let (to_creator, to_opener) = paths(name)?;
        Ok(Self {
            reader: open_fifo(&to_opener)?,
            writer: open_fifo(&to_creator)?,
            owned: None,
        })
    }

    /// Reads what is available, waiting for some until `deadline`; 0 means it passed.
    pub(crate) fn read(
        &self,
        buf: &mut [u8],
        deadline: Option<Instant>,
        closed: &AtomicBool,
    ) -> Result<usize, NetError> {
        let fd = self.reader.as_raw_fd();
        transfer(fd, true, deadline, closed, || (&self.reader).read(buf))
            .map_err(|e| e.into_net("read"))
    }

    /// Writes what fits, waiting for room until `deadline`; 0 means it passed.
    pub(crate) fn write(
        &self,
        buf: &[u8],
        deadline: Option<Instant>,
        closed: &AtomicBool,
    ) -> Result<usize, NetError> {
        let fd = self.writer.as_raw_fd();
        transfer(fd, false, deadline, closed, || (&self.writer).write(buf))
            .map_err(|e| e.into_net("write"))
    }

    /// Removes the FIFOs this end created, so no new peer can open them. The open handles
    /// stay valid until the value is dropped.
    pub(crate) fn close(&self) {
        for path in self.owned.iter().flatten() {
            // Already removed (by a reusing creator, say) is fine.
            let _ = fs::remove_file(path);
        }
    }
}

enum Stop {
    Closed,
    Os(io::Error),
}

impl Stop {
    fn into_net(self, operation: &'static str) -> NetError {
        match self {
            Self::Closed => NetError::Closed,
            Self::Os(e) => pipe_error(operation, &e),
        }
    }
}

/// Retries the non-blocking `op` on `fd`, polling in slices, until it moves bytes,
/// `deadline` passes (`Ok(0)`) or `closed` is set.
fn transfer(
    fd: std::os::fd::RawFd,
    for_reading: bool,
    deadline: Option<Instant>,
    closed: &AtomicBool,
    mut op: impl FnMut() -> io::Result<usize>,
) -> Result<usize, Stop> {
    loop {
        if closed.load(Ordering::Acquire) {
            return Err(Stop::Closed);
        }
        match op() {
            Ok(n) if n > 0 => return Ok(n),
            // Every end also holds the FIFO for writing, so 0 is not end of stream.
            Ok(_) => {}
            Err(e) if retryable(&e) => {}
            Err(e) => return Err(Stop::Os(e)),
        }
        let Some(slice) = slice_until(deadline) else {
            return Ok(0);
        };
        let ms = u16::try_from(slice.as_millis()).unwrap_or(u16::MAX);
        poll_fd(fd, for_reading, ms).map_err(Stop::Os)?;
    }
}

/// The FIFO the creator reads and the FIFO it writes, in the private per-user directory.
fn paths(name: &str) -> io::Result<(PathBuf, PathBuf)> {
    let dir = private_dir()?;
    Ok((
        dir.join(format!("{name}.in")),
        dir.join(format!("{name}.out")),
    ))
}

/// `<temp>/syrp-pipes-<uid>`, created with mode 0700; an existing one must be a real
/// directory owned by this user and closed to everyone else.
fn private_dir() -> io::Result<PathBuf> {
    // SAFETY: getuid takes no arguments, cannot fail and has no preconditions.
    let uid = unsafe { libc::getuid() };
    let dir = std::env::temp_dir().join(format!("syrp-pipes-{uid}"));
    match DirBuilder::new().mode(0o700).create(&dir) {
        Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
        _ => {}
    }
    let meta = fs::symlink_metadata(&dir)?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the pipe directory is not a private directory of this user",
        ));
    }
    Ok(dir)
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn make_fifo(path: &Path) -> io::Result<()> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    // SAFETY: `c_path` is a valid NUL-terminated string that outlives the call.
    if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Opens an existing FIFO for reading and writing without blocking (Linux and Apple allow
/// `O_RDWR` on a FIFO), refusing anything that is not a FIFO.
fn open_fifo(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.file_type().is_fifo() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the pipe path is not a FIFO",
        ));
    }
    Ok(file)
}
