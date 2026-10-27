// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`NamedPipe`]: a local, two-way byte pipe between processes, found by name.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::NetError;

#[cfg(unix)]
use crate::pipe_unix as imp;
#[cfg(windows)]
use crate::pipe_windows as imp;

/// The longest name [`NamedPipe`] accepts, in bytes.
const MAX_NAME: usize = 200;

/// A named, two-way byte pipe between two ends on this machine (JUCE's `NamedPipe`).
///
/// One end calls [`create_new_pipe`](Self::create_new_pipe) and the other
/// [`open_existing`](Self::open_existing) with the same name; then both ends can read and
/// write. Every method takes `&self`, so [`close`](Self::close) from another thread ends a
/// pending [`read`](Self::read) or [`write`](Self::write) with [`NetError::Closed`].
///
/// # Naming rule
/// A name is 1 to 200 bytes of ASCII letters, digits, `-`, `_` and `.`, and does not start
/// with `.` (Syrp's choice, so that one name is valid on every OS). It maps to:
/// - **Unix:** two FIFOs, `<name>.in` (the creator reads it) and `<name>.out` (the creator
///   writes it), in the private directory `syrp-pipes-<uid>` (mode 0700) inside
///   [`std::env::temp_dir`]. The creator removes both FIFOs when it closes.
/// - **Windows:** the byte-mode duplex pipe `\\.\pipe\<name>`, local clients only.
///
/// # Peers
/// On Unix each end keeps both FIFOs open for reading and writing, so data written before
/// the other end opens waits in the pipe, and a peer that goes away is not reported: reads
/// wait for data until their timeout. On Windows the creator waits for a client on its
/// first read or write and, when the client disconnects, for the next one; a client whose
/// creator went away gets [`NetError::Pipe`].
///
/// ```
/// use std::time::Duration;
/// use syrp_net::NamedPipe;
/// let name = format!("syrp-doc-{}", std::process::id());
/// let server = NamedPipe::create_new_pipe(&name, true)?;
/// let client = NamedPipe::open_existing(&name)?;
/// client.write(b"ping", None)?;
/// let mut buf = [0u8; 4];
/// assert_eq!(server.read(&mut buf, Some(Duration::from_secs(5)))?, 4);
/// assert_eq!(&buf, b"ping");
/// # Ok::<(), syrp_net::NetError>(())
/// ```
#[derive(Debug)]
pub struct NamedPipe {
    pipe: imp::Pipe,
    name: String,
    closed: AtomicBool,
}

