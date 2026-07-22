use super::*;

impl EventEmitter<BufferEvent> for Buffer {}

pub(super) fn offset_in_sub_ranges(
    sub_ranges: &[Range<Anchor>],
    offset: usize,
    snapshot: &TextBufferSnapshot,
) -> bool {
    let start_anchor = snapshot.anchor_before(offset);
    let end_anchor = snapshot.anchor_after(offset);

    sub_ranges.iter().any(|sub_range| {
        let is_before_start = sub_range.end.cmp(&start_anchor, snapshot).is_lt();
        let is_after_end = sub_range.start.cmp(&end_anchor, snapshot).is_gt();
        !is_before_start && !is_after_end
    })
}

impl Deref for Buffer {
    type Target = TextBuffer;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}
