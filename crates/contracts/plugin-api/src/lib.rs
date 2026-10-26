// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp processor and plugin contract (master plan §5.4 and §5.8, ADR 0003).
//!
//! Parameters: a [`ParamSpec`] per parameter, with a [`ParamRange`] that follows JUCE's
//! documented NormalisableRange semantics, registered in a [`ParamLayout`] that gives each one
//! a dense [`ParamIndex`]. Runtime values live in one contiguous atomic array, [`Params`].
//!
//! The per-block change stream, the `Processor` trait with its `PrepareCtx` and `RtCtx` tokens,
//! `ProcessIo` with MIDI and UMP event I/O, the editor contract, `PluginDescriptor` and the
//! state traits arrive in later increments of WP-0.2-06.
//!
//! ```
//! use syrp_plugin_api::{ParamLayout, ParamRange, ParamSpec, Params};
//! let mut layout = ParamLayout::new();
//! let spec = ParamSpec::new("gain", "Gain", ParamRange::new(0.0, 2.0)).with_default(1.0);
//! let gain = layout.register(spec).unwrap();
//! assert_eq!(Params::new(&layout).unwrap().get(gain), 1.0);
//! ```

use std::fmt;

use syrp_error::{Diagnostic, ErrorCode};

mod layout;
mod param;
mod range;

pub use layout::{ParamLayout, Params};
pub use param::{ParamIndex, ParamSpec, Smoothing};
pub use range::ParamRange;

/// Errors from the processor and parameter contract.
/// ```
/// use syrp_error::Diagnostic;
/// let err = syrp_plugin_api::PluginError::DuplicateParamId;
/// assert_eq!(err.code().as_str(), "SYRP-PLUGIN-0001");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PluginError {
    /// Two parameters were registered with the same string id (`SYRP-PLUGIN-0001`).
    DuplicateParamId,
    /// A parameter's id, range or default is invalid (`SYRP-PLUGIN-0002`).
    InvalidParamSpec,
    /// Memory for audio-path storage could not be reserved (`SYRP-PLUGIN-0003`).
    AllocationFailed,
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PluginError::DuplicateParamId => "duplicate parameter id",
            PluginError::InvalidParamSpec => "invalid parameter id, range or default",
            PluginError::AllocationFailed => "audio-path allocation failed",
        })
    }
}

impl std::error::Error for PluginError {}

impl Diagnostic for PluginError {
    fn code(&self) -> ErrorCode {
        ErrorCode::new(match self {
            PluginError::DuplicateParamId => "SYRP-PLUGIN-0001",
            PluginError::InvalidParamSpec => "SYRP-PLUGIN-0002",
            PluginError::AllocationFailed => "SYRP-PLUGIN-0003",
        })
    }
}
