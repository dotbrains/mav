use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorEvent {
    /// Emitted when the stored review comments change (added, removed, or updated).
    ReviewCommentsChanged {
        /// The new total count of review comments.
        total_count: usize,
    },
    InputIgnored {
        text: Arc<str>,
    },
    InputHandled {
        utf16_range_to_replace: Option<Range<isize>>,
        text: Arc<str>,
    },
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
    BufferFoldToggled {
        ids: Vec<BufferId>,
        folded: bool,
    },
    ExpandExcerptsRequested {
        excerpt_anchors: Vec<Anchor>,
        lines: u32,
        direction: ExpandExcerptDirection,
    },
    StageOrUnstageRequested {
        stage: bool,
        hunks: Vec<MultiBufferDiffHunk>,
    },
    OpenExcerptsRequested {
        selections_by_buffer: HashMap<BufferId, (Vec<Range<BufferOffset>>, Option<u32>)>,
        split: bool,
    },
    RestoreRequested {
        hunks: Vec<MultiBufferDiffHunk>,
    },
    /// Emitted when an underlying buffer changes, including edits made through another editor.
    BufferEdited,
    /// Emitted when this editor creates, undoes, or redoes an edit transaction.
    Edited {
        /// The transaction that changed the editor's buffer.
        transaction_id: clock::Lamport,
    },
    Reparsed(BufferId),
    Focused,
    FocusedIn,
    Blurred,
    DirtyChanged,
    Saved,
    TitleChanged,
    FileHandleChanged,
    SelectionsChanged {
        local: bool,
    },
    ScrollPositionChanged {
        local: bool,
        autoscroll: bool,
    },
    TransactionUndone {
        transaction_id: clock::Lamport,
    },
    TransactionBegun {
        transaction_id: clock::Lamport,
    },
    CursorShapeChanged,
    BreadcrumbsChanged,
    OutlineSymbolsChanged,
    PushedToNavHistory {
        anchor: Anchor,
        is_deactivate: bool,
    },
}

impl EventEmitter<EditorEvent> for Editor {}

pub(crate) enum ReportEditorEvent {
    Saved { auto_saved: bool },
    EditorOpened,
    Closed,
}

impl ReportEditorEvent {
    pub(super) fn event_type(&self) -> &'static str {
        match self {
            Self::Saved { .. } => "Editor Saved",
            Self::EditorOpened => "Editor Opened",
            Self::Closed => "Editor Closed",
        }
    }
}
