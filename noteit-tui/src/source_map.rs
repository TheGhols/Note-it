//! The units a visual editor is allowed to count in, as types that cannot be
//! mistaken for one another.
//!
//! Five different things get called "position" in a text editor, and the bugs
//! that follow come from one of them being passed where another was meant: a
//! byte offset used as a column, a terminal cell used as a character index, an
//! offset from one version of the text applied to the next. `docs/tui.md`
//! §26.3 makes them normatively incompatible, and this module is that rule
//! written in the type system.
//!
//! There is deliberately no `From<usize>`, no `Deref` and no arithmetic
//! between kinds. Every conversion that is legitimate is a named function that
//! validates, and every conversion that is not simply cannot be spelled.
//!
//! ## Why a generation is part of a position
//!
//! A `SourceOffset` means nothing on its own: byte 40 of one revision of a
//! note is a different place from byte 40 of the next. [`Generation`] is the
//! identity of one version of the [`Draft`](crate::draft::Draft)'s text, and
//! any object carrying offsets carries the generation they were measured in.
//! Handing a stale one back is then a refusal rather than a silent corruption.

/// Which version of the source a position was measured in.
///
/// Monotonic and opaque: it is compared, never arithmetic. A `Draft` hands out
/// a new one after every mutation, so an object built from the text before an
/// edit can be recognised as belonging to the text before that edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(u64);

impl Generation {
    /// The generation of a draft that has not been mutated yet.
    pub const fn first() -> Self {
        Self(0)
    }

    /// The generation after this one.
    ///
    /// Saturating rather than wrapping: at `u64::MAX` a wrap would make a
    /// stale object compare equal to a fresh one, which is the single thing
    /// this type exists to prevent. Reaching it takes more edits than a
    /// person can perform, and stopping is the safe end of that impossibility.
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A byte offset into the source, always on a UTF-8 boundary.
///
/// Bytes rather than characters because this is the unit the source *is*: a
/// range of bytes can be sliced out of a `String` and concatenated back
/// without a table, which is what makes the losslessness property checkable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceOffset(usize);

impl SourceOffset {
    /// The offset `byte`, if it is a real boundary of `source`.
    ///
    /// `None` for an offset past the end or inside a multi-byte sequence:
    /// those are the two ways a byte index becomes a place that does not
    /// exist, and both are caller errors worth seeing rather than clamping.
    pub fn in_source(source: &str, byte: usize) -> Option<Self> {
        source.is_char_boundary(byte).then_some(Self(byte))
    }

    /// The offset `byte` without checking, for a caller that just produced it
    /// from a boundary it already knows — a scanner walking `char_indices`,
    /// for instance.
    pub(crate) const fn trusted(byte: usize) -> Self {
        Self(byte)
    }

    pub const fn get(self) -> usize {
        self.0
    }
}

/// A half-open `[start, end)` span of source bytes.
///
/// Half-open because that is the only convention under which adjacent ranges
/// tile without overlapping and an empty range is expressible — and B.1's
/// whole contract is that the lexemes tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRange {
    start: SourceOffset,
    end: SourceOffset,
}

impl SourceRange {
    /// The range `start..end`, if both are boundaries of `source` and ordered.
    pub fn in_source(source: &str, start: usize, end: usize) -> Option<Self> {
        if start > end {
            return None;
        }
        Some(Self {
            start: SourceOffset::in_source(source, start)?,
            end: SourceOffset::in_source(source, end)?,
        })
    }

    pub(crate) const fn trusted(start: usize, end: usize) -> Self {
        Self {
            start: SourceOffset::trusted(start),
            end: SourceOffset::trusted(end),
        }
    }

    pub const fn start(self) -> usize {
        self.start.get()
    }

    pub const fn end(self) -> usize {
        self.end.get()
    }

    pub const fn len(self) -> usize {
        self.end.get() - self.start.get()
    }

    pub const fn is_empty(self) -> bool {
        self.start.get() == self.end.get()
    }

    pub const fn contains(self, offset: SourceOffset) -> bool {
        self.start.get() <= offset.get() && offset.get() < self.end.get()
    }

    /// Whether `self` covers every byte of `other`.
    pub const fn covers(self, other: Self) -> bool {
        self.start.get() <= other.start.get() && other.end.get() <= self.end.get()
    }

    /// The bytes of `source` this range names.
    ///
    /// Panics only if the range was built for a different string, which the
    /// constructors above are what prevent.
    pub fn slice(self, source: &str) -> &str {
        &source[self.start.get()..self.end.get()]
    }
}

