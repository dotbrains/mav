use super::*;

pub(super) struct VisibleEntriesForWorktree {
    pub(super) worktree_id: WorktreeId,
    pub(super) entries: Vec<GitEntry>,
    pub(super) index: OnceCell<HashSet<Arc<RelPath>>>,
}

pub(super) struct State {
    pub(super) last_worktree_root_id: Option<ProjectEntryId>,
    /// Maps from leaf project entry ID to the currently selected ancestor.
    /// Relevant only for auto-fold dirs, where a single project panel entry may actually consist of several
    /// project entries (and all non-leaf nodes are guaranteed to be directories).
    pub(super) ancestors: HashMap<ProjectEntryId, FoldedAncestors>,
    pub(super) visible_entries: Vec<VisibleEntriesForWorktree>,
    pub(super) max_width_item_index: Option<usize>,
    pub(super) edit_state: Option<EditState>,
    pub(super) temporarily_unfolded_pending_state: Option<TemporaryUnfoldedPendingState>,
    pub(super) unfolded_dir_ids: HashSet<ProjectEntryId>,
    pub(super) expanded_dir_ids: HashMap<WorktreeId, Vec<ProjectEntryId>>,
}

impl State {
    pub(super) fn is_unfolded(&self, entry_id: &ProjectEntryId) -> bool {
        self.unfolded_dir_ids.contains(entry_id)
            || self.edit_state.as_ref().map_or(false, |edit_state| {
                edit_state.temporarily_unfolded == Some(*entry_id)
            })
    }

    pub(super) fn derive(old: &Self) -> Self {
        Self {
            last_worktree_root_id: None,
            ancestors: Default::default(),
            visible_entries: Default::default(),
            max_width_item_index: None,
            edit_state: old.edit_state.clone(),
            temporarily_unfolded_pending_state: None,
            unfolded_dir_ids: old.unfolded_dir_ids.clone(),
            expanded_dir_ids: old.expanded_dir_ids.clone(),
        }
    }
}

pub struct ProjectPanel {
    pub(super) project: Entity<Project>,
    pub(super) fs: Arc<dyn Fs>,
    pub(super) focus_handle: FocusHandle,
    pub(super) scroll_handle: UniformListScrollHandle,
    // An update loop that keeps incrementing/decrementing scroll offset while there is a dragged entry that's
    // hovered over the start/end of a list.
    pub(super) hover_scroll_task: Option<Task<()>>,
    pub(super) rendered_entries_len: usize,
    pub(super) folded_directory_drag_target: Option<FoldedDirectoryDragTarget>,
    pub(super) drag_target_entry: Option<DragTarget>,
    pub(super) marked_entries: Vec<SelectedEntry>,
    pub(super) selection: Option<SelectedEntry>,
    pub(super) context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    pub(super) filename_editor: Entity<Editor>,
    pub(super) clipboard: Option<ClipboardEntry>,
    pub(super) _dragged_entry_destination: Option<Arc<Path>>,
    pub(super) workspace: WeakEntity<Workspace>,
    pub(super) diagnostics: HashMap<(WorktreeId, Arc<RelPath>), DiagnosticSeverity>,
    pub(super) diagnostic_counts: HashMap<(WorktreeId, Arc<RelPath>), DiagnosticCount>,
    pub(super) diagnostic_summary_update: Task<()>,
    // We keep track of the mouse down state on entries so we don't flash the UI
    // in case a user clicks to open a file.
    pub(super) mouse_down: bool,
    pub(super) hover_expand_task: Option<Task<()>>,
    pub(super) previous_drag_position: Option<Point<Pixels>>,
    pub(super) sticky_items_count: usize,
    pub(super) last_reported_update: Instant,
    pub(super) update_visible_entries_task: UpdateVisibleEntriesTask,
    pub(super) undo_manager: UndoManager,
    pub(super) state: State,
}

pub(super) struct UpdateVisibleEntriesTask {
    pub(super) _visible_entries_task: Task<()>,
    pub(super) focus_filename_editor: bool,
    pub(super) autoscroll: bool,
}

#[derive(Debug)]
pub(super) struct TemporaryUnfoldedPendingState {
    pub(super) previously_focused_leaf_entry: SelectedEntry,
    pub(super) temporarily_unfolded_active_entry_id: ProjectEntryId,
}

