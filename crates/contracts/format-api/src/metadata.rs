// SPDX-License-Identifier: MIT OR Apache-2.0
//! The metadata model: typed common chunks with an escape for the rest.

/// File metadata: an ordered list of [`MetadataItem`]s, kept in file order.
///
/// Readers fill it with everything they recognise; writers store the items their format can
/// hold and skip the rest. Text tags are unique per [`Tag`] when set through
/// [`Metadata::set_tag`].
///
/// ```
/// use syrp_format_api::{CuePoint, Metadata, MetadataItem, Tag};
/// let mut meta = Metadata::new();
/// meta.set_tag(Tag::Artist, "Someone");
/// meta.push(MetadataItem::Cue(CuePoint::new(1, 4_800).with_label("verse")));
/// meta.push(MetadataItem::Other("iXML".into(), b"<BWFXML/>".to_vec()));
/// assert_eq!(meta.tag(&Tag::Artist), Some("Someone"));
/// assert_eq!(meta.len(), 3);
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
#[doc(alias = "metadataValues")]
pub struct Metadata {
    items: Vec<MetadataItem>,
}

impl Metadata {
    /// An empty list.
    ///
    /// ```
    /// assert!(syrp_format_api::Metadata::new().is_empty());
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends `item`.
    ///
    /// ```
    /// use syrp_format_api::{Bext, Metadata, MetadataItem};
    /// let mut meta = Metadata::new();
    /// meta.push(MetadataItem::Bext(Bext::new()));
    /// assert!(meta.bext().is_some());
    /// ```
    pub fn push(&mut self, item: MetadataItem) {
        self.items.push(item);
    }

    /// All items in order.
    ///
    /// ```
    /// use syrp_format_api::{Metadata, MetadataItem, SampleLoop, LoopKind};
    /// let mut meta = Metadata::new();
    /// meta.push(MetadataItem::Loop(SampleLoop::new(LoopKind::Forward, 0, 99).unwrap()));
    /// let loops = meta.items().iter().filter(|i| matches!(i, MetadataItem::Loop(_))).count();
    /// assert_eq!(loops, 1);
    /// ```
    #[must_use]
    pub fn items(&self) -> &[MetadataItem] {
        &self.items
    }

    /// Number of items.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Metadata::new().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether there are no items.
    ///
    /// ```
    /// assert!(syrp_format_api::Metadata::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The value of the first text tag `tag`, if any.
    ///
    /// ```
    /// use syrp_format_api::{Metadata, Tag};
    /// let mut meta = Metadata::new();
    /// meta.set_tag(Tag::Custom("MOOD".into()), "calm");
    /// assert_eq!(meta.tag(&Tag::Custom("MOOD".into())), Some("calm"));
    /// assert_eq!(meta.tag(&Tag::Title), None);
    /// ```
    #[must_use]
    pub fn tag(&self, tag: &Tag) -> Option<&str> {
        self.items.iter().find_map(|item| match item {
            MetadataItem::Tag(t, value) if t == tag => Some(value.as_str()),
            _ => None,
        })
    }

    /// Sets text tag `tag` to `value`, replacing every earlier value of that tag in place
    /// (the first keeps its position) or appending when there was none.
    ///
    /// ```
    /// use syrp_format_api::{Metadata, Tag};
    /// let mut meta = Metadata::new();
    /// meta.set_tag(Tag::Title, "a");
    /// meta.set_tag(Tag::Title, "b");
    /// assert_eq!((meta.tag(&Tag::Title), meta.len()), (Some("b"), 1));
    /// ```
    pub fn set_tag(&mut self, tag: Tag, value: impl Into<String>) {
        let value = value.into();
        let mut found = false;
        self.items.retain_mut(|item| match item {
            MetadataItem::Tag(t, v) if *t == tag => {
                if found {
                    return false;
                }
                found = true;
                *v = value.clone();
                true
            }
            _ => true,
        });
        if !found {
            self.items.push(MetadataItem::Tag(tag, value));
        }
    }

    /// The first broadcast extension chunk, if any.
    ///
    /// ```
    /// assert!(syrp_format_api::Metadata::new().bext().is_none());
    /// ```
    #[must_use]
    pub fn bext(&self) -> Option<&Bext> {
        self.items.iter().find_map(|item| match item {
            MetadataItem::Bext(b) => Some(b),
            _ => None,
        })
    }
}