/// A `(line, scalar column)` pair, the unit [`Draft`](crate::draft::Draft)
/// counts in.
///
/// Kept as its own type because the raw editor's history, cursor and selection
/// are all expressed in it and none of that is being rewritten: the visual
/// layer converts at the boundary instead, by the named functions below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ScalarPosition {
    pub line: usize,
    pub column: usize,
}

/// An index counted in extended grapheme clusters within one run or block.
///
/// Separate from `SourceOffset` because the two disagree for every character
/// that is not one byte, and separate from `DisplayColumn` because a grapheme
/// may be two cells wide. B.1 does not produce these; the type is declared
/// here so B.2 cannot invent a second spelling of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GraphemeIndex(pub usize);

/// A terminal cell column, for layout and nothing else.
///
/// Never a text position: a cursor stored as a display column would move when
/// the font, the width table or the terminal changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DisplayColumn(pub usize);

// ---------------------------------------------------------------------------
// The named conversions, and only these
// ---------------------------------------------------------------------------

/// The byte offset of a `(line, scalar column)` position in `source`.
///
/// This is the bridge described in §26.3: the bytes of every earlier line plus
/// one for each `\n` that ended them, then the scalar boundary within the
/// line. A column past the end of its line lands at the line's end rather than
/// in the next one, because that is what the raw editor's own clamping means.
///
/// `None` when the line does not exist, which is the one case a caller must
/// not paper over.
pub fn offset_of_scalar(source: &str, position: ScalarPosition) -> Option<SourceOffset> {
    let mut lines = source.split('\n');
    let mut offset = 0usize;

    for _ in 0..position.line {
        let line = lines.next()?;
        offset += line.len() + 1;
    }

    let line = lines.next()?;
    let within = line
        .char_indices()
        .nth(position.column)
        .map_or(line.len(), |(byte, _)| byte);

    Some(SourceOffset::trusted(offset + within))
}

/// The `(line, scalar column)` position of a byte offset in `source`.
///
/// The inverse of [`offset_of_scalar`] for every offset that is a UTF-8
/// boundary; for one that is not, `None`, because there is no position there
/// to name.
pub fn scalar_of_offset(source: &str, offset: SourceOffset) -> Option<ScalarPosition> {
    let byte = offset.get();
    if byte > source.len() || !source.is_char_boundary(byte) {
        return None;
    }

    let before = &source[..byte];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..byte].chars().count();

    Some(ScalarPosition { line, column })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generation_never_wraps_into_an_older_one() {
        let mut generation = Generation::first();
        for _ in 0..5 {
            let next = generation.next();
            assert!(next > generation);
            generation = next;
        }
    }

    #[test]
    fn an_offset_inside_a_character_does_not_exist() {
        let source = "café";
        assert!(SourceOffset::in_source(source, 3).is_some());
        // `é` is two bytes at 3..5, so 4 is inside it.
        assert!(SourceOffset::in_source(source, 4).is_none());
        assert!(SourceOffset::in_source(source, 5).is_some());
        assert!(SourceOffset::in_source(source, 6).is_none());
    }

    #[test]
    fn an_inverted_range_is_refused() {
        assert!(SourceRange::in_source("abcdef", 4, 2).is_none());
        assert!(SourceRange::in_source("abcdef", 2, 4).is_some());
    }

    #[test]
    fn scalar_and_byte_positions_round_trip_through_unicode() {
        let source = "a\nçé日\nfim";
        for (index, _) in source.char_indices().chain([(source.len(), ' ')]) {
            let offset = SourceOffset::in_source(source, index).expect("a boundary");
            let scalar = scalar_of_offset(source, offset).expect("a position");
            assert_eq!(
                offset_of_scalar(source, scalar),
                Some(offset),
                "round trip at byte {index}"
            );
        }
    }

    #[test]
    fn a_crlf_line_ending_leaves_the_carriage_return_on_the_line() {
        // `Draft` splits on `\n` only, so `\r` is the last scalar of the line.
        // The bridge has to agree with it or every offset after one is wrong.
        let source = "a\r\nb";
        let position = ScalarPosition { line: 1, column: 0 };
        assert_eq!(
            offset_of_scalar(source, position),
            SourceOffset::in_source(source, 3)
        );
    }

    #[test]
    fn a_column_past_the_end_of_a_line_stops_at_its_end() {
        let source = "ab\ncd";
        let position = ScalarPosition {
            line: 0,
            column: 99,
        };
        assert_eq!(
            offset_of_scalar(source, position),
            SourceOffset::in_source(source, 2)
        );
    }

    #[test]
    fn a_line_that_does_not_exist_has_no_offset() {
        assert!(offset_of_scalar("ab\ncd", ScalarPosition { line: 7, column: 0 }).is_none());
    }
}
