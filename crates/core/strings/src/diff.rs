//! [`TextDiff`]: the changes that turn one text into another, in `char` positions.
//!
//! # Algorithm
//! The common prefix and suffix are trimmed first. The middle is compared with Myers' greedy
//! O((N + M) D) shortest-edit-script algorithm ("An O(ND) Difference Algorithm and Its
//! Variations", 1986), keeping one snapshot of the furthest-reaching diagonals per edit distance
//! so the script can be traced back. The snapshots cost O(D²) memory, so the search stops at
//! [`MAX_EDIT_DISTANCE`]; past that, the whole middle is replaced by one removal and one
//! insertion. The result is always correct, and minimal whenever the middle needs at most
//! [`MAX_EDIT_DISTANCE`] single-character edits.

use std::fmt;

/// The edit distance (inserted plus removed `char`s) beyond which [`TextDiff::new`] stops
/// searching for a minimal script and replaces the differing middle wholesale.
const MAX_EDIT_DISTANCE: usize = 1024;

/// One step of a [`TextDiff`]: either a removal of a range of `char`s, or an insertion of text
/// at a `char` position.
///
/// Positions count `char`s (Unicode scalar values), not bytes, and refer to the text as it is
/// after the earlier changes of the same diff have been applied.
///
/// ```
/// use syrp_strings::TextDiff;
/// let diff = TextDiff::new("abc", "abXc");
/// let change = &diff.changes()[0];
/// assert_eq!((change.start(), change.inserted_text(), change.is_deletion()), (2, "X", false));
/// ```
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Change {
    start: usize,
    removed: usize,
    inserted: String,
}

impl Change {
    /// The `char` position where the change applies.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("hello", "help");
    /// assert_eq!(diff.changes()[0].start(), 3);
    /// ```
    pub fn start(&self) -> usize {
        self.start
    }

    /// The number of `char`s removed at [`Change::start`]; zero for an insertion.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("abcd", "ad");
    /// assert_eq!(diff.changes()[0].removed_len(), 2);
    /// ```
    #[doc(alias = "length")]
    pub fn removed_len(&self) -> usize {
        self.removed
    }

    /// The text inserted at [`Change::start`]; empty for a removal.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("ad", "abcd");
    /// assert_eq!(diff.changes()[0].inserted_text(), "bc");
    /// ```
    #[doc(alias = "insertedText")]
    pub fn inserted_text(&self) -> &str {
        &self.inserted
    }

    /// Whether this change removes text (otherwise it inserts text).
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("ab", "b");
    /// assert!(diff.changes()[0].is_deletion());
    /// ```
    #[doc(alias = "isDeletion")]
    pub fn is_deletion(&self) -> bool {
        self.removed > 0
    }

    /// Returns `text` with this one change applied.
    ///
    /// A position or range past the end of `text` is clamped to its end.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("ab", "aXb");
    /// assert_eq!(diff.changes()[0].applied_to("ab"), "aXb");
    /// ```
    #[doc(alias = "appliedTo")]
    pub fn applied_to(&self, text: &str) -> String {
        let start = byte_offset(text, self.start);
        let end = start + byte_offset(&text[start..], self.removed);
        let mut out = String::with_capacity(text.len() - (end - start) + self.inserted.len());
        out.push_str(&text[..start]);
        out.push_str(&self.inserted);
        out.push_str(&text[end..]);
        out
    }
}

impl fmt::Debug for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            start,
            removed,
            inserted,
        } = self;
        if *removed > 0 {
            write!(f, "Remove({start}..{})", start + removed)
        } else {
            write!(f, "Insert({start}, {inserted:?})")
        }
    }
}

/// The changes that turn an original text into a target text (JUCE's `TextDiff`).
///
/// Applying [`TextDiff::changes`] in order to the original reproduces the target exactly. The
/// list is short for similar texts but is not guaranteed to be minimal; see the module
/// documentation for the algorithm.
///
/// ```
/// use syrp_strings::TextDiff;
/// let diff = TextDiff::new("the quick fox", "the slow brown fox");
/// assert_eq!(diff.apply("the quick fox"), "the slow brown fox");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TextDiff {
    changes: Vec<Change>,
}

