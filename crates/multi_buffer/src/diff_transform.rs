use super::*;

#[derive(Debug, Clone)]
pub(super) enum DiffTransform {
    BufferContent {
        summary: MBTextSummary,
        inserted_hunk_info: Option<DiffTransformHunkInfo>,
    },
    DeletedHunk {
        summary: TextSummary,
        buffer_id: BufferId,
        hunk_info: DiffTransformHunkInfo,
        base_text_byte_range: Range<usize>,
        has_trailing_newline: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub(super) struct DiffTransformHunkInfo {
    pub(super) buffer_id: BufferId,
    pub(super) hunk_start_anchor: text::Anchor,
    pub(super) hunk_secondary_status: DiffHunkSecondaryStatus,
    pub(super) is_logically_deleted: bool,
    pub(super) excerpt_end: ExcerptAnchor,
}

impl Eq for DiffTransformHunkInfo {}

impl PartialEq for DiffTransformHunkInfo {
    fn eq(&self, other: &DiffTransformHunkInfo) -> bool {
        self.buffer_id == other.buffer_id && self.hunk_start_anchor == other.hunk_start_anchor
    }
}

impl std::hash::Hash for DiffTransformHunkInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.buffer_id.hash(state);
        self.hunk_start_anchor.hash(state);
    }
}

impl DiffTransform {
    pub(super) fn hunk_info(&self) -> Option<DiffTransformHunkInfo> {
        match self {
            DiffTransform::DeletedHunk { hunk_info, .. } => Some(*hunk_info),
            DiffTransform::BufferContent {
                inserted_hunk_info, ..
            } => *inserted_hunk_info,
        }
    }
}

impl sum_tree::Item for DiffTransform {
    type Summary = DiffTransformSummary;

    fn summary(&self, _: <Self::Summary as sum_tree::Summary>::Context<'_>) -> Self::Summary {
        match self {
            DiffTransform::BufferContent { summary, .. } => DiffTransformSummary {
                input: *summary,
                output: *summary,
            },
            &DiffTransform::DeletedHunk { summary, .. } => DiffTransformSummary {
                input: MBTextSummary::default(),
                output: summary.into(),
            },
        }
    }
}
