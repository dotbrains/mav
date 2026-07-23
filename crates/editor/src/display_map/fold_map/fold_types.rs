use super::*;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default, Ord, PartialOrd, Hash)]
pub struct FoldId(pub(crate) usize);

impl From<FoldId> for ElementId {
    fn from(val: FoldId) -> Self {
        val.0.into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fold {
    pub id: FoldId,
    pub range: FoldRange,
    pub placeholder: FoldPlaceholder,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldRange(pub(crate) Range<Anchor>);

impl Deref for FoldRange {
    type Target = Range<Anchor>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FoldRange {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for FoldRange {
    fn default() -> Self {
        Self(Anchor::Min..Anchor::Max)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FoldMetadata {
    pub(crate) range: FoldRange,
    pub(crate) width: Option<Pixels>,
}

impl sum_tree::Item for Fold {
    type Summary = FoldSummary;

    fn summary(&self, _cx: &MultiBufferSnapshot) -> Self::Summary {
        FoldSummary {
            start: self.range.start,
            end: self.range.end,
            min_start: self.range.start,
            max_end: self.range.end,
            count: 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FoldSummary {
    pub(crate) start: Anchor,
    pub(crate) end: Anchor,
    pub(crate) min_start: Anchor,
    pub(crate) max_end: Anchor,
    pub(crate) count: usize,
}

impl Default for FoldSummary {
    fn default() -> Self {
        Self {
            start: Anchor::Min,
            end: Anchor::Max,
            min_start: Anchor::Max,
            max_end: Anchor::Min,
            count: 0,
        }
    }
}

impl sum_tree::Summary for FoldSummary {
    type Context<'a> = &'a MultiBufferSnapshot;

    fn zero(_cx: &MultiBufferSnapshot) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, other: &Self, buffer: Self::Context<'_>) {
        if other.min_start.cmp(&self.min_start, buffer) == Ordering::Less {
            self.min_start = other.min_start;
        }
        if other.max_end.cmp(&self.max_end, buffer) == Ordering::Greater {
            self.max_end = other.max_end;
        }

        #[cfg(debug_assertions)]
        {
            let start_comparison = self.start.cmp(&other.start, buffer);
            assert!(start_comparison <= Ordering::Equal);
            if start_comparison == Ordering::Equal {
                assert!(self.end.cmp(&other.end, buffer) >= Ordering::Equal);
            }
        }

        self.start = other.start;
        self.end = other.end;
        self.count += other.count;
    }
}

impl<'a> sum_tree::Dimension<'a, FoldSummary> for FoldRange {
    fn zero(_cx: &MultiBufferSnapshot) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a FoldSummary, _: &MultiBufferSnapshot) {
        self.0.start = summary.start;
        self.0.end = summary.end;
    }
}

impl sum_tree::SeekTarget<'_, FoldSummary, FoldRange> for FoldRange {
    fn cmp(&self, other: &Self, buffer: &MultiBufferSnapshot) -> Ordering {
        AnchorRangeExt::cmp(&self.0, &other.0, buffer)
    }
}

impl<'a> sum_tree::Dimension<'a, FoldSummary> for MultiBufferOffset {
    fn zero(_cx: &MultiBufferSnapshot) -> Self {
        Default::default()
    }

    fn add_summary(&mut self, summary: &'a FoldSummary, _: &MultiBufferSnapshot) {
        *self += summary.count;
    }
}
