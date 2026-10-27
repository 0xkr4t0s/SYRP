// SPDX-License-Identifier: AGPL-3.0-only OR BUSL-1.1
//! Metadata chunks into the contract's model. This increment keeps every chunk raw; typed
//! `MARK`, `INST` and text decoding (B3) follows in the next increment of WP-2.1-17.

use syrp_format_api::{FormatError, Metadata, MetadataItem};

/// Text from a file: UTF-8 when it is valid, otherwise one character per byte (ISO 8859-1).
/// Syrp's choice: AIFF only promises ASCII.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// A chunk identifier as text, decoded like [`decode_text`].
pub(crate) fn fourcc_name(id: [u8; 4]) -> String {
    decode_text(&id)
}

/// Collects metadata chunks in file order.
#[derive(Default)]
pub(crate) struct MetaBuilder {
    meta: Metadata,
}

impl MetaBuilder {
    /// Takes one chunk that is neither `COMM`, `SSND` nor `FVER`.
    pub(crate) fn chunk(&mut self, id: [u8; 4], body: Vec<u8>) -> Result<(), FormatError> {
        self.meta.push(MetadataItem::Other(fourcc_name(id), body));
        Ok(())
    }

    /// The items in file order.
    pub(crate) fn finish(self) -> Metadata {
        self.meta
    }
}
