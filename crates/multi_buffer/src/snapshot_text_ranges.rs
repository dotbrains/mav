use super::*;

impl MultiBufferSnapshot {
    pub fn grapheme_count_for_range(&self, range: &Range<MultiBufferOffset>) -> usize {
        self.text_for_range(range.clone())
            .collect::<String>()
            .graphemes(true)
            .count()
    }

    pub fn range_for_buffer(&self, buffer_id: BufferId) -> Option<Range<Point>> {
        let path_key = self.path_key_index_for_buffer(buffer_id)?;
        let start = Anchor::in_buffer(path_key, text::Anchor::min_for_buffer(buffer_id));
        let end = Anchor::in_buffer(path_key, text::Anchor::max_for_buffer(buffer_id));
        Some((start..end).to_point(self))
    }
}