impl Default for UpdateVisibleEntriesTask {
    fn default() -> Self {
        UpdateVisibleEntriesTask {
            _visible_entries_task: Task::ready(()),
            focus_filename_editor: Default::default(),
            autoscroll: Default::default(),
        }
    }
}

pub(super) enum DragTarget {
    /// Dragging on an entry
    Entry {
        /// The entry currently under the mouse cursor during a drag operation
        entry_id: ProjectEntryId,
        /// Highlight this entry along with all of its children
        highlight_entry_id: ProjectEntryId,
    },
    /// Dragging on background
    Background,
}

#[derive(Copy, Clone, Debug)]
pub(super) struct FoldedDirectoryDragTarget {
    pub(super) entry_id: ProjectEntryId,
    pub(super) index: usize,
    /// Whether we are dragging over the delimiter rather than the component itself.
    pub(super) is_delimiter_target: bool,
}

#[derive(Clone, Debug)]
pub(super) enum ValidationState {
    None,
    Warning(String),
    Error(String),
}

#[derive(Clone, Debug)]
pub(super) struct EditState {
    pub(super) worktree_id: WorktreeId,
    pub(super) entry_id: ProjectEntryId,
    pub(super) leaf_entry_id: Option<ProjectEntryId>,
    pub(super) is_dir: bool,
    pub(super) depth: usize,
    pub(super) processing_filename: Option<Arc<RelPath>>,
    pub(super) previously_focused: Option<SelectedEntry>,
    pub(super) validation_state: ValidationState,
    pub(super) temporarily_unfolded: Option<ProjectEntryId>,
}

impl EditState {
    pub(super) fn is_new_entry(&self) -> bool {
        self.leaf_entry_id.is_none()
    }
}

#[derive(Clone, Debug)]
pub(super) enum ClipboardEntry {
    Copied(BTreeSet<SelectedEntry>),
    Cut(BTreeSet<SelectedEntry>),
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub(super) struct DiagnosticCount {
    pub(super) error_count: usize,
    pub(super) warning_count: usize,
}

impl DiagnosticCount {
    pub(super) fn capped_error_count(&self) -> String {
        Self::capped_count(self.error_count)
    }

    pub(super) fn capped_warning_count(&self) -> String {
        Self::capped_count(self.warning_count)
    }

