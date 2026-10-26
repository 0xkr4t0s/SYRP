// SPDX-License-Identifier: MIT OR Apache-2.0
//! Native file and alert dialogs, requested asynchronously: the desktop shows the dialog and
//! later calls a completion callback once, on the message thread.

use std::path::{Path, PathBuf};

use syrp_loop_api::MessageThread;

/// Called once with the chosen paths; empty if the user cancelled.
/// ```
/// let _done: syrp_peer_api::FileDialogDone = Box::new(|_mt, paths| println!("{paths:?}"));
/// ```
pub type FileDialogDone = Box<dyn FnOnce(&MessageThread<'_>, Vec<PathBuf>)>;

/// Called once with the index of the pressed button, or `None` if the alert was dismissed
/// without one (Escape, closing the window).
/// ```
/// let _done: syrp_peer_api::AlertDone = Box::new(|_mt, button| println!("{button:?}"));
/// ```
pub type AlertDone = Box<dyn FnOnce(&MessageThread<'_>, Option<usize>)>;

/// What a file dialog picks.
/// ```
/// assert_ne!(syrp_peer_api::FileDialogKind::Open, syrp_peer_api::FileDialogKind::Save);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FileDialogKind {
    /// Existing files to open.
    Open,
    /// A path to save to (the platform asks before overwriting).
    Save,
    /// An existing folder.
    Folder,
}

/// A native file dialog request (JUCE's FileChooser role).
/// ```
/// use syrp_peer_api::{FileDialog, FileDialogKind};
/// let d = FileDialog::new(FileDialogKind::Open)
///     .with_title("Load sample")
///     .with_filter("Audio", ["wav", "aiff"])
///     .with_multiple(true);
/// assert_eq!((d.kind(), d.title(), d.allows_multiple()), (FileDialogKind::Open, "Load sample", true));
/// assert_eq!(d.filters()[0], ("Audio".to_string(), vec!["wav".to_string(), "aiff".to_string()]));
/// ```
#[doc(alias = "FileChooser")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDialog {
    kind: FileDialogKind,
    title: String,
    start: Option<PathBuf>,
    filters: Vec<(String, Vec<String>)>,
    multiple: bool,
}

impl FileDialog {
    /// A dialog of `kind` with no title, start path or filters, picking one item.
    /// ```
    /// let d = syrp_peer_api::FileDialog::new(syrp_peer_api::FileDialogKind::Folder);
    /// assert!(d.filters().is_empty() && d.start().is_none());
    /// ```
    pub fn new(kind: FileDialogKind) -> Self {
        FileDialog {
            kind,
            title: String::new(),
            start: None,
            filters: Vec::new(),
            multiple: false,
        }
    }

    /// Sets the window title.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert_eq!(FileDialog::new(FileDialogKind::Save).with_title("Export").title(), "Export");
    /// ```
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the folder to start in, or (for `Save`) a suggested file path.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// let d = FileDialog::new(FileDialogKind::Save).with_start("/tmp/mix.wav");
    /// assert_eq!(d.start(), Some(std::path::Path::new("/tmp/mix.wav")));
    /// ```
    #[must_use]
    pub fn with_start(mut self, path: impl Into<PathBuf>) -> Self {
        self.start = Some(path.into());
        self
    }

    /// Adds a named filter of file extensions, without dots (`"wav"`); the first filter
    /// added is selected first. No filters means any file.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// let d = FileDialog::new(FileDialogKind::Open).with_filter("Presets", ["preset"]);
    /// assert_eq!(d.filters().len(), 1);
    /// ```
    #[must_use]
    pub fn with_filter<S: Into<String>>(
        mut self,
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = S>,
    ) -> Self {
        let exts = extensions.into_iter().map(Into::into).collect();
        self.filters.push((name.into(), exts));
        self
    }

    /// Allows picking several files or folders (ignored for `Save`).
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert!(FileDialog::new(FileDialogKind::Open).with_multiple(true).allows_multiple());
    /// ```
    #[must_use]
    pub fn with_multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    /// What the dialog picks.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert_eq!(FileDialog::new(FileDialogKind::Save).kind(), FileDialogKind::Save);
    /// ```
    pub fn kind(&self) -> FileDialogKind {
        self.kind
    }

