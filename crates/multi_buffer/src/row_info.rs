use super::*;

#[derive(Clone)]
pub struct ExcerptBoundaryInfo {
    pub start_anchor: Anchor,
    pub range: ExcerptRange<text::Anchor>,
    pub end_row: MultiBufferRow,
}

impl ExcerptBoundaryInfo {
    pub fn start_text_anchor(&self) -> text::Anchor {
        self.range.context.start
    }
    pub fn buffer_id(&self) -> BufferId {
        self.start_text_anchor().buffer_id
    }
    pub fn buffer<'a>(&self, snapshot: &'a MultiBufferSnapshot) -> &'a BufferSnapshot {
        snapshot
            .buffer_for_id(self.buffer_id())
            .expect("buffer snapshot not found for excerpt boundary")
    }
}

impl std::fmt::Debug for ExcerptBoundaryInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field("buffer_id", &self.buffer_id())
            .field("range", &self.range)
            .finish()
    }
}

impl PartialEq for ExcerptBoundaryInfo {
    fn eq(&self, other: &Self) -> bool {
        self.start_anchor == other.start_anchor && self.range == other.range
    }
}

impl Eq for ExcerptBoundaryInfo {}

/// A boundary between `Excerpt`s in a [`MultiBuffer`]
#[derive(Debug)]
pub struct ExcerptBoundary {
    pub prev: Option<ExcerptBoundaryInfo>,
    pub next: ExcerptBoundaryInfo,
    /// The row in the `MultiBuffer` where the boundary is located
    pub row: MultiBufferRow,
}

impl ExcerptBoundary {
    pub fn starts_new_buffer(&self) -> bool {
        match (self.prev.as_ref(), &self.next) {
            (None, _) => true,
            (Some(prev), next) => prev.buffer_id() != next.buffer_id(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ExpandInfo {
    pub direction: ExpandExcerptDirection,
    pub start_anchor: Anchor,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct RowInfo {
    pub buffer_id: Option<BufferId>,
    pub buffer_row: Option<u32>,
    pub multibuffer_row: Option<MultiBufferRow>,
    pub diff_status: Option<buffer_diff::DiffHunkStatus>,
    pub expand_info: Option<ExpandInfo>,
    pub wrapped_buffer_row: Option<u32>,
}