/// One piece of file metadata.
///
/// ```
/// use syrp_format_api::{MetadataItem, Tag};
/// let item = MetadataItem::Tag(Tag::Genre, "Ambient".into());
/// assert!(matches!(item, MetadataItem::Tag(Tag::Genre, _)));
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MetadataItem {
    /// A Broadcast Wave `bext` chunk.
    Bext(Bext),
    /// A marker at a sample position (WAV `cue `/`LIST adtl`, AIFF `MARK`).
    Cue(CuePoint),
    /// A loop region (WAV `smpl`, AIFF `INST`).
    Loop(SampleLoop),
    /// A text tag (RIFF `INFO`, ID3, Vorbis comments).
    Tag(Tag, String),
    /// Anything else: an identifier (chunk id, frame id or key) and its raw bytes.
    Other(String, Vec<u8>),
}

/// The name of a text tag. Formats map these to their own keys.
///
/// ```
/// use syrp_format_api::Tag;
/// assert_ne!(Tag::Title, Tag::Custom("TITLE".into()));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Tag {
    /// Title of the work.
    Title,
    /// Performing artist.
    Artist,
    /// Album or collection.
    Album,
    /// Genre.
    Genre,
    /// Creation or release date, as written in the file.
    Date,
    /// Free comment.
    Comment,
    /// Track number, as written in the file (for example `"3"` or `"3/12"`).
    TrackNumber,
    /// Copyright notice.
    Copyright,
    /// Software that wrote the file.
    Encoder,
    /// A key the list above does not name, as spelled in the file.
    Custom(String),
}

/// A Broadcast Wave Format `bext` chunk. Text fields are UTF-8 here; writers truncate them to
/// the chunk's fixed sizes.
///
/// ```
/// use syrp_format_api::Bext;
/// let bext = Bext::new().with_originator("Field recorder").with_time_reference(48_000 * 3600);
/// assert_eq!(bext.time_reference(), 172_800_000);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Bext {
    description: String,
    originator: String,
    originator_reference: String,
    origination_date: String,
    origination_time: String,
    time_reference: u64,
    coding_history: String,
}

impl Bext {
    /// An empty chunk with a time reference of 0.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().description(), "");
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sample count since midnight of the first sample in the file.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().time_reference(), 0);
    /// ```
    #[must_use]
    #[doc(alias = "bwavTimeReference")]
    pub fn time_reference(&self) -> u64 {
        self.time_reference
    }

    /// Sets [`time_reference`](Bext::time_reference).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_time_reference(9).time_reference(), 9);
    /// ```
    #[must_use]
    pub fn with_time_reference(mut self, samples: u64) -> Self {
        self.time_reference = samples;
        self
    }

    /// Free description of the sound, up to 256 bytes in the chunk.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_description("x").description(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavDescription")]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Sets [`description`](Bext::description).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_description("y").description(), "y");
    /// ```
    #[must_use]
    pub fn with_description(mut self, value: impl Into<String>) -> Self {
        self.description = value.into();
        self
    }

    /// Name of the producer or the recording device, up to 32 bytes.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_originator("x").originator(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavOriginator")]
    pub fn originator(&self) -> &str {
        &self.originator
    }

    /// Sets [`originator`](Bext::originator).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_originator("y").originator(), "y");
    /// ```
    #[must_use]
    pub fn with_originator(mut self, value: impl Into<String>) -> Self {
        self.originator = value.into();
        self
    }

    /// Unique reference given by the originator, up to 32 bytes.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_originator_reference("x").originator_reference(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavOriginatorRef")]
    pub fn originator_reference(&self) -> &str {
        &self.originator_reference
    }

    /// Sets [`originator_reference`](Bext::originator_reference).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_originator_reference("y").originator_reference(), "y");
    /// ```
    #[must_use]
    pub fn with_originator_reference(mut self, value: impl Into<String>) -> Self {
        self.originator_reference = value.into();
        self
    }

    /// Creation date as `yyyy-mm-dd`.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_origination_date("x").origination_date(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavOriginationDate")]
    pub fn origination_date(&self) -> &str {
        &self.origination_date
    }

    /// Sets [`origination_date`](Bext::origination_date).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_origination_date("y").origination_date(), "y");
    /// ```
    #[must_use]
    pub fn with_origination_date(mut self, value: impl Into<String>) -> Self {
        self.origination_date = value.into();
        self
    }

    /// Creation time as `hh:mm:ss`.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_origination_time("x").origination_time(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavOriginationTime")]
    pub fn origination_time(&self) -> &str {
        &self.origination_time
    }

    /// Sets [`origination_time`](Bext::origination_time).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_origination_time("y").origination_time(), "y");
    /// ```
    #[must_use]
    pub fn with_origination_time(mut self, value: impl Into<String>) -> Self {
        self.origination_time = value.into();
        self
    }

    /// Coding history lines, one process per line.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_coding_history("x").coding_history(), "x");
    /// ```
    #[must_use]
    #[doc(alias = "bwavCodingHistory")]
    pub fn coding_history(&self) -> &str {
        &self.coding_history
    }

    /// Sets [`coding_history`](Bext::coding_history).
    ///
    /// ```
    /// assert_eq!(syrp_format_api::Bext::new().with_coding_history("y").coding_history(), "y");
    /// ```
    #[must_use]
    pub fn with_coding_history(mut self, value: impl Into<String>) -> Self {
        self.coding_history = value.into();
        self
    }
}