impl TextDiff {
    /// Computes the changes from `original` to `target`.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("same", "same");
    /// assert!(diff.is_empty());
    /// ```
    pub fn new(original: &str, target: &str) -> Self {
        let a: Vec<char> = original.chars().collect();
        let b: Vec<char> = target.chars().collect();
        let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let (a_rest, b_rest) = (&a[prefix..], &b[prefix..]);
        let suffix = a_rest
            .iter()
            .rev()
            .zip(b_rest.iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
        let a_mid = &a_rest[..a_rest.len() - suffix];
        let b_mid = &b_rest[..b_rest.len() - suffix];

        let ops = myers(a_mid, b_mid).unwrap_or_else(|| {
            let mut ops = vec![Op::Delete; a_mid.len()];
            ops.extend(b_mid.iter().map(|&c| Op::Insert(c)));
            ops
        });
        Self {
            changes: changes_from_ops(prefix, &ops),
        }
    }

    /// The changes, in the order they must be applied.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("abc", "");
    /// assert_eq!(diff.changes().len(), 1);
    /// ```
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The number of changes.
    ///
    /// ```
    /// assert_eq!(syrp_strings::TextDiff::new("ab", "ba").len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Whether the texts were equal, so there are no changes.
    ///
    /// ```
    /// assert!(!syrp_strings::TextDiff::new("a", "b").is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Applies every change in order to `original` and returns the result.
    ///
    /// Given the `original` the diff was computed from, the result equals the target. For any
    /// other text, positions past the end are clamped to the end.
    ///
    /// ```
    /// let diff = syrp_strings::TextDiff::new("héllo wörld", "hello world!");
    /// assert_eq!(diff.apply("héllo wörld"), "hello world!");
    /// ```
    #[doc(alias = "appliedTo")]
    pub fn apply(&self, original: &str) -> String {
        let mut out = String::with_capacity(original.len());
        let mut rest = original.chars();
        let mut out_chars = 0;
        for Change {
            start,
            removed,
            inserted,
        } in &self.changes
        {
            // Copy unchanged text up to the change, then skip the removed range.
            while out_chars < *start {
                match rest.next() {
                    Some(c) => out.push(c),
                    None => break,
                }
                out_chars += 1;
            }
            rest.by_ref().take(*removed).for_each(drop);
            out.push_str(inserted);
            out_chars += inserted.chars().count();
        }
        out.extend(rest);
        out
    }
}

/// One step of an edit script over `char`s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal,
    Delete,
    Insert(char),
}

/// Myers' greedy forward search with traceback. Returns `None` when the edit distance exceeds
/// [`MAX_EDIT_DISTANCE`].
fn myers(a: &[char], b: &[char]) -> Option<Vec<Op>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let limit = MAX_EDIT_DISTANCE.min(a.len() + b.len());
    let off = limit as isize + 1;
    // `v[k + off]`: the furthest x reached on diagonal k = x - y.
    let mut v = vec![0isize; 2 * limit + 3];
    // `trace[d]` holds v[-d..=d] as it was before round d.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    for d in 0..=limit as isize {
        trace.push(v[(off - d) as usize..=(off + d) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let at = |k: isize| v[(k + off) as usize];
            let mut x = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
                at(k + 1)
            } else {
                at(k - 1) + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(k + off) as usize] = x;
            if x >= n && y >= m {
                return Some(trace_back(&trace, b, n, m));
            }
        }
    }
    None
}

/// Walks the snapshots from the end point back to the origin, collecting the edit script.
fn trace_back(trace: &[Vec<isize>], b: &[char], n: isize, m: isize) -> Vec<Op> {
    let (mut x, mut y) = (n, m);
    let mut ops = Vec::new();
    for (d, snapshot) in trace.iter().enumerate().rev() {
        let d = d as isize;
        if d == 0 {
            ops.extend((0..x).map(|_| Op::Equal));
            break;
        }
        let at = |k: isize| snapshot[(k + d) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y {
            ops.push(Op::Equal);
            x -= 1;
            y -= 1;
        }
        ops.push(if x == prev_x {
            Op::Insert(b[prev_y as usize])
        } else {
            Op::Delete
        });
        (x, y) = (prev_x, prev_y);
    }
    ops.reverse();
    ops
}

/// Groups runs of deletions and insertions into [`Change`]s, positions offset by `prefix`.
fn changes_from_ops(prefix: usize, ops: &[Op]) -> Vec<Change> {
    let mut changes: Vec<Change> = Vec::new();
    let mut pos = prefix;
    let mut last_was_edit = false;
    for op in ops {
        match op {
            Op::Equal => {
                pos += 1;
                last_was_edit = false;
                continue;
            }
            Op::Delete => match changes.last_mut() {
                Some(c) if last_was_edit && c.removed > 0 => c.removed += 1,
                _ => changes.push(Change {
                    start: pos,
                    removed: 1,
                    inserted: String::new(),
                }),
            },
            Op::Insert(ch) => {
                match changes.last_mut() {
                    Some(c) if last_was_edit && c.removed == 0 => c.inserted.push(*ch),
                    _ => changes.push(Change {
                        start: pos,
                        removed: 0,
                        inserted: ch.to_string(),
                    }),
                }
                pos += 1;
            }
        }
        last_was_edit = true;
    }
    changes
}

/// The byte offset of the `chars`-th `char` of `text`, clamped to its length.
fn byte_offset(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(i, _)| i)
}
