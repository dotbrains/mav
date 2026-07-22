use super::*;

pub type RenderDiffHunkControlsFn = Arc<
    dyn Fn(
        u32,
        &DiffHunkStatus,
        Range<Anchor>,
        bool,
        Pixels,
        &Entity<Editor>,
        &mut Window,
        &mut App,
    ) -> AnyElement,
>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DisplayDiffHunk {
    Folded {
        display_row: DisplayRow,
    },
    Unfolded {
        is_created_file: bool,
        diff_base_byte_range: Range<usize>,
        display_row_range: Range<DisplayRow>,
        multi_buffer_range: Range<Anchor>,
        status: DiffHunkStatus,
        word_diffs: Vec<Range<MultiBufferOffset>>,
    },
}

#[derive(Clone)]
pub(super) struct InlineBlamePopoverState {
    pub(super) scroll_handle: ScrollHandle,
    pub(super) commit_message: Option<ParsedCommitMessage>,
    pub(super) markdown: Entity<Markdown>,
}

pub(super) struct InlineBlamePopover {
    pub(super) position: gpui::Point<Pixels>,
    pub(super) hide_task: Option<Task<()>>,
    pub(super) popover_bounds: Option<Bounds<Pixels>>,
    pub(super) popover_state: InlineBlamePopoverState,
    pub(super) keyboard_grace: bool,
}

/// Represents a diff review button indicator that shows up when hovering over lines in the gutter
/// in diff view mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PhantomDiffReviewIndicator {
    /// The starting anchor of the selection (or the only row if not dragging).
    pub(super) start: Anchor,
    /// The ending anchor of the selection. Equal to start_anchor for single-line selection.
    pub(super) end: Anchor,
    /// There's a small debounce between hovering over the line and showing the indicator.
    /// We don't want to show the indicator when moving the mouse from editor to e.g. project panel.
    pub(super) is_active: bool,
}

#[derive(Clone, Debug)]
pub(super) struct DiffReviewDragState {
    start_anchor: Anchor,
    current_anchor: Anchor,
}

/// Identifies a specific hunk in the diff buffer.
/// Used as a key to group comments by their location.
#[derive(Clone, Debug)]
pub(super) struct DiffHunkKey {
    /// The file path (relative to worktree) this hunk belongs to.
    pub(super) file_path: Arc<util::rel_path::RelPath>,
    /// An anchor at the start of the hunk. This tracks position as the buffer changes.
    pub(super) hunk_start_anchor: Anchor,
}

/// A review comment stored locally before being sent to the Agent panel.
#[derive(Clone)]
pub(super) struct StoredReviewComment {
    /// Unique identifier for this comment (for edit/delete operations).
    pub(super) id: usize,
    /// The comment text entered by the user.
    pub(super) comment: String,
    /// Anchors for the code range being reviewed.
    pub(super) range: Range<Anchor>,
    /// Whether this comment is currently being edited inline.
    pub(super) is_editing: bool,
}

/// Represents an active diff review overlay that appears when clicking the "Add Review" button.
pub(super) struct DiffReviewOverlay {
    pub(super) anchor_range: Range<Anchor>,
    /// The block ID for the overlay.
    pub(super) block_id: CustomBlockId,
    /// The editor entity for the review input.
    pub(super) prompt_editor: Entity<Editor>,
    /// The hunk key this overlay belongs to.
    pub(super) hunk_key: DiffHunkKey,
    /// Whether the comments section is expanded.
    pub(super) comments_expanded: bool,
    /// Editors for comments currently being edited inline.
    /// Key: comment ID, Value: Editor entity for inline editing.
    pub(super) inline_edit_editors: HashMap<usize, Entity<Editor>>,
    /// Subscriptions for inline edit editors' action handlers.
    /// Key: comment ID, Value: Subscription keeping the Newline action handler alive.
    pub(super) inline_edit_subscriptions: HashMap<usize, Subscription>,
    /// The current user's avatar URI for display in comment rows.
    pub(super) user_avatar_uri: Option<SharedUri>,
    /// Subscription to keep the action handler alive.
    _subscription: Subscription,
}

impl DiffReviewDragState {
    pub(super) fn row_range(
        &self,
        snapshot: &DisplaySnapshot,
    ) -> std::ops::RangeInclusive<DisplayRow> {
        let start = self.start_anchor.to_display_point(snapshot).row();
        let current = self.current_anchor.to_display_point(snapshot).row();

        (start..=current).sorted()
    }
}

impl StoredReviewComment {
    fn new(id: usize, comment: String, anchor_range: Range<Anchor>) -> Self {
        Self {
            id,
            comment,
            range: anchor_range,
            is_editing: false,
        }
    }
}
