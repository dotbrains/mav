use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    BufferRangesUpdated {
        buffer: Entity<Buffer>,
        path_key: PathKey,
        ranges: Vec<ExcerptRange<text::Anchor>>,
    },
    BuffersRemoved {
        removed_buffer_ids: Vec<BufferId>,
    },
    BuffersEdited {
        buffer_ids: Vec<BufferId>,
    },
    DiffHunksToggled,
    Edited {
        edited_buffer: Option<Entity<Buffer>>,
        source: BufferEditSource,
    },
    TransactionUndone {
        transaction_id: TransactionId,
    },
    Reloaded,
    LanguageChanged(BufferId, bool),
    Reparsed(BufferId),
    Saved,
    FileHandleChanged,
    DirtyChanged,
    DiagnosticsUpdated,
    BufferDiffChanged,
}

/// A diff hunk, representing a range of consequent lines in a multibuffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiBufferDiffHunk {
    /// The row range in the multibuffer where this diff hunk appears.
    pub row_range: Range<MultiBufferRow>,
    /// The buffer ID that this hunk belongs to.
    pub buffer_id: BufferId,
    /// The range of the underlying buffer that this hunk corresponds to.
    pub buffer_range: Range<text::Anchor>,
    /// The range within the buffer's diff base that this hunk corresponds to.
    pub diff_base_byte_range: Range<BufferOffset>,
    /// The status of this hunk (added/modified/deleted and secondary status).
    pub status: DiffHunkStatus,
    /// The word diffs for this hunk.
    pub word_diffs: Vec<Range<MultiBufferOffset>>,
    pub excerpt_range: ExcerptRange<text::Anchor>,
    pub multi_buffer_range: Range<Anchor>,
}

impl MultiBufferDiffHunk {
    pub fn status(&self) -> DiffHunkStatus {
        self.status
    }

    pub fn is_created_file(&self) -> bool {
        self.diff_base_byte_range == (BufferOffset(0)..BufferOffset(0))
            && self.buffer_range.start.is_min()
            && self.buffer_range.end.is_max()
    }
}
