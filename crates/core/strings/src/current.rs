//! The process-wide current translations and [`translate`] (JUCE's `TRANS`).
//!
//! Doc examples here are `standalone_crate` so they run in their own process.
//!
//! The mappings sit behind a reader-writer lock, so every function here is safe to call from
//! any thread. They lock (and [`translate`] may allocate), so none of them is for the audio
//! thread: translate text on the message thread and hand the result over.

use std::borrow::Cow;
use std::sync::{Arc, PoisonError, RwLock};

use crate::LocalisedStrings;

static CURRENT: RwLock<Option<Arc<LocalisedStrings>>> = RwLock::new(None);

/// Installs `mappings` as the process-wide translations, or removes them with `None`.
///
/// Later [`translate`] calls on every thread use the new set. Callers that already hold the
/// previous set from [`current_mappings`] keep it until they drop it.
///
/// Not for the audio thread: it takes a write lock.
///
/// ```standalone_crate
/// use syrp_strings::{set_current_mappings, translate, LocalisedStrings};
/// let fr = LocalisedStrings::parse(r#""Cancel" = "Annuler""#, false).unwrap();
/// set_current_mappings(Some(fr));
/// assert_eq!(translate("Cancel"), "Annuler");
/// set_current_mappings(None);
/// assert_eq!(translate("Cancel"), "Cancel");
/// ```
#[doc(alias = "setCurrentMappings")]
pub fn set_current_mappings(mappings: Option<LocalisedStrings>) {
    let next = mappings.map(Arc::new);
    // Swap under the lock, drop the old set after releasing it.
    let previous = std::mem::replace(
        &mut *CURRENT.write().unwrap_or_else(PoisonError::into_inner),
        next,
    );
    drop(previous);
}

/// The process-wide translations, if any are installed.
///
/// Not for the audio thread: it takes a read lock.
///
/// ```standalone_crate
/// use syrp_strings::{current_mappings, set_current_mappings, LocalisedStrings};
/// set_current_mappings(Some(LocalisedStrings::parse("language: Dutch", false).unwrap()));
/// assert_eq!(current_mappings().unwrap().language_name(), "Dutch");
/// # set_current_mappings(None);
/// ```
#[doc(alias = "getCurrentMappings")]
pub fn current_mappings() -> Option<Arc<LocalisedStrings>> {
    CURRENT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Translates `text` with the process-wide mappings (JUCE's `TRANS` and `translate`).
///
/// Returns the input borrowed when there are no mappings or no entry for it, and an owned copy
/// of the translation otherwise. Safe from any thread; not for the audio thread (it takes a read
/// lock and may allocate).
///
/// ```standalone_crate
/// // With no mappings installed, text comes back unchanged and borrowed.
/// let out = syrp_strings::translate("Untranslated");
/// assert_eq!(out, "Untranslated");
/// ```
#[doc(alias = "TRANS")]
pub fn translate(text: &str) -> Cow<'_, str> {
    let guard = CURRENT.read().unwrap_or_else(PoisonError::into_inner);
    match guard.as_deref().and_then(|m| m.get(text)) {
        Some(translated) => Cow::Owned(translated.to_owned()),
        None => Cow::Borrowed(text),
    }
}
