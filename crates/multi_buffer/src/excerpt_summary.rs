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

impl Excerpt {
    pub(super) fn new(
        path_key: PathKey,
        path_key_index: PathKeyIndex,
        buffer_snapshot: &BufferSnapshot,
        range: ExcerptRange<text::Anchor>,
        has_trailing_newline: bool,
    ) -> Self {
        Excerpt {
            path_key,
            path_key_index,
            buffer_id: buffer_snapshot.remote_id(),
            max_buffer_row: range.context.end.to_point(&buffer_snapshot).row,
            text_summary: buffer_snapshot.text_summary_for_range::<TextSummary, _>(
                range.context.to_offset(&buffer_snapshot),
            ),
            range,
            has_trailing_newline,
        }
    }

    pub(super) fn buffer_snapshot<'a>(
        &self,
        snapshot: &'a MultiBufferSnapshot,
    ) -> &'a BufferSnapshot {
        &snapshot
            .buffers
            .get(&self.buffer_id)
            .expect("buffer snapshot not found for excerpt")
            .buffer_snapshot
    }

    pub(super) fn buffer(&self, multibuffer: &MultiBuffer) -> Entity<Buffer> {
        multibuffer
            .buffer(self.buffer_id)
            .expect("buffer entity not found for excerpt")
    }

    pub(super) fn chunks_in_range<'a>(
        &'a self,
        range: Range<usize>,
        language_aware: LanguageAwareStyling,
        snapshot: &'a MultiBufferSnapshot,
    ) -> ExcerptChunks<'a> {
        let buffer = self.buffer_snapshot(snapshot);
        let content_start = self.range.context.start.to_offset(buffer);
        let chunks_start = content_start + range.start;
        let chunks_end = content_start + cmp::min(range.end, self.text_summary.len);

        let has_footer = self.has_trailing_newline
            && range.start <= self.text_summary.len
            && range.end > self.text_summary.len;

        let content_chunks = buffer.chunks(chunks_start..chunks_end, language_aware);

        ExcerptChunks {
            content_chunks,
            has_footer,
            end: self.end_anchor(),
        }
    }

    pub(super) fn seek_chunks(
        &self,
        excerpt_chunks: &mut ExcerptChunks,
        range: Range<usize>,
        snapshot: &MultiBufferSnapshot,
    ) {
        let buffer = self.buffer_snapshot(snapshot);
        let content_start = self.range.context.start.to_offset(buffer);
        let chunks_start = content_start + range.start;
        let chunks_end = content_start + cmp::min(range.end, self.text_summary.len);
        excerpt_chunks.content_chunks.seek(chunks_start..chunks_end);
        excerpt_chunks.has_footer = self.has_trailing_newline
            && range.start <= self.text_summary.len
            && range.end > self.text_summary.len;
    }

    pub(super) fn clip_anchor(
        &self,
        text_anchor: text::Anchor,
        snapshot: &MultiBufferSnapshot,
    ) -> text::Anchor {
        let buffer = self.buffer_snapshot(snapshot);
        if text_anchor.cmp(&self.range.context.start, buffer).is_lt() {
            self.range.context.start
        } else if text_anchor.cmp(&self.range.context.end, buffer).is_gt() {
            self.range.context.end
        } else {
            text_anchor
        }
    }

    pub(crate) fn contains(&self, anchor: &ExcerptAnchor, snapshot: &MultiBufferSnapshot) -> bool {
        self.path_key_index == anchor.path
            && self.buffer_id == anchor.text_anchor.buffer_id
            && self
                .range
                .contains(&anchor.text_anchor(), self.buffer_snapshot(snapshot))
    }

    pub(super) fn start_anchor(&self) -> ExcerptAnchor {
        ExcerptAnchor::in_buffer(self.path_key_index, self.range.context.start)
    }

    pub(super) fn end_anchor(&self) -> ExcerptAnchor {
        ExcerptAnchor::in_buffer(self.path_key_index, self.range.context.end)
    }
}

impl PartialEq for Excerpt {
    fn eq(&self, other: &Self) -> bool {
        self.path_key_index == other.path_key_index
            && self.buffer_id == other.buffer_id
            && self.range.context == other.range.context
    }
}

impl sum_tree::Item for Excerpt {
    type Summary = ExcerptSummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        let mut text = self.text_summary;
        if self.has_trailing_newline {
            text += TextSummary::from("\n");
        }
        ExcerptSummary {
            path_key: self.path_key.clone(),
            path_key_index: Some(self.path_key_index),
            max_anchor: Some(self.range.context.end),
            widest_line_number: self.max_buffer_row,
            text: text.into(),
            count: 1,
        }
    }
}

pub(super) struct ExcerptChunks<'a> {
    pub(super) content_chunks: BufferChunks<'a>,
    pub(super) end: ExcerptAnchor,
    pub(super) has_footer: bool,
}

impl<'a> Iterator for ExcerptChunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(chunk) = self.content_chunks.next() {
            return Some(chunk);
        }

        if self.has_footer {
            let text = "\n";
            let chars = 0b1;
            let newlines = 0b1;
            self.has_footer = false;
            return Some(Chunk {
                text,
                chars,
                newlines,
                ..Default::default()
            });
        }

        None
    }
}
