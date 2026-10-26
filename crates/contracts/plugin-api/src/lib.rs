// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Syrp processor and plugin contract (master plan §5.4 and §5.8, ADR 0003).
//!
//! - [`Processor`]: `prepare` with a [`PrepareCtx`], `process` with [`ProcessIo`], [`Params`]
//!   and an [`RtCtx`], plus layout negotiation, latency, [`Tail`], bypass and precision.
//! - Parameters: a [`ParamSpec`] per parameter, with a [`ParamRange`] that follows JUCE's
//!   documented NormalisableRange semantics, registered in a [`ParamLayout`] that gives each
//!   one a dense [`ParamIndex`]. Values live in one atomic array, [`Params`]; each block
//!   carries one sorted [`ParamChanges`] stream and `syrp-midi` MIDI 1.0 and UMP buffers.
//!
//! - Editor: [`Editor`] builds its UI through an opaque [`EditorHost`] and returns a
//!   [`NodeHandle`]; it sees parameters only through [`ParamHandles`].
//! - [`PluginDescriptor`] and [`PluginState`], saved in a versioned [`StateEnvelope`].
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

mod changes;
mod descriptor;
mod editor;
mod io;
mod layout;
mod param;
mod processor;
mod range;
mod state;

pub use changes::{ParamChange, ParamChanges};
pub use descriptor::{PluginCategory, PluginDescriptor, PluginFormat};
pub use editor::{
    Editor, EditorError, EditorHost, GesturePhase, NodeHandle, ParamGestures, ParamHandles,
    SizeConstraints,
};
pub use io::ProcessIo;
pub use layout::{ParamLayout, Params};
pub use param::{ParamIndex, ParamSpec, Smoothing};
pub use processor::{PrepareCtx, Prepared, Processor, RtCtx, Tail, prepare_processor};
pub use range::ParamRange;
pub use state::{PluginState, StateEnvelope, load_state, save_state};

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
    /// A change was pushed to a full [`ParamChanges`] stream (`SYRP-PLUGIN-0004`).
    ParamChangesFull,
    /// Double precision was asked of a processor without it (`SYRP-PLUGIN-0005`).
    PrecisionUnsupported,
    /// The processor could not prepare for the configuration (`SYRP-PLUGIN-0006`).
    PrepareFailed,
    /// A plugin descriptor is missing a name, vendor or id, or an id is malformed
    /// (`SYRP-PLUGIN-0009`).
    InvalidDescriptor,
    /// A state blob is not a well-formed envelope, or its payload is invalid
    /// (`SYRP-PLUGIN-0010`).
    StateMalformed,
    /// A state blob was written by a newer state or envelope version (`SYRP-PLUGIN-0011`).
    StateTooNew,
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PluginError::DuplicateParamId => "duplicate parameter id",
            PluginError::InvalidParamSpec => "invalid parameter id, range or default",
            PluginError::AllocationFailed => "audio-path allocation failed",
            PluginError::ParamChangesFull => "parameter change stream is full",
            PluginError::PrecisionUnsupported => "processor does not support double precision",
            PluginError::PrepareFailed => "processor could not prepare",
            PluginError::InvalidDescriptor => "invalid plugin descriptor",
            PluginError::StateMalformed => "malformed plugin state",
            PluginError::StateTooNew => "plugin state is from a newer version",
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
            PluginError::ParamChangesFull => "SYRP-PLUGIN-0004",
            PluginError::PrecisionUnsupported => "SYRP-PLUGIN-0005",
            PluginError::PrepareFailed => "SYRP-PLUGIN-0006",
            PluginError::InvalidDescriptor => "SYRP-PLUGIN-0009",
            PluginError::StateMalformed => "SYRP-PLUGIN-0010",
            PluginError::StateTooNew => "SYRP-PLUGIN-0011",
        })
    }
}