/// A marker at a sample position, with an optional label.
///
/// ```
/// use syrp_format_api::CuePoint;
/// let cue = CuePoint::new(2, 96_000).with_label("chorus");
/// assert_eq!((cue.id(), cue.position(), cue.label()), (2, 96_000, Some("chorus")));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CuePoint {
    id: u32,
    position: u64,
    label: Option<String>,
}

impl CuePoint {
    /// A cue with identifier `id` at frame `position`, without a label.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::CuePoint::new(1, 0).label(), None);
    /// ```
    #[must_use]
    pub fn new(id: u32, position: u64) -> Self {
        Self {
            id,
            position,
            label: None,
        }
    }

    /// Sets the label.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::CuePoint::new(1, 0).with_label("a").label(), Some("a"));
    /// ```
    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The file's identifier for this cue.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::CuePoint::new(7, 0).id(), 7);
    /// ```
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Position in frames from the start of the file.
    ///
    /// ```
    /// assert_eq!(syrp_format_api::CuePoint::new(1, 44).position(), 44);
    /// ```
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The label, if the file gives one.
    ///
    /// ```
    /// assert!(syrp_format_api::CuePoint::new(1, 0).label().is_none());
    /// ```
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }
}

/// How a sampler plays a loop.
///
/// ```
/// assert_ne!(syrp_format_api::LoopKind::Forward, syrp_format_api::LoopKind::PingPong);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LoopKind {
    /// Start to end, then jump back to start.
    Forward,
    /// Start to end, then end to start, and so on.
    PingPong,
    /// End to start, then jump back to end.
    Backward,
}

/// A loop region in frames, both ends inclusive, with a play count.
///
/// ```
/// use syrp_format_api::{LoopKind, SampleLoop};
/// let lp = SampleLoop::new(LoopKind::PingPong, 100, 199).unwrap().with_play_count(4);
/// assert_eq!((lp.start(), lp.end(), lp.play_count()), (100, 199, 4));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleLoop {
    kind: LoopKind,
    start: u64,
    end: u64,
    play_count: u32,
}

impl SampleLoop {
    /// A loop over frames `start..=end` that repeats forever, or `None` when `end < start`.
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// assert!(SampleLoop::new(LoopKind::Forward, 10, 9).is_none());
    /// assert_eq!(SampleLoop::new(LoopKind::Forward, 0, 0).unwrap().play_count(), 0);
    /// ```
    #[must_use]
    pub fn new(kind: LoopKind, start: u64, end: u64) -> Option<Self> {
        (start <= end).then_some(Self {
            kind,
            start,
            end,
            play_count: 0,
        })
    }

    /// Sets how many times the loop plays; 0 means forever.
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// let lp = SampleLoop::new(LoopKind::Forward, 0, 9).unwrap().with_play_count(2);
    /// assert_eq!(lp.play_count(), 2);
    /// ```
    #[must_use]
    pub fn with_play_count(mut self, count: u32) -> Self {
        self.play_count = count;
        self
    }

    /// The play direction.
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// assert_eq!(SampleLoop::new(LoopKind::Backward, 0, 1).unwrap().kind(), LoopKind::Backward);
    /// ```
    #[must_use]
    pub fn kind(&self) -> LoopKind {
        self.kind
    }

    /// First frame of the loop.
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// assert_eq!(SampleLoop::new(LoopKind::Forward, 5, 9).unwrap().start(), 5);
    /// ```
    #[must_use]
    pub fn start(&self) -> u64 {
        self.start
    }

    /// Last frame of the loop (inclusive).
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// assert_eq!(SampleLoop::new(LoopKind::Forward, 5, 9).unwrap().end(), 9);
    /// ```
    #[must_use]
    pub fn end(&self) -> u64 {
        self.end
    }

    /// How many times the loop plays; 0 means forever.
    ///
    /// ```
    /// use syrp_format_api::{LoopKind, SampleLoop};
    /// assert_eq!(SampleLoop::new(LoopKind::Forward, 5, 9).unwrap().play_count(), 0);
    /// ```
    #[must_use]
    pub fn play_count(&self) -> u32 {
        self.play_count
    }
}
