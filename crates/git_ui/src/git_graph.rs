use crate::{
    commit_tooltip::{CommitAvatar, CommitDetails, CommitTooltip},
    commit_view::CommitView,
    git_status_icon,
};
use collections::{BTreeMap, HashMap, IndexSet};
use editor::Editor;
use file_icons::FileIcons;
use git::{
    BuildCommitPermalinkParams, GitHostingProviderRegistry, GitRemote, Oid, ParsedGitRemote,
    commit::ParsedCommitMessage,
    parse_git_remote_url,
    repository::{
        CommitDiff, CommitFile, InitialGraphCommitData, LogOrder, LogSource, RepoPath,
        SearchCommitArgs,
    },
    status::{FileStatus, StatusCode, TrackedStatus},
};
use gpui::{
    Action, Anchor, AnyElement, App, Bounds, ClickEvent, ClipboardItem, DefiniteLength,
    DismissEvent, DragMoveEvent, ElementId, Empty, Entity, EventEmitter, FocusHandle, Focusable,
    MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, ScrollStrategy, ScrollWheelEvent,
    SharedString, Subscription, Task, TextStyleRefinement, UniformListScrollHandle, WeakEntity,
    Window, actions, anchored, deferred, point, prelude::*, px, uniform_list,
};
use language::line_diff;
use menu::{Cancel, SelectFirst, SelectLast, SelectNext, SelectPrevious};
use picker::{Picker, PickerDelegate};
use project::{
    GIT_COMMAND_TASK_TAG, ProjectPath, TaskSourceKind,
    git_store::{
        CommitDataState, GitGraphEvent, GitStore, GitStoreEvent, GraphDataResponse, Repository,
        RepositoryEvent, RepositoryId,
    },
};
use search::{
    SearchOption, SearchOptions, SearchSource, SelectNextMatch, SelectPreviousMatch,
    ToggleCaseSensitive, buffer_search,
};
use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};
use task::{ResolvedTask, TaskContext, TaskVariables, VariableName};
use theme::AccentColors;
use time::{OffsetDateTime, UtcOffset};
use ui::{
    Chip, ColumnWidthConfig, CommonAnimationExt as _, ContextMenu, ContextMenuEntry, DiffStat,
    Divider, HeaderResizeInfo, HighlightedLabel, ListItem, ListItemSpacing,
    RedistributableColumnsState, ScrollableHandle, Table, TableInteractionState,
    TableRenderContext, TableResizeBehavior, Tooltip, WithScrollbar, bind_redistributable_columns,
    prelude::*, render_redistributable_columns_resize_handles, render_table_header,
    table_row::TableRow,
};
use workspace::{
    ModalView, Workspace,
    item::{Item, ItemEvent, TabTooltipContent},
};

const RESIZE_HANDLE_WIDTH: f32 = 8.0;
const COMMIT_TAG_LIST_WIDTH_IN_REMS: Rems = rems(10.);
const CUSTOM_GIT_COMMANDS_DOCS_SLUG: &str = "tasks#custom-git-commands";

struct DraggedSplitHandle;

mod entrypoint;
pub use entrypoint::{init, open_or_reuse_graph, resolve_file_history_target_from_project_path};

mod commit_tag_picker;
use commit_tag_picker::*;

mod changed_files;
use changed_files::*;

mod copied_state;
use copied_state::*;

mod graph_data;
use graph_data::*;

mod graph_layout;
use graph_layout::*;

mod state;
use state::*;

mod time_format;
use time_format::*;

#[cfg(any(test, feature = "test-support"))]
mod random_dag;
#[cfg(any(test, feature = "test-support"))]
pub use random_dag::generate_random_commit_dag;
mod actions;
mod commit_detail;
mod context_menu;
mod core;
mod graph_canvas;
mod item_traits;
mod render;
mod render_helpers;
mod search_bar;
mod serialization;
mod table_rows;
#[cfg(any(test, feature = "test-support"))]
mod test_support;

actions!(
    git_graph,
    [
        /// Opens the Git Graph Tab.
        Open,
        /// Copies the SHA of the selected commit to the clipboard.
        CopyCommitSha,
        /// Copies a tag from the selected commit to the clipboard.
        CopyCommitTag,
        /// Opens the commit view for the selected commit.
        OpenCommitView,
        /// Focuses the search field.
        FocusSearch,
        /// Focuses the next git graph tab stop.
        FocusNextTabStop,
        /// Focuses the previous git graph tab stop.
        FocusPreviousTabStop,
        /// Selects a commit half a page above the current selection.
        ScrollUp,
        /// Selects a commit half a page below the current selection.
        ScrollDown,
        /// Toggles the selected commit's changed files between flat and tree views.
        ToggleChangedFilesView,
    ]
);

/// Opens the Git Graph Tab at a specific commit.
#[derive(Clone, PartialEq, serde::Deserialize, schemars::JsonSchema, gpui::Action)]
#[action(namespace = git_graph)]
pub struct OpenAtCommit {
    pub sha: String,
}

fn compute_diff_stats(diff: &CommitDiff) -> (usize, usize) {
    diff.files.iter().fold((0, 0), |(added, removed), file| {
        let old_text = file.old_text.as_deref().unwrap_or("");
        let new_text = file.new_text.as_deref().unwrap_or("");
        let hunks = line_diff(old_text, new_text);
        hunks
            .iter()
            .fold((added, removed), |(a, r), (old_range, new_range)| {
                (
                    a + (new_range.end - new_range.start) as usize,
                    r + (old_range.end - old_range.start) as usize,
                )
            })
    })
}

struct GitGraphContextMenu {
    menu: Entity<ContextMenu>,
    position: Point<Pixels>,
    entry_idx: usize,
    _subscription: Subscription,
}

pub struct GitGraph {
    focus_handle: FocusHandle,
    search_state: SearchState,
    graph_data: GraphData,
    git_store: Entity<GitStore>,
    workspace: WeakEntity<Workspace>,
    context_menu: Option<GitGraphContextMenu>,
    table_interaction_state: Entity<TableInteractionState>,
    column_widths: Entity<RedistributableColumnsState>,
    selected_entry_idx: Option<usize>,
    hovered_entry_idx: Option<usize>,
    graph_canvas_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    log_source: LogSource,
    log_order: LogOrder,
    selected_commit_diff: Option<CommitDiff>,
    selected_commit_diff_stats: Option<(usize, usize)>,
    _commit_diff_task: Option<Task<()>>,
    commit_details_split_state: Entity<SplitState>,
    repo_id: RepositoryId,
    changed_files_scroll_handle: UniformListScrollHandle,
    changed_files_view_mode: ChangedFilesViewMode,
    changed_files_expanded_dirs: HashMap<RepoPath, bool>,
    pending_select_sha: Option<Oid>,
}

mod persistence;

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result, bail};
    use collections::{HashMap, HashSet};
    use fs::FakeFs;
    use git::Oid;
    use git::repository::{CommitData, InitialGraphCommitData};
    use gpui::{TestAppContext, UpdateGlobal};
    use project::git_store::{GitStoreEvent, RepositoryEvent};
    use project::{Project, TaskSourceKind, task_store::TaskSettingsLocation};
    use rand::prelude::*;
    use serde_json::json;
    use settings::{SettingsStore, ThemeSettingsContent};
    use smallvec::{SmallVec, smallvec};
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    mod helpers;
    use helpers::*;

    mod file_history;
    mod graph_layout;
    mod interaction;
    mod loading;
    mod persistence;
    mod refs;
    mod search_and_reload;
    mod tasks;
}