    /// The window title (empty: the platform's default).
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert_eq!(FileDialog::new(FileDialogKind::Open).title(), "");
    /// ```
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The start folder or suggested path.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert!(FileDialog::new(FileDialogKind::Open).start().is_none());
    /// ```
    pub fn start(&self) -> Option<&Path> {
        self.start.as_deref()
    }

    /// The `(name, extensions)` filters in order.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert!(FileDialog::new(FileDialogKind::Open).filters().is_empty());
    /// ```
    pub fn filters(&self) -> &[(String, Vec<String>)] {
        &self.filters
    }

    /// Whether several items may be picked.
    /// ```
    /// # use syrp_peer_api::{FileDialog, FileDialogKind};
    /// assert!(!FileDialog::new(FileDialogKind::Open).allows_multiple());
    /// ```
    pub fn allows_multiple(&self) -> bool {
        self.multiple
    }
}

/// The icon and tone of an alert.
/// ```
/// assert_ne!(syrp_peer_api::AlertKind::Info, syrp_peer_api::AlertKind::Error);
/// ```
#[doc(alias = "MessageBoxIconType")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AlertKind {
    /// Information.
    Info,
    /// A question for the user.
    Question,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// A native alert box request (JUCE's NativeMessageBox role).
/// ```
/// use syrp_peer_api::{AlertDialog, AlertKind};
/// let a = AlertDialog::new(AlertKind::Question, "Unsaved changes", "Save before closing?")
///     .with_buttons(["Save", "Discard", "Cancel"]);
/// assert_eq!((a.kind(), a.title(), a.message()), (AlertKind::Question, "Unsaved changes", "Save before closing?"));
/// assert_eq!(a.buttons(), ["Save", "Discard", "Cancel"]);
/// ```
#[doc(alias = "NativeMessageBox")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertDialog {
    kind: AlertKind,
    title: String,
    message: String,
    buttons: Vec<String>,
}

impl AlertDialog {
    /// An alert with a single "OK" button.
    /// ```
    /// use syrp_peer_api::{AlertDialog, AlertKind};
    /// assert_eq!(AlertDialog::new(AlertKind::Info, "Done", "Export finished").buttons(), ["OK"]);
    /// ```
    pub fn new(kind: AlertKind, title: impl Into<String>, message: impl Into<String>) -> Self {
        AlertDialog {
            kind,
            title: title.into(),
            message: message.into(),
            buttons: vec!["OK".to_owned()],
        }
    }

    /// Replaces the button labels, left to right in reading order; the callback receives the
    /// index of the one pressed. An empty list keeps the single "OK" button.
    /// ```
    /// # use syrp_peer_api::{AlertDialog, AlertKind};
    /// let a = AlertDialog::new(AlertKind::Warning, "", "").with_buttons(Vec::<String>::new());
    /// assert_eq!(a.buttons(), ["OK"]);
    /// ```
    #[must_use]
    pub fn with_buttons<S: Into<String>>(mut self, labels: impl IntoIterator<Item = S>) -> Self {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        if !labels.is_empty() {
            self.buttons = labels;
        }
        self
    }

    /// The icon and tone.
    /// ```
    /// # use syrp_peer_api::{AlertDialog, AlertKind};
    /// assert_eq!(AlertDialog::new(AlertKind::Error, "", "").kind(), AlertKind::Error);
    /// ```
    pub fn kind(&self) -> AlertKind {
        self.kind
    }

    /// The title.
    /// ```
    /// # use syrp_peer_api::{AlertDialog, AlertKind};
    /// assert_eq!(AlertDialog::new(AlertKind::Info, "T", "M").title(), "T");
    /// ```
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The message text.
    /// ```
    /// # use syrp_peer_api::{AlertDialog, AlertKind};
    /// assert_eq!(AlertDialog::new(AlertKind::Info, "T", "M").message(), "M");
    /// ```
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The button labels (never empty).
    /// ```
    /// # use syrp_peer_api::{AlertDialog, AlertKind};
    /// assert_eq!(AlertDialog::new(AlertKind::Info, "", "").buttons().len(), 1);
    /// ```
    pub fn buttons(&self) -> &[String] {
        &self.buttons
    }
}
