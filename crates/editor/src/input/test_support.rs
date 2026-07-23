use super::*;

#[cfg(any(test, feature = "test-support"))]
impl Editor {
    pub fn set_linked_edit_ranges_for_testing(
        &mut self,
        ranges: Vec<(Range<Point>, Vec<Range<Point>>)>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let Some((buffer, _)) = self
            .buffer
            .read(cx)
            .text_anchor_for_position(self.selections.newest_anchor().start, cx)
        else {
            return None;
        };
        let buffer = buffer.read(cx);
        let buffer_id = buffer.remote_id();
        let mut linked_ranges = Vec::with_capacity(ranges.len());
        for (base_range, linked_ranges_points) in ranges {
            let base_anchor =
                buffer.anchor_before(base_range.start)..buffer.anchor_after(base_range.end);
            let linked_anchors = linked_ranges_points
                .into_iter()
                .map(|range| buffer.anchor_before(range.start)..buffer.anchor_after(range.end))
                .collect();
            linked_ranges.push((base_anchor, linked_anchors));
        }
        let mut map = HashMap::default();
        map.insert(buffer_id, linked_ranges);
        self.linked_edit_ranges = linked_editing_ranges::LinkedEditingRanges(map);
        Some(())
    }

    #[cfg(test)]
    pub(crate) fn set_auto_replace_emoji_shortcode(&mut self, auto_replace: bool) {
        self.auto_replace_emoji_shortcode = auto_replace;
    }
}