    pub(super) fn capped_count(count: usize) -> String {
        if count > 99 {
            "99+".to_string()
        } else {
            count.to_string()
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct EntryDetails {
    pub(super) filename: String,
    pub(super) icon: Option<SharedString>,
    pub(super) path: Arc<RelPath>,
    pub(super) depth: usize,
    pub(super) kind: EntryKind,
    pub(super) is_ignored: bool,
    pub(super) is_expanded: bool,
    pub(super) is_selected: bool,
    pub(super) is_marked: bool,
    pub(super) is_editing: bool,
    pub(super) is_processing: bool,
    pub(super) is_cut: bool,
    pub(super) sticky: Option<StickyDetails>,
    pub(super) filename_text_color: Color,
    pub(super) diagnostic_severity: Option<DiagnosticSeverity>,
    pub(super) diagnostic_count: Option<DiagnosticCount>,
    pub(super) git_status: GitSummary,
    pub(super) is_private: bool,
    pub(super) worktree_id: WorktreeId,
    pub(super) canonical_path: Option<Arc<Path>>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct StickyDetails {
    pub(super) sticky_index: usize,
}

/// Permanently deletes the selected file or directory.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub(super) struct Delete {
    #[serde(default)]
    pub skip_prompt: bool,
}

/// Moves the selected file or directory to the system trash.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub(super) struct Trash {
    #[serde(default)]
    pub skip_prompt: bool,
}

/// Selects the next entry with diagnostics.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectNextDiagnostic {
    #[serde(default)]
    pub severity: GoToDiagnosticSeverityFilter,
}

/// Selects the previous entry with diagnostics.
#[derive(PartialEq, Clone, Default, Debug, Deserialize, JsonSchema, Action)]
#[action(namespace = project_panel)]
#[serde(deny_unknown_fields)]
pub(super) struct SelectPrevDiagnostic {
    #[serde(default)]
    pub severity: GoToDiagnosticSeverityFilter,
}

actions!(
    project_panel,
    [
        /// Expands the selected entry in the project tree.
        ExpandSelectedEntry,
        /// Collapses the selected entry in the project tree.
        CollapseSelectedEntry,
        /// Collapses the selected entry and its children in the project tree.
        CollapseSelectedEntryAndChildren,
        /// Expands the selected entry and its children in the project tree.
        ExpandSelectedEntryAndChildren,
        /// Collapses all entries in the project tree.
        CollapseAllEntries,
        /// Expands all entries in the project tree.
        ExpandAllEntries,
        /// Creates a new directory.
        NewDirectory,
        /// Creates a new file.
        NewFile,
        /// Copies the selected file or directory.
        Copy,
        /// Duplicates the selected file or directory.
        Duplicate,
        /// Reveals the selected item in the system file manager.
        RevealInFileManager,
        /// Removes the selected folder from the project.
        RemoveFromProject,
        /// Cuts the selected file or directory.
        Cut,
        /// Pastes the previously cut or copied item.
        Paste,
        /// Downloads the selected remote file
        DownloadFromRemote,
        /// Renames the selected file or directory.
        Rename,
        /// Opens the selected file in the editor.
        Open,
        /// Opens the selected file in a permanent tab.
        OpenPermanent,
        /// Opens the selected file in a vertical split.
        OpenSplitVertical,
        /// Opens the selected file in a horizontal split.
        OpenSplitHorizontal,
        /// Toggles visibility of git-ignored files.
        ToggleHideGitIgnore,
        /// Toggles visibility of hidden files.
        ToggleHideHidden,
        /// Starts a new search in the selected directory.
        NewSearchInDirectory,
        /// Unfolds the selected directory.
        UnfoldDirectory,
        /// Folds the selected directory.
        FoldDirectory,
        /// Scroll half a page upwards
        ScrollUp,
        /// Scroll half a page downwards
        ScrollDown,
        /// Scroll until the cursor displays at the center
        ScrollCursorCenter,
        /// Scroll until the cursor displays at the top
        ScrollCursorTop,
        /// Scroll until the cursor displays at the bottom
        ScrollCursorBottom,
        /// Selects the parent directory.
        SelectParent,
        /// Selects the next entry with git changes.
        SelectNextGitEntry,
        /// Selects the previous entry with git changes.
        SelectPrevGitEntry,
        /// Selects the next directory.
        SelectNextDirectory,
        /// Selects the previous directory.
        SelectPrevDirectory,
        /// Opens a diff view to compare two marked files.
        CompareMarkedFiles,
        /// Undoes the last file operation.
        Undo,
        /// Redoes the last undone file operation.
        Redo,
        /// Opens a markdown preview for the selected file.
        OpenMarkdownPreview,
    ]
);

#[derive(Clone, Debug, Default)]
pub(super) struct FoldedAncestors {
    pub(super) current_ancestor_depth: usize,
    pub(super) ancestors: Vec<ProjectEntryId>,
}

impl FoldedAncestors {
    pub(super) fn max_ancestor_depth(&self) -> usize {
        self.ancestors.len()
    }

    /// Note: This returns None for last item in ancestors list
    pub(super) fn active_ancestor(&self) -> Option<ProjectEntryId> {
        if self.current_ancestor_depth == 0 {
            return None;
        }
        self.ancestors.get(self.current_ancestor_depth).copied()
    }

    pub(super) fn active_index(&self) -> usize {
        self.max_ancestor_depth()
            .saturating_sub(1)
            .saturating_sub(self.current_ancestor_depth)
    }

    pub(super) fn set_active_index(&mut self, index: usize) -> bool {
        let new_depth = self
            .max_ancestor_depth()
            .saturating_sub(1)
            .saturating_sub(index);
        if self.current_ancestor_depth != new_depth {
            self.current_ancestor_depth = new_depth;
            true
        } else {
            false
        }
    }

    pub(super) fn active_component(&self, file_name: &str) -> Option<String> {
        Path::new(file_name)
            .components()
            .nth(self.active_index())
            .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
    }
}