impl NamedPipe {
    /// Creates the pipe `name` and returns its creating end. With `must_not_exist` it fails
    /// when a pipe of that name already exists; otherwise it reuses it.
    ///
    /// # Errors
    /// [`NetError::InvalidPipeName`] when `name` breaks the [naming rule](Self#naming-rule),
    /// [`NetError::Pipe`] (kind `AlreadyExists` with `must_not_exist`) when the OS refuses.
    ///
    /// ```
    /// use syrp_net::NamedPipe;
    /// let name = format!("syrp-doc-create-{}", std::process::id());
    /// let p = NamedPipe::create_new_pipe(&name, true)?;
    /// assert!(p.is_open());
    /// assert!(NamedPipe::create_new_pipe(&name, true).is_err());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn create_new_pipe(name: &str, must_not_exist: bool) -> Result<Self, NetError> {
        check_name(name)?;
        let pipe = imp::Pipe::create(name, must_not_exist).map_err(|e| pipe_error("create", &e))?;
        Ok(Self::wrap(pipe, name))
    }

    /// Opens the pipe `name`, which the other end created with
    /// [`create_new_pipe`](Self::create_new_pipe).
    ///
    /// # Errors
    /// [`NetError::InvalidPipeName`] for a bad name, [`NetError::Pipe`] (kind `NotFound`)
    /// when no such pipe exists, or another [`NetError::Pipe`] when the OS refuses.
    ///
    /// ```
    /// let err = syrp_net::NamedPipe::open_existing("syrp-doc-no-such-pipe").unwrap_err();
    /// assert!(matches!(err, syrp_net::NetError::Pipe { kind: std::io::ErrorKind::NotFound, .. }));
    /// ```
    pub fn open_existing(name: &str) -> Result<Self, NetError> {
        check_name(name)?;
        let pipe = imp::Pipe::open(name).map_err(|e| pipe_error("open", &e))?;
        Ok(Self::wrap(pipe, name))
    }

    /// Reads until `buf` is full or `timeout` passes (`None` waits indefinitely), and
    /// returns how many bytes arrived: fewer than `buf.len()`, possibly 0, only on timeout.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the pipe is (or becomes) closed, [`NetError::Pipe`] when
    /// the OS read fails.
    ///
    /// ```
    /// use std::time::Duration;
    /// let name = format!("syrp-doc-read-{}", std::process::id());
    /// let p = syrp_net::NamedPipe::create_new_pipe(&name, true)?;
    /// assert_eq!(p.read(&mut [0u8; 4], Some(Duration::from_millis(10)))?, 0);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn read(&self, buf: &mut [u8], timeout: Option<Duration>) -> Result<usize, NetError> {
        let deadline = deadline(timeout);
        let mut filled = 0;
        while filled < buf.len() {
            self.check_open()?;
            let n = self.pipe.read(&mut buf[filled..], deadline, &self.closed)?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        Ok(filled)
    }

    /// Writes `buf`, waiting at most `timeout` (`None` waits indefinitely) for room in the
    /// pipe, and returns how many bytes were written: fewer than `buf.len()` only on timeout.
    ///
    /// # Errors
    /// [`NetError::Closed`] when the pipe is (or becomes) closed, [`NetError::Pipe`] when
    /// the OS write fails.
    ///
    /// ```no_run
    /// # let p = syrp_net::NamedPipe::open_existing("my-app")?;
    /// assert_eq!(p.write(b"hello", Some(std::time::Duration::from_secs(1)))?, 5);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn write(&self, buf: &[u8], timeout: Option<Duration>) -> Result<usize, NetError> {
        let deadline = deadline(timeout);
        let mut sent = 0;
        while sent < buf.len() {
            self.check_open()?;
            let n = self.pipe.write(&buf[sent..], deadline, &self.closed)?;
            if n == 0 {
                break;
            }
            sent += n;
        }
        Ok(sent)
    }

    /// Closes this end: further calls fail with [`NetError::Closed`], and calls pending in
    /// other threads return within a few tens of milliseconds. Closing twice does nothing.
    ///
    /// ```
    /// let name = format!("syrp-doc-close-{}", std::process::id());
    /// let p = syrp_net::NamedPipe::create_new_pipe(&name, true)?;
    /// p.close();
    /// assert!(!p.is_open());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.pipe.close();
        }
    }

    /// Whether [`close`](Self::close) has not been called.
    ///
    /// ```
    /// let name = format!("syrp-doc-open-{}", std::process::id());
    /// assert!(syrp_net::NamedPipe::create_new_pipe(&name, true)?.is_open());
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }

    /// The name this pipe was created or opened with.
    ///
    /// ```
    /// let name = format!("syrp-doc-name-{}", std::process::id());
    /// assert_eq!(syrp_net::NamedPipe::create_new_pipe(&name, true)?.name(), name);
    /// # Ok::<(), syrp_net::NetError>(())
    /// ```
    pub fn name(&self) -> &str {
        &self.name
    }

    fn wrap(pipe: imp::Pipe, name: &str) -> Self {
        Self {
            pipe,
            name: name.to_owned(),
            closed: AtomicBool::new(false),
        }
    }

    fn check_open(&self) -> Result<(), NetError> {
        if self.is_open() {
            Ok(())
        } else {
            Err(NetError::Closed)
        }
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        self.close();
    }
}

fn deadline(timeout: Option<Duration>) -> Option<Instant> {
    timeout.and_then(|t| Instant::now().checked_add(t))
}

fn check_name(name: &str) -> Result<(), NetError> {
    let reason = if name.is_empty() {
        Some("it is empty")
    } else if name.len() > MAX_NAME {
        Some("it is longer than 200 bytes")
    } else if name.starts_with('.') {
        Some("it starts with '.'")
    } else if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        Some("only ASCII letters, digits, '-', '_' and '.' are allowed")
    } else {
        None
    };
    reason.map_or(Ok(()), |reason| {
        Err(NetError::InvalidPipeName {
            name: name.to_owned(),
            reason,
        })
    })
}

pub(crate) fn pipe_error(operation: &'static str, e: &io::Error) -> NetError {
    NetError::Pipe {
        operation,
        kind: e.kind(),
        os_error: e.raw_os_error(),
    }
}

/// How long to wait in one slice before checking for close: [`SLICE`](crate::socket::SLICE),
/// or less when `deadline` is nearer. `None` means the deadline has passed.
pub(crate) fn slice_until(deadline: Option<Instant>) -> Option<Duration> {
    let slice = crate::socket::SLICE;
    match deadline {
        None => Some(slice),
        Some(d) => {
            let left = d.saturating_duration_since(Instant::now());
            (!left.is_zero()).then(|| left.min(slice))
        }
    }
}
