use super::*;

impl MultiBufferSnapshot {
    pub fn text(&self) -> String {
        self.chunks(
            MultiBufferOffset::ZERO..self.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
        )
        .map(|chunk| chunk.text)
        .collect()
    }

    pub fn reversed_chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        self.reversed_chunks_in_range(MultiBufferOffset::ZERO..position.to_offset(self))
            .flat_map(|c| c.chars().rev())
    }

    pub(super) fn reversed_chunks_in_range(
        &self,
        range: Range<MultiBufferOffset>,
    ) -> ReversedMultiBufferChunks<'_> {
        let mut cursor = self.cursor::<MultiBufferOffset, BufferOffset>();
        cursor.seek(&range.end);
        let current_chunks = cursor.region().as_ref().map(|region| {
            let start_overshoot = range.start.saturating_sub(region.range.start);
            let end_overshoot = range.end - region.range.start;
            let end = (region.buffer_range.start + end_overshoot).min(region.buffer_range.end);
            let start = region.buffer_range.start + start_overshoot;
            region.buffer.reversed_chunks_in_range(start..end)
        });
        ReversedMultiBufferChunks {
            cursor,
            current_chunks,
            start: range.start,
            offset: range.end,
        }
    }

    pub fn chars_at<T: ToOffset>(&self, position: T) -> impl Iterator<Item = char> + '_ {
        let offset = position.to_offset(self);
        self.text_for_range(offset..self.len())
            .flat_map(|chunk| chunk.chars())
    }

    pub fn text_for_range<T: ToOffset>(&self, range: Range<T>) -> impl Iterator<Item = &str> + '_ {
        self.chunks(
            range,
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
        )
        .map(|chunk| chunk.text)
    }

    pub fn is_line_blank(&self, row: MultiBufferRow) -> bool {
        self.text_for_range(Point::new(row.0, 0)..Point::new(row.0, self.line_len(row)))
            .all(|chunk| chunk.matches(|c: char| !c.is_whitespace()).next().is_none())
    }

    pub fn contains_str_at<T>(&self, position: T, needle: &str) -> bool
    where
        T: ToOffset,
    {
        let position = position.to_offset(self);
        position == self.clip_offset(position, Bias::Left)
            && self
                .bytes_in_range(position..self.len())
                .flatten()
                .copied()
                .take(needle.len())
                .eq(needle.bytes())
    }
}
