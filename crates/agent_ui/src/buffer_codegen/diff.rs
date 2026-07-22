use super::*;

#[derive(Default)]
pub struct Diff {
    pub deleted_row_ranges: Vec<(Anchor, RangeInclusive<u32>)>,
    pub inserted_row_ranges: Vec<Range<Anchor>>,
}

impl Diff {
    pub(super) fn is_empty(&self) -> bool {
        self.deleted_row_ranges.is_empty() && self.inserted_row_ranges.is_empty()
    }
}
