// SPDX-License-Identifier: MIT OR Apache-2.0
//! Drag-and-drop of files and text from other applications onto a peer.

use std::path::PathBuf;

use syrp_geom::Point;

/// What is being dragged.
/// ```
/// use syrp_peer_api::DragData;
/// let d = DragData::Files(vec!["/tmp/kick.wav".into()]);
/// assert!(matches!(d, DragData::Files(ref f) if f.len() == 1));
/// ```
#[doc(alias = "FileDragAndDropTarget")]
#[doc(alias = "TextDragAndDropTarget")]
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DragData {
    /// Paths of files or folders, in the order the platform gave them.
    Files(Vec<PathBuf>),
    /// Plain text.
    Text(String),
}

/// Where a drag is in its sequence.
/// ```
/// assert_ne!(syrp_peer_api::DragPhase::Enter, syrp_peer_api::DragPhase::Drop);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DragPhase {
    /// The drag entered the peer.
    Enter,
    /// The drag moved within the peer.
    Over,
    /// The drag left the peer, or was cancelled, without dropping.
    Leave,
    /// The data was dropped on the peer. Ends the sequence.
    Drop,
}

/// One step of an external drag over a peer. Answer `Enter` and `Over` with
/// [`Peer::accept_drop`](crate::Peer::accept_drop) so the platform shows the right cursor; a
/// `Drop` arrives only if the last answer accepted.
/// ```
/// use syrp_geom::Point;
/// use syrp_peer_api::{DragData, DragEvent, DragPhase};
/// let ev = DragEvent::new(DragPhase::Over, Point::new(5.0, 6.0), DragData::Text("hi".into()));
/// assert_eq!((ev.phase(), ev.position()), (DragPhase::Over, Point::new(5.0, 6.0)));
/// assert_eq!(ev.data(), &DragData::Text("hi".into()));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DragEvent {
    phase: DragPhase,
    position: Point<f64>,
    data: DragData,
}

impl DragEvent {
    /// A drag step at `position` carrying `data`.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{DragData, DragEvent, DragPhase};
    /// let _ = DragEvent::new(DragPhase::Enter, Point::new(0.0, 0.0), DragData::Files(vec![]));
    /// ```
    pub fn new(phase: DragPhase, position: Point<f64>, data: DragData) -> Self {
        DragEvent {
            phase,
            position,
            data,
        }
    }

    /// Where in the sequence this step is.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{DragData, DragEvent, DragPhase};
    /// let ev = DragEvent::new(DragPhase::Leave, Point::new(0.0, 0.0), DragData::Text(String::new()));
    /// assert_eq!(ev.phase(), DragPhase::Leave);
    /// ```
    pub fn phase(&self) -> DragPhase {
        self.phase
    }

    /// Position in logical units relative to the peer.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{DragData, DragEvent, DragPhase};
    /// let ev = DragEvent::new(DragPhase::Over, Point::new(2.0, 3.0), DragData::Text(String::new()));
    /// assert_eq!(ev.position().y, 3.0);
    /// ```
    pub fn position(&self) -> Point<f64> {
        self.position
    }

    /// The dragged files or text.
    /// ```
    /// # use syrp_geom::Point; use syrp_peer_api::{DragData, DragEvent, DragPhase};
    /// let ev = DragEvent::new(DragPhase::Drop, Point::new(0.0, 0.0), DragData::Text("x".into()));
    /// assert!(matches!(ev.data(), DragData::Text(t) if t == "x"));
    /// ```
    pub fn data(&self) -> &DragData {
        &self.data
    }
}
