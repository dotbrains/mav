use super::*;

/// A slice into a [`Buffer`] that is being edited in a [`MultiBuffer`].
#[derive(Clone, Debug)]
pub(crate) struct Excerpt {
    /// The location of the excerpt in the [`MultiBuffer`]
    pub(crate) path_key: PathKey,
    pub(crate) path_key_index: PathKeyIndex,
    pub(crate) buffer_id: BufferId,
    /// The range of the buffer to be shown in the excerpt
    pub(crate) range: ExcerptRange<text::Anchor>,

    /// The last row in the excerpted slice of the buffer
    pub(crate) max_buffer_row: BufferRow,
    /// A summary of the text in the excerpt
    pub(crate) text_summary: TextSummary,
    pub(crate) has_trailing_newline: bool,
}

/// A range of text from a single [`Buffer`], to be shown as an `Excerpt`.
/// These ranges are relative to the buffer itself
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ExcerptRange<T> {
    /// The full range of text to be shown in the excerpt.
    pub context: Range<T>,
    /// The primary range of text to be highlighted in the excerpt.
    /// In a multi-buffer search, this would be the text that matched the search
    pub primary: Range<T>,
}

impl<T: Clone> ExcerptRange<T> {
    pub fn new(context: Range<T>) -> Self {
        Self {
            context: context.clone(),
            primary: context,
        }
    }
}

impl ExcerptRange<text::Anchor> {
    pub fn contains(&self, t: &text::Anchor, snapshot: &BufferSnapshot) -> bool {
        self.context.start.cmp(t, snapshot).is_le() && self.context.end.cmp(t, snapshot).is_ge()
    }
}

#[derive(Clone, Debug)]
pub struct ExcerptSummary {
    pub(super) path_key: PathKey,
    pub(super) path_key_index: Option<PathKeyIndex>,
    pub(super) max_anchor: Option<text::Anchor>,
    pub(super) widest_line_number: u32,
    pub(super) text: MBTextSummary,
    pub(super) count: usize,
}

impl ExcerptSummary {
    pub fn min() -> Self {
        ExcerptSummary {
            path_key: PathKey::min(),
            path_key_index: None,
            max_anchor: None,
            widest_line_number: 0,
            text: MBTextSummary::default(),
            count: 0,
        }
    }

    pub(super) fn len(&self) -> ExcerptOffset {
        ExcerptDimension(self.text.len)
    }
}

#[derive(Debug, Clone)]
pub struct DiffTransformSummary {
    pub(super) input: MBTextSummary,
    pub(super) output: MBTextSummary,
}

/// Summary of a string of text.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MBTextSummary {
    /// Length in bytes.
    pub len: MultiBufferOffset,
    /// Length in UTF-8.
    pub chars: usize,
    /// Length in UTF-16 code units
    pub len_utf16: OffsetUtf16,
    /// A point representing the number of lines and the length of the last line.
    ///
    /// In other words, it marks the point after the last byte in the text, (if
    /// EOF was a character, this would be its position).
    pub lines: Point,
    /// How many `char`s are in the first line
    pub first_line_chars: u32,
    /// How many `char`s are in the last line
    pub last_line_chars: u32,
    /// How many UTF-16 code units are in the last line
    pub last_line_len_utf16: u32,
    /// The row idx of the longest row
    pub longest_row: u32,
    /// How many `char`s are in the longest row
    pub longest_row_chars: u32,
}

impl From<TextSummary> for MBTextSummary {
    fn from(summary: TextSummary) -> Self {
        MBTextSummary {
            len: MultiBufferOffset(summary.len),
            chars: summary.chars,
            len_utf16: summary.len_utf16,
            lines: summary.lines,
            first_line_chars: summary.first_line_chars,
            last_line_chars: summary.last_line_chars,
            last_line_len_utf16: summary.last_line_len_utf16,
            longest_row: summary.longest_row,
            longest_row_chars: summary.longest_row_chars,
        }
    }
}

impl From<MBTextSummary> for TextSummary {
    fn from(summary: MBTextSummary) -> Self {
        TextSummary {
            len: summary.len.0,
            chars: summary.chars,
            len_utf16: summary.len_utf16,
            lines: summary.lines,
            first_line_chars: summary.first_line_chars,
            last_line_chars: summary.last_line_chars,
            last_line_len_utf16: summary.last_line_len_utf16,
            longest_row: summary.longest_row,
            longest_row_chars: summary.longest_row_chars,
        }
    }
}

impl From<&str> for MBTextSummary {
    fn from(text: &str) -> Self {
        MBTextSummary::from(TextSummary::from(text))
    }
}

impl MultiBufferDimension for MBTextSummary {
    type TextDimension = TextSummary;

    fn from_summary(summary: &MBTextSummary) -> Self {
        *summary
    }

    fn add_text_dim(&mut self, summary: &Self::TextDimension) {
        *self += *summary;
    }

    fn add_mb_text_summary(&mut self, summary: &MBTextSummary) {
        *self += *summary;
    }
}

impl AddAssign for MBTextSummary {
    fn add_assign(&mut self, other: MBTextSummary) {
        let joined_chars = self.last_line_chars + other.first_line_chars;
        if joined_chars > self.longest_row_chars {
            self.longest_row = self.lines.row;
            self.longest_row_chars = joined_chars;
        }
        if other.longest_row_chars > self.longest_row_chars {
            self.longest_row = self.lines.row + other.longest_row;
            self.longest_row_chars = other.longest_row_chars;
        }

        if self.lines.row == 0 {
            self.first_line_chars += other.first_line_chars;
        }

        if other.lines.row == 0 {
            self.last_line_chars += other.first_line_chars;
            self.last_line_len_utf16 += other.last_line_len_utf16;
        } else {
            self.last_line_chars = other.last_line_chars;
            self.last_line_len_utf16 = other.last_line_len_utf16;
        }

        self.chars += other.chars;
        self.len += other.len;
        self.len_utf16 += other.len_utf16;
        self.lines += other.lines;
    }
}

impl AddAssign<TextSummary> for MBTextSummary {
    fn add_assign(&mut self, other: TextSummary) {
        *self += MBTextSummary::from(other);
    }
}

impl MBTextSummary {
    pub fn lines_utf16(&self) -> PointUtf16 {
        PointUtf16 {
            row: self.lines.row,
            column: self.last_line_len_utf16,
        }
    }
}
