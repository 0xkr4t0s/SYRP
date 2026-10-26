// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! [`Logger`], the process-wide current logger, and [`WriterLogger`].

use std::io::Write;
use std::sync::{Mutex, PoisonError, RwLock};

/// A destination for log messages.
///
/// [`Logger::log`] runs while the current-logger lock is held, so a logger must not call
/// [`set_current_logger`] or [`write_to_log`]: either can deadlock.
///
/// ```
/// use std::sync::Mutex;
/// use syrp_sys::Logger;
///
/// struct Collect(Mutex<Vec<String>>);
/// impl Logger for Collect {
///     fn log(&self, message: &str) {
///         self.0.lock().unwrap().push(message.to_owned());
///     }
/// }
/// let c = Collect(Mutex::new(Vec::new()));
/// c.log("hello");
/// assert_eq!(c.0.lock().unwrap()[0], "hello");
/// ```
#[doc(alias = "juce::Logger")]
pub trait Logger {
    /// Records one message. Implementations add their own line ending if they need one.
    ///
    /// ```
    /// # use syrp_sys::{Logger, WriterLogger};
    /// WriterLogger::new(std::io::sink()).log("ignored");
    /// ```
    #[doc(alias = "logMessage")]
    fn log(&self, message: &str);
}

/// The logger used by [`write_to_log`].
static CURRENT: RwLock<Option<Box<dyn Logger + Send + Sync>>> = RwLock::new(None);

/// Installs `logger` as the process-wide logger (or removes it with `None`) and returns the
/// previous one. Any thread; it waits for log calls in progress on other threads.
///
/// ```
/// use syrp_sys::{set_current_logger, WriterLogger};
/// let previous = set_current_logger(Some(Box::new(WriterLogger::new(std::io::sink()))));
/// set_current_logger(previous);
/// ```
#[doc(alias = "setCurrentLogger")]
pub fn set_current_logger(
    logger: Option<Box<dyn Logger + Send + Sync>>,
) -> Option<Box<dyn Logger + Send + Sync>> {
    let mut current = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
    std::mem::replace(&mut *current, logger)
}

/// Sends `message` to the current logger, or, when none is set, writes it and a newline to
/// standard error (and, on Windows, to the debugger with `OutputDebugStringW`).
///
/// Any thread; not RT-safe (it locks and does I/O). A logger must not call
/// [`set_current_logger`] or [`write_to_log`] from inside [`Logger::log`]: either can deadlock.
///
/// ```
/// syrp_sys::write_to_log("starting up");
/// ```
#[doc(alias = "writeToLog")]
pub fn write_to_log(message: &str) {
    let current = CURRENT.read().unwrap_or_else(PoisonError::into_inner);
    if let Some(logger) = current.as_deref() {
        logger.log(message);
        return;
    }
    drop(current);
    let _ = writeln!(std::io::stderr().lock(), "{message}");
    #[cfg(windows)]
    {
        let text = crate::os_info::wide(&format!("{message}\n"));
        // SAFETY: `text` is a live, NUL-terminated UTF-16 string the call only reads.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::OutputDebugStringW(text.as_ptr())
        };
    }
}

/// A [`Logger`] that writes each message and a newline to any [`std::io::Write`], flushing
/// after each one. Write errors are ignored, since a logger has nowhere to report them.
///
/// ```
/// use syrp_sys::{Logger, WriterLogger};
/// let logger = WriterLogger::new(Vec::new());
/// logger.log("one");
/// logger.log("two");
/// assert_eq!(logger.into_inner(), b"one\ntwo\n");
/// ```
#[derive(Debug)]
pub struct WriterLogger<W> {
    writer: Mutex<W>,
}

impl<W: Write> WriterLogger<W> {
    /// Wraps `writer`.
    ///
    /// ```
    /// # use syrp_sys::WriterLogger;
    /// let _log = WriterLogger::new(std::io::stdout());
    /// ```
    #[must_use]
    pub fn new(writer: W) -> Self {
        Self {
            writer: Mutex::new(writer),
        }
    }

    /// Returns the writer.
    ///
    /// ```
    /// # use syrp_sys::WriterLogger;
    /// assert!(WriterLogger::new(Vec::<u8>::new()).into_inner().is_empty());
    /// ```
    #[must_use]
    pub fn into_inner(self) -> W {
        self.writer
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl<W: Write> Logger for WriterLogger<W> {
    fn log(&self, message: &str) {
        let mut w = self.writer.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = writeln!(w, "{message}").and_then(|()| w.flush());
    }
}
