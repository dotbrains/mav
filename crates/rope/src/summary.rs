use super::*;

impl sum_tree::Item for Chunk {
    type Summary = ChunkSummary;

    fn summary(&self, _cx: ()) -> Self::Summary {
        ChunkSummary {
            text: self.as_slice().text_summary(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChunkSummary {
    pub(crate) text: TextSummary,
}

impl sum_tree::ContextLessSummary for ChunkSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        self.text += &summary.text;
    }
}

/// Summary of a string of text.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TextSummary {
    /// Length in bytes.
    pub len: usize,
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

impl TextSummary {
    pub fn lines_utf16(&self) -> PointUtf16 {
        PointUtf16 {
            row: self.lines.row,
            column: self.last_line_len_utf16,
        }
    }

    pub fn newline() -> Self {
        Self {
            len: 1,
            chars: 1,
            len_utf16: OffsetUtf16(1),
            first_line_chars: 0,
            last_line_chars: 0,
            last_line_len_utf16: 0,
            lines: Point::new(1, 0),
            longest_row: 0,
            longest_row_chars: 0,
        }
    }

    pub fn add_newline(&mut self) {
        self.len += 1;
        self.len_utf16 += OffsetUtf16(self.len_utf16.0 + 1);
        self.last_line_chars = 0;
        self.last_line_len_utf16 = 0;
        self.lines += Point::new(1, 0);
    }
}

impl<'a> From<&'a str> for TextSummary {
    fn from(text: &'a str) -> Self {
        let mut len_utf16 = OffsetUtf16(0);
        let mut lines = Point::new(0, 0);
        let mut first_line_chars = 0;
        let mut last_line_chars = 0;
        let mut last_line_len_utf16 = 0;
        let mut longest_row = 0;
        let mut longest_row_chars = 0;
        let mut chars = 0;
        for c in text.chars() {
            chars += 1;
            len_utf16.0 += c.len_utf16();

            if c == '\n' {
                lines += Point::new(1, 0);
                last_line_len_utf16 = 0;
                last_line_chars = 0;
            } else {
                lines.column += c.len_utf8() as u32;
                last_line_len_utf16 += c.len_utf16() as u32;
                last_line_chars += 1;
            }

            if lines.row == 0 {
                first_line_chars = last_line_chars;
            }

            if last_line_chars > longest_row_chars {
                longest_row = lines.row;
                longest_row_chars = last_line_chars;
            }
        }

        TextSummary {
            len: text.len(),
            chars,
            len_utf16,
            lines,
            first_line_chars,
            last_line_chars,
            last_line_len_utf16,
            longest_row,
            longest_row_chars,
        }
    }
}

impl sum_tree::ContextLessSummary for TextSummary {
    fn zero() -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &Self) {
        *self += summary;
    }
}

impl ops::Add<Self> for TextSummary {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        AddAssign::add_assign(&mut self, &rhs);
        self
    }
}

impl<'a> ops::AddAssign<&'a Self> for TextSummary {
    fn add_assign(&mut self, other: &'a Self) {
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

impl ops::AddAssign<Self> for TextSummary {
    fn add_assign(&mut self, other: Self) {
        *self += &other;
    }
}

pub trait TextDimension:
    'static + Clone + Copy + Default + for<'a> Dimension<'a, ChunkSummary> + std::fmt::Debug
{
    fn from_text_summary(summary: &TextSummary) -> Self;
    fn from_chunk(chunk: ChunkSlice) -> Self;
    fn add_assign(&mut self, other: &Self);
}

impl<D1: TextDimension, D2: TextDimension> TextDimension for Dimensions<D1, D2, ()> {
    fn from_text_summary(summary: &TextSummary) -> Self {
        Dimensions(
            D1::from_text_summary(summary),
            D2::from_text_summary(summary),
            (),
        )
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        Dimensions(D1::from_chunk(chunk), D2::from_chunk(chunk), ())
    }

    fn add_assign(&mut self, other: &Self) {
        self.0.add_assign(&other.0);
        self.1.add_assign(&other.1);
    }
}

impl<'a> sum_tree::Dimension<'a, ChunkSummary> for TextSummary {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ChunkSummary, _: ()) {
        *self += &summary.text;
    }
}

impl TextDimension for TextSummary {
    fn from_text_summary(summary: &TextSummary) -> Self {
        *summary
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        chunk.text_summary()
    }

    fn add_assign(&mut self, other: &Self) {
        *self += other;
    }
}

impl<'a> sum_tree::Dimension<'a, ChunkSummary> for usize {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ChunkSummary, _: ()) {
        *self += summary.text.len;
    }
}

impl TextDimension for usize {
    fn from_text_summary(summary: &TextSummary) -> Self {
        summary.len
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        chunk.len()
    }

    fn add_assign(&mut self, other: &Self) {
        *self += other;
    }
}

impl<'a> sum_tree::Dimension<'a, ChunkSummary> for OffsetUtf16 {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ChunkSummary, _: ()) {
        *self += summary.text.len_utf16;
    }
}

impl TextDimension for OffsetUtf16 {
    fn from_text_summary(summary: &TextSummary) -> Self {
        summary.len_utf16
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        chunk.len_utf16()
    }

    fn add_assign(&mut self, other: &Self) {
        *self += other;
    }
}

impl<'a> sum_tree::Dimension<'a, ChunkSummary> for Point {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ChunkSummary, _: ()) {
        *self += summary.text.lines;
    }
}

impl TextDimension for Point {
    fn from_text_summary(summary: &TextSummary) -> Self {
        summary.lines
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        chunk.lines()
    }

    fn add_assign(&mut self, other: &Self) {
        *self += other;
    }
}

impl<'a> sum_tree::Dimension<'a, ChunkSummary> for PointUtf16 {
    fn zero(_cx: ()) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a ChunkSummary, _: ()) {
        *self += summary.text.lines_utf16();
    }
}

impl TextDimension for PointUtf16 {
    fn from_text_summary(summary: &TextSummary) -> Self {
        summary.lines_utf16()
    }

    fn from_chunk(chunk: ChunkSlice) -> Self {
        PointUtf16 {
            row: chunk.lines().row,
            column: chunk.last_line_len_utf16(),
        }
    }

    fn add_assign(&mut self, other: &Self) {
        *self += other;
    }
}
