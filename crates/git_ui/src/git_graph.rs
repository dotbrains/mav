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
    Hsla, MouseButton, MouseDownEvent, PathBuilder, Pixels, Point, ScrollStrategy,
    ScrollWheelEvent, SharedString, Subscription, Task, TextStyleRefinement,
    UniformListScrollHandle, WeakEntity, Window, actions, anchored, deferred, point, prelude::*,
    px, uniform_list,
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
use smallvec::{SmallVec, smallvec};
use std::{
    cell::Cell,
    ops::Range,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use task::{ResolvedTask, TaskContext, TaskVariables, VariableName};
use theme::AccentColors;
use time::{OffsetDateTime, UtcOffset, format_description::BorrowedFormatItem};
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

const COMMIT_CIRCLE_RADIUS: Pixels = px(3.5);
const COMMIT_CIRCLE_STROKE_WIDTH: Pixels = px(1.5);
const LANE_WIDTH: Pixels = px(16.0);
const LEFT_PADDING: Pixels = px(12.0);
const LINE_WIDTH: Pixels = px(1.5);
const RESIZE_HANDLE_WIDTH: f32 = 8.0;
const COPIED_STATE_DURATION: Duration = Duration::from_secs(2);
const COMMIT_TAG_LIST_WIDTH_IN_REMS: Rems = rems(10.);
const CUSTOM_GIT_COMMANDS_DOCS_SLUG: &str = "tasks#custom-git-commands";
// Extra vertical breathing room added to the UI line height when computing
// the git graph's row height, so commit dots and lines have space around them.
const ROW_VERTICAL_PADDING: Pixels = px(4.0);

struct CopiedState {
    copied_at: Option<Instant>,
}

impl CopiedState {
    fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { copied_at: None }
    }

    fn is_copied(&self) -> bool {
        self.copied_at
            .map(|t| t.elapsed() < COPIED_STATE_DURATION)
            .unwrap_or(false)
    }

    fn mark_copied(&mut self) {
        self.copied_at = Some(Instant::now());
    }
}

struct DraggedSplitHandle;

struct CommitTagPicker {
    picker: Entity<Picker<CommitTagPickerDelegate>>,
}

impl CommitTagPicker {
    fn new(tag_names: Vec<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = CommitTagPickerDelegate {
            picker: cx.entity().downgrade(),
            tag_names,
            selected_index: 0,
        };
        let picker = cx.new(|cx| {
            Picker::nonsearchable_uniform_list(delegate, window, cx)
                .initial_width(COMMIT_TAG_LIST_WIDTH_IN_REMS)
        });
        Self { picker }
    }
}

impl EventEmitter<DismissEvent> for CommitTagPicker {}
impl ModalView for CommitTagPicker {}

impl Focusable for CommitTagPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl Render for CommitTagPicker {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().child(self.picker.clone())
    }
}

struct CommitTagPickerDelegate {
    picker: WeakEntity<CommitTagPicker>,
    tag_names: Vec<SharedString>,
    selected_index: usize,
}

impl PickerDelegate for CommitTagPickerDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "commit-tag"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Copy Tag".into()
    }

    fn match_count(&self) -> usize {
        self.tag_names.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        _query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        if let Some(tag_name) = self.tag_names.get(self.selected_index) {
            cx.write_to_clipboard(ClipboardItem::new_string(tag_name.to_string()));
        }
        self.dismissed(window, cx);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.picker
            .update(cx, |_this, cx| cx.emit(DismissEvent))
            .ok();
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        Some(
            ListItem::new(ix)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(Label::new(self.tag_names.get(ix)?.clone())),
        )
    }
}

mod changed_files;
use changed_files::*;

mod graph_data;
use graph_data::*;

enum QueryState {
    Pending(SharedString),
    Confirmed((SharedString, Task<()>)),
    Empty,
}

impl QueryState {
    fn next_state(&mut self) {
        match self {
            Self::Confirmed((query, _)) => *self = Self::Pending(std::mem::take(query)),
            _ => {}
        };
    }
}

struct SearchState {
    case_sensitive: bool,
    editor: Entity<Editor>,
    state: QueryState,
    matches: IndexSet<Oid>,
    selected_index: Option<usize>,
}

struct SplitState {
    left_ratio: f32,
    visible_left_ratio: f32,
}

impl SplitState {
    fn new() -> Self {
        Self {
            left_ratio: 1.0,
            visible_left_ratio: 1.0,
        }
    }

    fn right_ratio(&self) -> f32 {
        1.0 - self.visible_left_ratio
    }

    fn on_drag_move(
        &mut self,
        drag_event: &DragMoveEvent<DraggedSplitHandle>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let drag_position = drag_event.event.position;
        let bounds = drag_event.bounds;
        let bounds_width = bounds.right() - bounds.left();

        let min_ratio = 0.1;
        let max_ratio = 0.9;

        let new_ratio = (drag_position.x - bounds.left()) / bounds_width;
        self.visible_left_ratio = new_ratio.clamp(min_ratio, max_ratio);
    }

    fn commit_ratio(&mut self) {
        self.left_ratio = self.visible_left_ratio;
    }

    fn on_double_click(&mut self) {
        self.left_ratio = 1.0;
        self.visible_left_ratio = 1.0;
    }
}

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

fn timestamp_format() -> &'static [BorrowedFormatItem<'static>] {
    static FORMAT: OnceLock<Vec<BorrowedFormatItem<'static>>> = OnceLock::new();
    FORMAT.get_or_init(|| {
        time::format_description::parse("[day] [month repr:short] [year] [hour]:[minute]")
            .unwrap_or_default()
    })
}

fn format_timestamp(timestamp: i64) -> String {
    let Ok(datetime) = OffsetDateTime::from_unix_timestamp(timestamp) else {
        return "Unknown".to_string();
    };

    let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let local_datetime = datetime.to_offset(local_offset);

    local_datetime
        .format(timestamp_format())
        .unwrap_or_default()
}

pub fn init(cx: &mut App) {
    workspace::register_serializable_item::<GitGraph>(cx);

    cx.observe_new(|workspace: &mut workspace::Workspace, _, _| {
        workspace.register_action_renderer(|div, workspace, window, cx| {
            div.when_some(
                resolve_file_history_target(workspace, window, cx),
                |div, (repo_id, log_source)| {
                    let git_store = workspace.project().read(cx).git_store().clone();
                    let workspace = workspace.weak_handle();

                    div.on_action(move |_: &git::FileHistory, window, cx| {
                        let git_store = git_store.clone();
                        workspace
                            .update(cx, |workspace, cx| {
                                open_or_reuse_graph(
                                    workspace,
                                    repo_id,
                                    git_store,
                                    log_source.clone(),
                                    None,
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                    })
                },
            )
            .when(
                workspace.project().read(cx).active_repository(cx).is_some(),
                |div| {
                    let workspace = workspace.weak_handle();

                    div.on_action({
                        let workspace = workspace.clone();
                        move |_: &Open, window, cx| {
                            workspace
                                .update(cx, |workspace, cx| {
                                    let Some(repo) =
                                        workspace.project().read(cx).active_repository(cx)
                                    else {
                                        return;
                                    };
                                    let selected_repo_id = repo.read(cx).id;

                                    let git_store =
                                        workspace.project().read(cx).git_store().clone();
                                    open_or_reuse_graph(
                                        workspace,
                                        selected_repo_id,
                                        git_store,
                                        LogSource::All,
                                        None,
                                        window,
                                        cx,
                                    );
                                })
                                .ok();
                        }
                    })
                    .on_action(move |action: &OpenAtCommit, window, cx| {
                        let sha = action.sha.clone();
                        workspace
                            .update(cx, |workspace, cx| {
                                let Some(repo) = workspace.project().read(cx).active_repository(cx)
                                else {
                                    return;
                                };
                                let selected_repo_id = repo.read(cx).id;

                                let git_store = workspace.project().read(cx).git_store().clone();
                                open_or_reuse_graph(
                                    workspace,
                                    selected_repo_id,
                                    git_store,
                                    LogSource::All,
                                    Some(sha),
                                    window,
                                    cx,
                                );
                            })
                            .ok();
                    })
                },
            )
        });
    })
    .detach();
}

/// Resolves a `git::FileHistory` target from a known project path (used by
/// callers like `project_panel` that own a focused selection but cannot be
/// referenced from this module due to dependency direction).
pub fn resolve_file_history_target_from_project_path(
    workspace: &Workspace,
    project_path: &ProjectPath,
    cx: &App,
) -> Option<(RepositoryId, LogSource)> {
    let git_store = workspace.project().read(cx).git_store();
    let (repo, repo_path) = git_store
        .read(cx)
        .repository_and_path_for_project_path(project_path, cx)?;
    let log_source = if repo_path.is_empty() {
        LogSource::All
    } else {
        LogSource::Path(repo_path)
    };
    Some((repo.read(cx).id, log_source))
}

fn resolve_file_history_target(
    workspace: &Workspace,
    window: &Window,
    cx: &App,
) -> Option<(RepositoryId, LogSource)> {
    if let Some(panel) = workspace.panel::<crate::git_panel::GitPanel>(cx)
        && panel.read(cx).focus_handle(cx).contains_focused(window, cx)
        && let Some((repository, repo_path)) = panel.read(cx).selected_file_history_target()
    {
        return Some((repository.read(cx).id, LogSource::Path(repo_path)));
    }

    let editor = workspace.active_item_as::<Editor>(cx)?;

    let file = editor
        .read(cx)
        .file_at(editor.read(cx).selections.newest_anchor().head(), cx)?;
    let project_path = ProjectPath {
        worktree_id: file.worktree_id(cx),
        path: file.path().clone(),
    };

    let git_store = workspace.project().read(cx).git_store();
    let (repo, repo_path) = git_store
        .read(cx)
        .repository_and_path_for_project_path(&project_path, cx)?;
    Some((repo.read(cx).id, LogSource::Path(repo_path)))
}

pub fn open_or_reuse_graph(
    workspace: &mut Workspace,
    repo_id: RepositoryId,
    git_store: Entity<GitStore>,
    log_source: LogSource,
    sha: Option<String>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let existing = workspace.items_of_type::<GitGraph>(cx).find(|graph| {
        let graph = graph.read(cx);
        graph.repo_id == repo_id && graph.log_source == log_source
    });

    if let Some(existing) = existing {
        if let Some(sha) = sha {
            existing.update(cx, |graph, cx| {
                graph.select_commit_by_sha(sha.as_str(), cx);
            });
        }
        workspace.activate_item(&existing, true, true, window, cx);
        return;
    }

    let workspace_handle = workspace.weak_handle();
    let git_graph = cx.new(|cx| {
        let mut graph = GitGraph::new(
            repo_id,
            git_store,
            workspace_handle,
            Some(log_source),
            window,
            cx,
        );
        if let Some(sha) = sha {
            graph.select_commit_by_sha(sha.as_str(), cx);
        }
        graph
    });
    workspace.add_item_to_active_pane(Box::new(git_graph), None, true, window, cx);
}

fn lane_center_x(bounds: Bounds<Pixels>, lane: f32) -> Pixels {
    bounds.origin.x + LEFT_PADDING + lane * LANE_WIDTH + LANE_WIDTH / 2.0
}

fn to_row_center(
    to_row: usize,
    row_height: Pixels,
    scroll_offset: Pixels,
    bounds: Bounds<Pixels>,
) -> Pixels {
    bounds.origin.y + to_row as f32 * row_height + row_height / 2.0 - scroll_offset
}

fn draw_commit_circle(center_x: Pixels, center_y: Pixels, color: Hsla, window: &mut Window) {
    let radius = COMMIT_CIRCLE_RADIUS;

    let mut builder = PathBuilder::fill();

    // Start at the rightmost point of the circle
    builder.move_to(point(center_x + radius, center_y));

    // Draw the circle using two arc_to calls (top half, then bottom half)
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center_x - radius, center_y),
    );
    builder.arc_to(
        point(radius, radius),
        px(0.),
        false,
        true,
        point(center_x + radius, center_y),
    );
    builder.close();

    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
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

impl GitGraph {
    fn invalidate_state(&mut self, cx: &mut Context<Self>) {
        self.graph_data.clear();
        self.search_state.matches.clear();
        self.search_state.selected_index = None;
        self.search_state.state.next_state();
        self.context_menu = None;
        cx.emit(ItemEvent::Edit);
        cx.notify();
    }

    /// Computes the height of a single commit row in the git graph.
    ///
    /// The returned value is snapped to the nearest physical pixel. This is
    /// required so that the canvas's float math and the `uniform_list` layout
    /// (which snaps to device pixels) agree on row positions; otherwise rows
    /// drift apart as the user scrolls when `ui_font_size` is fractional.
    fn row_height(window: &Window, _cx: &App) -> Pixels {
        let rem_size = window.rem_size();
        let line_height = window.text_style().line_height_in_pixels(rem_size);
        let raw = line_height + ROW_VERTICAL_PADDING;
        let scale = window.scale_factor();

        (raw * scale).round() / scale
    }

    fn visible_row_count(&self, window: &Window, cx: &App) -> usize {
        let row_height = Self::row_height(window, cx);
        let viewport_height = self
            .table_interaction_state
            .read(cx)
            .scroll_handle
            .0
            .borrow()
            .last_item_size
            .map_or(window.viewport_size().height, |size| size.item.height);

        ((viewport_height / row_height).ceil() as usize).min(self.graph_data.commits.len())
    }

    fn graph_canvas_content_width(&self) -> Pixels {
        (LANE_WIDTH * self.graph_data.max_lanes.max(6) as f32) + LEFT_PADDING * 2.0
    }

    fn preview_column_fractions(&self, window: &Window, cx: &App) -> [f32; 5] {
        // todo(git_graph): We should make a column/table api that allows removing table columns
        let fractions = self
            .column_widths
            .read(cx)
            .preview_fractions(window.rem_size());

        let is_path_history = matches!(self.log_source, LogSource::Path(_));
        let graph_fraction = if is_path_history { 0.0 } else { fractions[0] };
        let offset = if is_path_history { 0 } else { 1 };

        [
            graph_fraction,
            fractions[offset],
            fractions[offset + 1],
            fractions[offset + 2],
            fractions[offset + 3],
        ]
    }

    fn table_column_width_config(&self, window: &Window, cx: &App) -> ColumnWidthConfig {
        let [_, description, date, author, commit] = self.preview_column_fractions(window, cx);
        let table_total = description + date + author + commit;

        let widths = if table_total > 0.0 {
            vec![
                DefiniteLength::Fraction(description / table_total),
                DefiniteLength::Fraction(date / table_total),
                DefiniteLength::Fraction(author / table_total),
                DefiniteLength::Fraction(commit / table_total),
            ]
        } else {
            vec![
                DefiniteLength::Fraction(0.25),
                DefiniteLength::Fraction(0.25),
                DefiniteLength::Fraction(0.25),
                DefiniteLength::Fraction(0.25),
            ]
        };

        ColumnWidthConfig::explicit(widths)
    }

    fn graph_viewport_width(&self, window: &Window, cx: &App) -> Pixels {
        self.column_widths
            .read(cx)
            .preview_column_width(0, window)
            .unwrap_or_else(|| self.graph_canvas_content_width())
    }

    pub fn new(
        repo_id: RepositoryId,
        git_store: Entity<GitStore>,
        workspace: WeakEntity<Workspace>,
        log_source: Option<LogSource>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();

        let accent_colors = cx.theme().accents();
        let graph = GraphData::new(accent_colors_count(accent_colors));
        let log_source = log_source.unwrap_or_default();
        let log_order = LogOrder::default();

        cx.subscribe(&git_store, |this, _, event, cx| match event {
            GitStoreEvent::RepositoryUpdated(updated_repo_id, repo_event, _) => {
                if this.repo_id == *updated_repo_id {
                    if let Some(repository) = this.get_repository(cx) {
                        this.on_repository_event(repository, repo_event, cx);
                    }
                }
            }
            _ => {}
        })
        .detach();

        let search_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Search commits…", window, cx);
            editor
        });

        let table_interaction_state = cx.new(|cx| {
            let mut state = TableInteractionState::new(cx);
            state.focus_handle = state.focus_handle.tab_index(1).tab_stop(true);
            state
        });

        let column_widths = if matches!(log_source, LogSource::Path(_)) {
            cx.new(|_cx| {
                RedistributableColumnsState::new(
                    4,
                    vec![
                        DefiniteLength::Fraction(0.72),
                        DefiniteLength::Fraction(0.12),
                        DefiniteLength::Fraction(0.1),
                        DefiniteLength::Fraction(0.06),
                    ],
                    vec![
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                    ],
                )
            })
        } else {
            cx.new(|_cx| {
                RedistributableColumnsState::new(
                    5,
                    vec![
                        DefiniteLength::Fraction(0.14),
                        DefiniteLength::Fraction(0.6192),
                        DefiniteLength::Fraction(0.1032),
                        DefiniteLength::Fraction(0.086),
                        DefiniteLength::Fraction(0.0516),
                    ],
                    vec![
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                        TableResizeBehavior::Resizable,
                    ],
                )
            })
        };
        let mut row_height = Self::row_height(window, cx);

        cx.observe_global_in::<settings::SettingsStore>(window, move |this, window, cx| {
            let new_row_height = Self::row_height(window, cx);
            if new_row_height != row_height {
                // The `uniform_list` powering the table caches the item size
                // from its last layout; invalidate it so it re-measures with
                // the new row height on the next frame.
                this.table_interaction_state.update(cx, |state, _cx| {
                    state.scroll_handle.0.borrow_mut().last_item_size = None;
                });
                row_height = new_row_height;
                cx.notify();
            }
        })
        .detach();

        let mut this = GitGraph {
            focus_handle,
            git_store,
            search_state: SearchState {
                case_sensitive: false,
                editor: search_editor,
                matches: IndexSet::default(),
                selected_index: None,
                state: QueryState::Empty,
            },
            workspace,
            graph_data: graph,
            _commit_diff_task: None,
            context_menu: None,
            table_interaction_state,
            column_widths,
            selected_entry_idx: None,
            hovered_entry_idx: None,
            graph_canvas_bounds: Rc::new(Cell::new(None)),
            selected_commit_diff: None,
            selected_commit_diff_stats: None,
            log_source,
            log_order,
            commit_details_split_state: cx.new(|_cx| SplitState::new()),
            repo_id,
            changed_files_scroll_handle: UniformListScrollHandle::new(),
            changed_files_view_mode: ChangedFilesViewMode::default(),
            changed_files_expanded_dirs: HashMap::default(),
            pending_select_sha: None,
        };

        this.fetch_initial_graph_data(cx);
        this
    }

    fn on_repository_event(
        &mut self,
        repository: Entity<Repository>,
        event: &RepositoryEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            RepositoryEvent::GraphEvent((source, order), event)
                if source == &self.log_source && order == &self.log_order =>
            {
                match event {
                    GitGraphEvent::FullyLoaded => {
                        if let Some(pending_sha_index) =
                            self.pending_select_sha.take().and_then(|oid| {
                                repository
                                    .read(cx)
                                    .get_graph_data(source.clone(), *order)
                                    .and_then(|data| data.commit_oid_to_index.get(&oid).copied())
                            })
                        {
                            self.select_entry(pending_sha_index, ScrollStrategy::Nearest, cx);
                        }
                        let count = match self.graph_data.max_commit_count {
                            AllCommitCount::FullyLoaded(count) | AllCommitCount::Loading(count) => {
                                count
                            }
                            AllCommitCount::NotLoaded => 0,
                        };
                        self.graph_data.max_commit_count = AllCommitCount::FullyLoaded(count);
                        cx.notify();
                    }
                    GitGraphEvent::LoadingError => {
                        cx.notify();
                    }
                    GitGraphEvent::CountUpdated(commit_count) => {
                        let old_count = self.graph_data.commits.len();

                        if let Some(pending_selection_index) =
                            repository.update(cx, |repository, cx| {
                                let GraphDataResponse {
                                    commits,
                                    is_loading,
                                    error: _,
                                } = repository.graph_data(
                                    source.clone(),
                                    *order,
                                    old_count..*commit_count,
                                    cx,
                                );
                                self.graph_data.add_commits(commits);

                                let pending_sha_index = self.pending_select_sha.and_then(|oid| {
                                    repository.get_graph_data(source.clone(), *order).and_then(
                                        |data| data.commit_oid_to_index.get(&oid).copied(),
                                    )
                                });

                                if !is_loading && pending_sha_index.is_none() {
                                    self.pending_select_sha.take();
                                }

                                pending_sha_index
                            })
                        {
                            self.select_entry(pending_selection_index, ScrollStrategy::Nearest, cx);
                            self.pending_select_sha.take();
                        }

                        cx.notify();
                    }
                }
            }
            RepositoryEvent::HeadChanged | RepositoryEvent::BranchListChanged => {
                // Only invalidate if we scanned atleast once,
                // meaning we are not inside the initial repo loading state
                // NOTE: this fixes an loading performance regression
                if repository.read(cx).scan_id > 1 {
                    self.pending_select_sha = None;
                    self.invalidate_state(cx);
                }
            }
            RepositoryEvent::StashEntriesChanged if self.log_source == LogSource::All => {
                // Stash entries initial's scan id is 2, so we don't want to invalidate the graph before that
                if repository.read(cx).scan_id > 2 {
                    self.pending_select_sha = None;
                    self.invalidate_state(cx);
                }
            }
            RepositoryEvent::GraphEvent(_, _) => {}
            _ => {}
        }
    }

    fn fetch_initial_graph_data(&mut self, cx: &mut App) {
        if let Some(repository) = self.get_repository(cx) {
            repository.update(cx, |repository, cx| {
                let commits = repository
                    .graph_data(self.log_source.clone(), self.log_order, 0..usize::MAX, cx)
                    .commits;
                self.graph_data.add_commits(commits);
            });
        }
    }

    fn get_repository(&self, cx: &App) -> Option<Entity<Repository>> {
        let git_store = self.git_store.read(cx);
        git_store.repositories().get(&self.repo_id).cloned()
    }

    fn has_context_menu(&self) -> bool {
        self.context_menu.is_some()
    }

    /// Checks whether a ref name from git's `%D` decoration
    ///  format refers to the currently checked-out branch.
    fn is_head_ref(ref_name: &str, head_branch_name: &Option<SharedString>) -> bool {
        head_branch_name.as_ref().is_some_and(|head| {
            ref_name == head.as_ref() || ref_name.strip_prefix("HEAD -> ") == Some(head.as_ref())
        })
    }

    /// Extracts a ref name (branch, remote ref, or tag) from a decoration in
    /// git's `%D` format, returning `None` for a detached `HEAD`.
    fn ref_name_from_decoration(decoration: &str) -> Option<SharedString> {
        let name = decoration
            .strip_prefix("tag: ")
            .or_else(|| decoration.strip_prefix("HEAD -> "))
            .unwrap_or(decoration);
        if name.is_empty() || name == "HEAD" {
            return None;
        }
        Some(SharedString::from(name.to_string()))
    }

    fn render_chip(
        &self,
        name: &SharedString,
        accent_color: gpui::Hsla,
        is_head: bool,
    ) -> impl IntoElement {
        Chip::new(name.clone())
            .label_size(LabelSize::Small)
            .truncate()
            .map(|chip| {
                if is_head {
                    chip.icon(IconName::Check)
                        .bg_color(accent_color.opacity(0.25))
                        .border_color(accent_color.opacity(0.5))
                } else {
                    chip.bg_color(accent_color.opacity(0.08))
                        .border_color(accent_color.opacity(0.25))
                }
            })
    }

    /// Renders a ref chip for the commit at `commit_idx`. Chips that name a ref
    /// (branch, remote ref, or tag) get a right-click handler that opens a
    /// ref-specific context menu, so that custom commands can be resolved
    /// against the clicked ref.
    fn render_ref_chip(
        &self,
        name: &SharedString,
        accent_color: gpui::Hsla,
        is_head: bool,
        commit_idx: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = self.render_chip(name, accent_color, is_head);
        let Some(ref_name) = Self::ref_name_from_decoration(name) else {
            return chip.into_any_element();
        };
        div()
            .child(chip)
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.deploy_entry_context_menu(
                        event.position,
                        commit_idx,
                        Some(ref_name.clone()),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_table_rows(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Vec<AnyElement>> {
        let repository = self.get_repository(cx);

        let head_branch_name: Option<SharedString> = repository.as_ref().and_then(|repo| {
            repo.read(cx)
                .snapshot()
                .branch
                .as_ref()
                .map(|branch| SharedString::from(branch.name().to_string()))
        });

        let row_height = Self::row_height(window, cx);
        let has_context_menu = self.has_context_menu();

        // We fetch data outside the visible viewport to avoid loading entries when
        // users scroll through the git graph
        if let Some(repository) = repository.as_ref() {
            const FETCH_RANGE: usize = 100;
            repository.update(cx, |repository, cx| {
                self.graph_data.commits[range.start.saturating_sub(FETCH_RANGE)
                    ..(range.end + FETCH_RANGE)
                        .min(self.graph_data.commits.len().saturating_sub(1))]
                    .iter()
                    .for_each(|commit| {
                        repository.fetch_commit_data(commit.data.sha, false, cx);
                    });
            });
        }

        range
            .map(|idx| {
                let Some((commit, repository)) =
                    self.graph_data.commits.get(idx).zip(repository.as_ref())
                else {
                    return vec![
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                        div().h(row_height).into_any_element(),
                    ];
                };

                let data = repository.update(cx, |repository, cx| {
                    repository
                        .fetch_commit_data(commit.data.sha, false, cx)
                        .clone()
                });

                let short_sha = commit.data.sha.display_short();
                let mut formatted_time = String::new();
                let subject: SharedString;
                let author_name: SharedString;

                if let CommitDataState::Loaded(ref data) = data {
                    subject = data.subject.clone();
                    author_name = data.author_name.clone();
                    formatted_time = format_timestamp(data.commit_timestamp);
                } else {
                    subject = "Loading…".into();
                    author_name = "".into();
                }

                let accent_colors = cx.theme().accents();
                let accent_color = accent_colors
                    .0
                    .get(commit.color_idx)
                    .copied()
                    .unwrap_or_else(|| accent_colors.0.first().copied().unwrap_or_default());

                let is_selected = self.selected_entry_idx == Some(idx);
                let is_matched = self.search_state.matches.contains(&commit.data.sha);
                let column_label = |label: SharedString| {
                    Label::new(label)
                        .when(!is_selected, |c| c.color(Color::Muted))
                        .truncate()
                        .into_any_element()
                };

                let subject_label = if is_matched {
                    let query = match &self.search_state.state {
                        QueryState::Confirmed((query, _)) => Some(query.clone()),
                        _ => None,
                    };
                    let highlight_ranges = query
                        .and_then(|q| {
                            let ranges = if self.search_state.case_sensitive {
                                subject
                                    .match_indices(q.as_str())
                                    .map(|(start, matched)| start..start + matched.len())
                                    .collect::<Vec<_>>()
                            } else {
                                let q = q.to_lowercase();
                                let subject_lower = subject.to_lowercase();

                                subject_lower
                                    .match_indices(&q)
                                    .filter_map(|(start, matched)| {
                                        let end = start + matched.len();
                                        subject.is_char_boundary(start).then_some(()).and_then(
                                            |_| subject.is_char_boundary(end).then_some(start..end),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            };

                            (!ranges.is_empty()).then_some(ranges)
                        })
                        .unwrap_or_default();
                    HighlightedLabel::from_ranges(subject, highlight_ranges)
                        .when(!is_selected, |c| c.color(Color::Muted))
                        .truncate()
                        .into_any_element()
                } else {
                    column_label(subject)
                };

                vec![
                    div()
                        .id(ElementId::NamedInteger("commit-subject".into(), idx as u64))
                        .overflow_hidden()
                        .when(!has_context_menu, |this| {
                            if let CommitDataState::Loaded(commit_data) = &data {
                                let sha = commit.data.sha.to_string();
                                let author_name = commit_data.author_name.clone();
                                let author_email = commit_data.author_email.clone();
                                let message = commit_data.message.clone();
                                let commit_timestamp = commit_data.commit_timestamp;
                                let workspace = self.workspace.clone();
                                let repository = repository.clone();
                                this.hoverable_tooltip(move |_window, cx| {
                                    let remote_url = repository.read(cx).default_remote_url();
                                    let provider_registry =
                                        GitHostingProviderRegistry::default_global(cx);
                                    let commit_details = CommitDetails {
                                        sha: sha.clone().into(),
                                        author_name: author_name.clone(),
                                        author_email: author_email.clone(),
                                        commit_time: OffsetDateTime::from_unix_timestamp(
                                            commit_timestamp,
                                        )
                                        .unwrap_or_else(|_| OffsetDateTime::now_utc()),
                                        message: Some(ParsedCommitMessage::parse(
                                            sha.clone(),
                                            message.to_string(),
                                            remote_url.as_deref(),
                                            Some(provider_registry),
                                        )),
                                    };
                                    cx.new(|cx| {
                                        CommitTooltip::new(
                                            commit_details,
                                            repository.clone(),
                                            workspace.clone(),
                                            cx,
                                        )
                                    })
                                    .into()
                                })
                            } else {
                                this
                            }
                        })
                        .child(
                            h_flex()
                                .gap_2()
                                .overflow_hidden()
                                .children((!commit.data.ref_names.is_empty()).then(|| {
                                    h_flex().gap_1().children(commit.data.ref_names.iter().map(
                                        |name| {
                                            let is_head =
                                                Self::is_head_ref(name.as_ref(), &head_branch_name);
                                            self.render_ref_chip(
                                                name,
                                                accent_color,
                                                is_head,
                                                idx,
                                                cx,
                                            )
                                        },
                                    ))
                                }))
                                .child(subject_label),
                        )
                        .into_any_element(),
                    column_label(formatted_time.into()),
                    column_label(author_name),
                    column_label(short_sha.into()),
                ]
            })
            .collect()
    }

    fn cancel(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_entry_idx = None;
        self.selected_commit_diff = None;
        self.selected_commit_diff_stats = None;
        self.changed_files_expanded_dirs.clear();
        cx.emit(ItemEvent::Edit);
        cx.notify();
    }

    fn select_first(&mut self, _: &SelectFirst, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_entry(0, ScrollStrategy::Nearest, cx);
    }

    fn select_prev(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected_entry_idx) = &self.selected_entry_idx {
            self.select_entry(
                selected_entry_idx.saturating_sub(1),
                ScrollStrategy::Nearest,
                cx,
            );
        } else {
            self.select_first(&SelectFirst, window, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selected_entry_idx) = &self.selected_entry_idx {
            self.select_entry(
                selected_entry_idx
                    .saturating_add(1)
                    .min(self.graph_data.commits.len().saturating_sub(1)),
                ScrollStrategy::Nearest,
                cx,
            );
        } else {
            self.select_prev(&SelectPrevious, window, cx);
        }
    }

    fn select_last(&mut self, _: &SelectLast, _window: &mut Window, cx: &mut Context<Self>) {
        self.select_entry(
            self.graph_data.commits.len().saturating_sub(1),
            ScrollStrategy::Nearest,
            cx,
        );
    }

    fn scroll_up(&mut self, _: &ScrollUp, window: &mut Window, cx: &mut Context<Self>) {
        let step = (self.visible_row_count(window, cx) / 2).max(1);
        let target_idx = self.selected_entry_idx.unwrap_or(0).saturating_sub(step);

        self.select_entry(target_idx, ScrollStrategy::Nearest, cx);
    }

    fn scroll_down(&mut self, _: &ScrollDown, window: &mut Window, cx: &mut Context<Self>) {
        let Some(last_entry_idx) = self.graph_data.commits.len().checked_sub(1) else {
            return;
        };

        let step = (self.visible_row_count(window, cx) / 2).max(1);
        let target_idx = self
            .selected_entry_idx
            .unwrap_or(0)
            .saturating_add(step)
            .min(last_entry_idx);

        self.select_entry(target_idx, ScrollStrategy::Nearest, cx);
    }

    fn confirm(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        self.open_selected_commit_view(window, cx);
    }

    fn toggle_changed_files_view(
        &mut self,
        _: &ToggleChangedFilesView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.changed_files_view_mode = self.changed_files_view_mode.toggled();
        self.changed_files_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    fn search(&mut self, query: SharedString, cx: &mut Context<Self>) {
        let Some(repo) = self.get_repository(cx) else {
            return;
        };

        self.search_state.matches.clear();
        self.search_state.selected_index = None;
        self.search_state.editor.update(cx, |editor, _cx| {
            editor.set_text_style_refinement(Default::default());
        });

        if query.as_str().is_empty() {
            self.search_state.state = QueryState::Empty;
            cx.notify();
            return;
        }

        let (request_tx, request_rx) = async_channel::unbounded::<Oid>();

        repo.update(cx, |repo, cx| {
            repo.search_commits(
                self.log_source.clone(),
                SearchCommitArgs {
                    query: query.clone(),
                    case_sensitive: self.search_state.case_sensitive,
                },
                request_tx,
                cx,
            );
        });

        let search_task = cx.spawn(async move |this, cx| {
            while let Ok(first_oid) = request_rx.recv().await {
                let mut pending_oids = vec![first_oid];
                while let Ok(oid) = request_rx.try_recv() {
                    pending_oids.push(oid);
                }

                this.update(cx, |this, cx| {
                    if this.search_state.selected_index.is_none() {
                        this.search_state.selected_index = Some(0);
                        this.select_commit_by_sha(first_oid, cx);
                    }

                    this.search_state.matches.extend(pending_oids);
                    cx.notify();
                })
                .ok();
            }

            this.update(cx, |this, cx| {
                if this.search_state.matches.is_empty() {
                    this.search_state.editor.update(cx, |editor, cx| {
                        editor.set_text_style_refinement(TextStyleRefinement {
                            color: Some(Color::Error.color(cx)),
                            ..Default::default()
                        });
                    });
                }
            })
            .ok();
        });

        self.search_state.state = QueryState::Confirmed((query, search_task));
        cx.emit(ItemEvent::Edit);
    }

    fn confirm_search(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        let query = self.search_state.editor.read(cx).text(cx).into();
        self.search(query, cx);
    }

    fn activate_search_editor_if_focused(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_state.editor.update(cx, |editor, cx| {
            if editor.is_focused(window) {
                editor.select_all(&Default::default(), window, cx);
                editor.show_cursor(cx);
            }
        });
    }

    fn focus_next_tab_stop(
        &mut self,
        _: &FocusNextTabStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
        self.activate_search_editor_if_focused(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn focus_previous_tab_stop(
        &mut self,
        _: &FocusPreviousTabStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
        self.activate_search_editor_if_focused(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    fn select_entry(
        &mut self,
        idx: usize,
        scroll_strategy: ScrollStrategy,
        cx: &mut Context<Self>,
    ) {
        if self.selected_entry_idx == Some(idx) || idx >= self.graph_data.commits.len() {
            debug_assert!(
                idx < self.graph_data.commits.len(),
                "attempted to select out of bounds index: {idx}, commits.len: {}",
                self.graph_data.commits.len()
            );
            return;
        }

        self.selected_entry_idx = Some(idx);
        self.selected_commit_diff = None;
        self.selected_commit_diff_stats = None;
        self.changed_files_expanded_dirs.clear();
        self.changed_files_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.table_interaction_state.update(cx, |state, cx| {
            state.scroll_handle.scroll_to_item(idx, scroll_strategy);
            cx.notify();
        });

        let Some(commit) = self.graph_data.commits.get(idx) else {
            return;
        };

        let sha = commit.data.sha.to_string();

        let Some(repository) = self.get_repository(cx) else {
            return;
        };

        let diff_receiver = repository.update(cx, |repo, _| repo.load_commit_diff(sha));

        self._commit_diff_task = Some(cx.spawn(async move |this, cx| {
            if let Ok(Ok(diff)) = diff_receiver.await {
                this.update(cx, |this, cx| {
                    let stats = compute_diff_stats(&diff);
                    this.selected_commit_diff = Some(diff);
                    this.selected_commit_diff_stats = Some(stats);
                    cx.notify();
                })
                .ok();
            }
        }));

        cx.emit(ItemEvent::Edit);
        cx.notify();
    }

    fn select_previous_match(&mut self, cx: &mut Context<Self>) {
        if self.search_state.matches.is_empty() {
            return;
        }

        let mut prev_selection = self.search_state.selected_index.unwrap_or_default();

        if prev_selection == 0 {
            prev_selection = self.search_state.matches.len() - 1;
        } else {
            prev_selection -= 1;
        }

        let Some(&oid) = self.search_state.matches.get_index(prev_selection) else {
            return;
        };

        self.search_state.selected_index = Some(prev_selection);
        self.select_commit_by_sha(oid, cx);
    }

    fn select_next_match(&mut self, cx: &mut Context<Self>) {
        if self.search_state.matches.is_empty() {
            return;
        }

        let mut next_selection = self
            .search_state
            .selected_index
            .map(|index| index + 1)
            .unwrap_or_default();

        if next_selection >= self.search_state.matches.len() {
            next_selection = 0;
        }

        let Some(&oid) = self.search_state.matches.get_index(next_selection) else {
            return;
        };

        self.search_state.selected_index = Some(next_selection);
        self.select_commit_by_sha(oid, cx);
    }

    pub fn set_repo_id(&mut self, repo_id: RepositoryId, cx: &mut Context<Self>) {
        if repo_id != self.repo_id
            && self
                .git_store
                .read(cx)
                .repositories()
                .contains_key(&repo_id)
        {
            self.repo_id = repo_id;
            self.invalidate_state(cx);
        }
    }

    pub fn select_commit_by_sha(&mut self, sha: impl TryInto<Oid>, cx: &mut Context<Self>) {
        fn inner(this: &mut GitGraph, oid: Oid, cx: &mut Context<GitGraph>) {
            let Some(selected_repository) = this.get_repository(cx) else {
                return;
            };

            let Some(index) = selected_repository
                .read(cx)
                .get_graph_data(this.log_source.clone(), this.log_order)
                .and_then(|data| data.commit_oid_to_index.get(&oid))
                .copied()
            else {
                this.pending_select_sha = Some(oid);
                return;
            };

            this.pending_select_sha = None;
            this.select_entry(index, ScrollStrategy::Center, cx);
        }

        if let Ok(oid) = sha.try_into() {
            inner(self, oid, cx);
        }
    }

    fn open_selected_commit_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };

        self.open_commit_view(selected_entry_index, window, cx);
    }

    fn open_commit_view(
        &mut self,
        entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit_entry) = self.graph_data.commits.get(entry_index) else {
            return;
        };

        let Some(repository) = self.get_repository(cx) else {
            return;
        };

        CommitView::open(
            commit_entry.data.sha.to_string(),
            repository.downgrade(),
            self.workspace.clone(),
            None,
            None,
            window,
            cx,
        );
    }

    fn copy_commit_sha(&mut self, entry_index: usize, cx: &mut Context<Self>) {
        let Some(commit) = self.graph_data.commits.get(entry_index) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(commit.data.sha.to_string()));
    }

    fn copy_selected_commit_sha(
        &mut self,
        _: &CopyCommitSha,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };
        self.copy_commit_sha(selected_entry_index, cx);
    }

    fn copy_commit_tag(&mut self, entry_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(commit) = self.graph_data.commits.get(entry_index) else {
            return;
        };

        let tag_names = commit
            .data
            .tag_names()
            .into_iter()
            .map(|tag_name| SharedString::from(tag_name.to_string()))
            .collect::<Vec<_>>();

        match tag_names.as_slice() {
            [] => {}
            [tag_name] => cx.write_to_clipboard(ClipboardItem::new_string(tag_name.to_string())),
            _ => {
                self.workspace
                    .update(cx, |workspace, cx| {
                        workspace.toggle_modal(window, cx, |window, cx| {
                            CommitTagPicker::new(tag_names, window, cx)
                        });
                    })
                    .ok();
            }
        }
    }

    fn copy_selected_commit_tag(
        &mut self,
        _: &CopyCommitTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };
        self.copy_commit_tag(selected_entry_index, window, cx);
    }

    fn git_task_context(
        &self,
        commit_sha: Oid,
        ref_name: Option<&str>,
        cx: &App,
    ) -> Option<TaskContext> {
        let repository_path = self
            .get_repository(cx)?
            .read(cx)
            .work_directory_abs_path
            .to_path_buf();

        let repository_name = repository_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToString::to_string);

        let mut task_variables = TaskVariables::from_iter([
            (VariableName::GitSha, commit_sha.to_string()),
            (VariableName::GitShaShort, commit_sha.display_short()),
            (
                VariableName::GitRepositoryPath,
                repository_path.to_string_lossy().into_owned(),
            ),
        ]);

        if let Some(repository_name) = repository_name {
            task_variables.insert(VariableName::GitRepositoryName, repository_name);
        }

        if let Some(ref_name) = ref_name {
            task_variables.insert(VariableName::GitRef, ref_name.to_string());
        }

        Some(TaskContext {
            cwd: Some(repository_path),
            task_variables,
            ..TaskContext::default()
        })
    }

    fn git_context_menu_tasks(
        &self,
        task_context: &TaskContext,
        cx: &App,
    ) -> Vec<(TaskSourceKind, ResolvedTask)> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };

        let project = workspace.read(cx).project().clone();

        let task_inventory = project.read_with(cx, |project, cx| {
            project.task_store().read(cx).task_inventory().cloned()
        });

        let Some(task_inventory) = task_inventory else {
            return Vec::new();
        };

        task_inventory
            .read(cx)
            .resolve_global_tasks_with_tag(GIT_COMMAND_TASK_TAG, task_context)
    }

    fn schedule_git_task(
        &mut self,
        task_source_kind: TaskSourceKind,
        resolved_task: ResolvedTask,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.schedule_resolved_task(
                    task_source_kind,
                    resolved_task,
                    false,
                    window,
                    cx,
                );
            })
            .ok();
    }

    fn deploy_entry_context_menu(
        &mut self,
        position: Point<Pixels>,
        index: usize,
        ref_name: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit) = self.graph_data.commits.get(index) else {
            return;
        };
        let sha = commit.data.sha;
        let sha_short = sha.display_short();
        let git_tasks = self
            .git_task_context(sha, ref_name.as_deref(), cx)
            .map(|task_context| self.git_context_menu_tasks(&task_context, cx))
            .unwrap_or_default();

        let header = match &ref_name {
            Some(ref_name) => format!("Ref {ref_name}"),
            None => format!("Commit {sha_short}"),
        };

        let focus_handle = self.focus_handle.clone();
        let git_graph = cx.entity();
        let context_menu = ContextMenu::build(window, cx, |context_menu, window, _| {
            context_menu
                .context(focus_handle)
                .header(header)
                .entry(
                    "View Commit",
                    Some(OpenCommitView.boxed_clone()),
                    window.handler_for(&git_graph, move |this, window, cx| {
                        this.open_commit_view(index, window, cx);
                    }),
                )
                .entry(
                    "Copy SHA",
                    Some(CopyCommitSha.boxed_clone()),
                    window.handler_for(&git_graph, move |this, _window, cx| {
                        this.copy_commit_sha(index, cx);
                    }),
                )
                .when_some(ref_name.clone(), |menu, ref_name| {
                    menu.entry("Copy Ref Name", None, move |_window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(ref_name.to_string()));
                    })
                })
                .when(ref_name.is_none(), |menu| {
                    menu.map(|menu| {
                        let tag_names = commit
                            .data
                            .tag_names()
                            .into_iter()
                            .map(|tag_name| SharedString::from(tag_name.to_string()))
                            .collect::<Vec<_>>();
                        let copy_tag_label = "Copy Tag";

                        match tag_names.as_slice() {
                            [] => menu.item(
                                ContextMenuEntry::new(copy_tag_label)
                                    .action(CopyCommitTag.boxed_clone())
                                    .disabled(true),
                            ),
                            [tag_name] => {
                                let tag_name = tag_name.clone();
                                let label = format!("{copy_tag_label}: {tag_name}");
                                menu.entry(
                                    label,
                                    Some(CopyCommitTag.boxed_clone()),
                                    move |_window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            tag_name.to_string(),
                                        ));
                                    },
                                )
                            }
                            _ => menu.submenu(copy_tag_label, move |menu, _window, _cx| {
                                let mut menu =
                                    menu.fixed_width(COMMIT_TAG_LIST_WIDTH_IN_REMS.into());

                                for tag_name in tag_names.clone() {
                                    let tag_name_to_copy = tag_name.clone();

                                    menu = menu.entry(tag_name, None, move |_window, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            tag_name_to_copy.to_string(),
                                        ));
                                    });
                                }
                                menu
                            }),
                        }
                    })
                })
                .map(|mut menu| {
                    menu = menu.separator().header("Custom Commands");

                    if git_tasks.is_empty() {
                        return menu.item(
                            ContextMenuEntry::new("Learn More")
                                .icon(IconName::ArrowUpRight)
                                .icon_color(Color::Muted)
                                .icon_position(IconPosition::End)
                                .handler(|_window, cx| {
                                    let docs_url = release_channel::docs_url(
                                        CUSTOM_GIT_COMMANDS_DOCS_SLUG,
                                        cx,
                                    );
                                    cx.open_url(&docs_url);
                                }),
                        );
                    }

                    for (task_source_kind, resolved_task) in git_tasks {
                        let label = resolved_task.display_label().to_string();

                        menu = menu.entry(
                            label,
                            None,
                            window.handler_for(&git_graph, move |this, window, cx| {
                                this.schedule_git_task(
                                    task_source_kind.clone(),
                                    resolved_task.clone(),
                                    window,
                                    cx,
                                );
                            }),
                        );
                    }

                    menu
                })
        });
        self.set_context_menu(context_menu, position, index, window, cx);
    }

    fn set_context_menu(
        &mut self,
        context_menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        entry_idx: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&context_menu.focus_handle(cx), cx);

        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu
                        .menu
                        .focus_handle(cx)
                        .contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some(GitGraphContextMenu {
            menu: context_menu,
            position,
            entry_idx,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let color = cx.theme().colors();
        let query_focus_handle = self
            .search_state
            .editor
            .focus_handle(cx)
            .tab_index(1)
            .tab_stop(true);
        let search_options = {
            let mut options = SearchOptions::NONE;
            options.set(
                SearchOptions::CASE_SENSITIVE,
                self.search_state.case_sensitive,
            );
            options
        };

        h_flex()
            .key_context("GitGraphSearchBar")
            .tab_index(1)
            .tab_group()
            .tab_stop(false)
            .w_full()
            .p_1p5()
            .gap_1p5()
            .border_b_1()
            .border_color(color.border_variant)
            .child(
                h_flex()
                    .h_8()
                    .flex_1()
                    .min_w_0()
                    .px_1p5()
                    .gap_1()
                    .track_focus(&query_focus_handle)
                    .border_1()
                    .border_color(color.border_variant)
                    .rounded_md()
                    .bg(color.toolbar_background)
                    .on_action(cx.listener(Self::confirm_search))
                    .child(self.search_state.editor.clone())
                    .child(SearchOption::CaseSensitive.as_button(
                        search_options,
                        SearchSource::Buffer,
                        query_focus_handle,
                    )),
            )
            .child(
                h_flex()
                    .min_w_64()
                    .gap_1()
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("git-graph-search-prev", IconName::ChevronLeft)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .tooltip(move |_, cx| {
                                Tooltip::for_action_in(
                                    "Select Previous Match",
                                    &SelectPreviousMatch,
                                    &focus_handle,
                                    cx,
                                )
                            })
                            .map(|this| {
                                if self.search_state.matches.is_empty() {
                                    this.disabled(true)
                                } else {
                                    this.disabled(false).on_click(cx.listener(|this, _, _, cx| {
                                        this.select_previous_match(cx);
                                    }))
                                }
                            })
                    })
                    .child({
                        let focus_handle = self.focus_handle.clone();
                        IconButton::new("git-graph-search-next", IconName::ChevronRight)
                            .shape(ui::IconButtonShape::Square)
                            .icon_size(IconSize::Small)
                            .tooltip(move |_, cx| {
                                Tooltip::for_action_in(
                                    "Select Next Match",
                                    &SelectNextMatch,
                                    &focus_handle,
                                    cx,
                                )
                            })
                            .map(|this| {
                                if self.search_state.matches.is_empty() {
                                    this.disabled(true)
                                } else {
                                    this.disabled(false).on_click(cx.listener(|this, _, _, cx| {
                                        this.select_next_match(cx);
                                    }))
                                }
                            })
                    })
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Label::new(format!(
                                    "{}/{}",
                                    self.search_state
                                        .selected_index
                                        .map(|index| index + 1)
                                        .unwrap_or(0),
                                    self.search_state.matches.len()
                                ))
                                .size(LabelSize::Small)
                                .when(self.search_state.matches.is_empty(), |this| {
                                    this.color(Color::Disabled)
                                }),
                            )
                            .when(
                                matches!(
                                    &self.search_state.state,
                                    QueryState::Confirmed((_, task)) if !task.is_ready()
                                ),
                                |this| {
                                    this.child(
                                        Icon::new(IconName::ArrowCircle)
                                            .color(Color::Accent)
                                            .size(IconSize::Small)
                                            .with_rotate_animation(2)
                                            .into_any_element(),
                                    )
                                },
                            ),
                    ),
            )
    }

    fn render_loading_spinner(&self, cx: &App) -> AnyElement {
        let rems = TextSize::Large.rems(cx);
        Icon::new(IconName::LoadCircle)
            .size(IconSize::Custom(rems))
            .color(Color::Accent)
            .with_rotate_animation(3)
            .into_any_element()
    }

    fn render_commit_detail_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(selected_idx) = self.selected_entry_idx else {
            return Empty.into_any_element();
        };

        let Some(commit_entry) = self.graph_data.commits.get(selected_idx) else {
            return Empty.into_any_element();
        };

        let Some(repository) = self.get_repository(cx) else {
            return Empty.into_any_element();
        };

        let data = repository.update(cx, |repository, cx| {
            repository
                .fetch_commit_data(commit_entry.data.sha, false, cx)
                .clone()
        });

        let full_sha: SharedString = commit_entry.data.sha.to_string().into();
        let ref_names = commit_entry.data.ref_names.clone();

        let head_branch_name: Option<SharedString> = repository
            .read(cx)
            .snapshot()
            .branch
            .as_ref()
            .map(|branch| SharedString::from(branch.name().to_string()));

        let accent_colors = cx.theme().accents();
        let accent_color = accent_colors
            .0
            .get(commit_entry.color_idx)
            .copied()
            .unwrap_or_else(|| accent_colors.0.first().copied().unwrap_or_default());

        // todo(git graph): We should use the full commit message here
        let (author_name, author_email, commit_timestamp, commit_message) = match &data {
            CommitDataState::Loaded(data) => (
                data.author_name.clone(),
                data.author_email.clone(),
                Some(data.commit_timestamp),
                data.subject.clone(),
            ),
            CommitDataState::Loading(_) => ("Loading…".into(), "".into(), None, "Loading…".into()),
        };

        let date_string = commit_timestamp
            .and_then(|ts| OffsetDateTime::from_unix_timestamp(ts).ok())
            .map(|datetime| {
                let local_offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
                let local_datetime = datetime.to_offset(local_offset);
                let format =
                    time::format_description::parse("[month repr:short] [day], [year]").ok();
                format
                    .and_then(|f| local_datetime.format(&f).ok())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        let remote = repository.update(cx, |repo, cx| {
            let remote_url = repo.default_remote_url()?;
            let provider_registry = GitHostingProviderRegistry::default_global(cx);
            let (provider, parsed) = parse_git_remote_url(provider_registry, &remote_url)?;
            Some(GitRemote {
                host: provider,
                owner: parsed.owner.into(),
                repo: parsed.repo.into(),
            })
        });

        let avatar = {
            let author_email_for_avatar = if author_email.is_empty() {
                None
            } else {
                Some(author_email.clone())
            };

            CommitAvatar::new(&full_sha, author_email_for_avatar, remote.as_ref())
                .size(px(40.))
                .render(window, cx)
        };

        let changed_files_count = self
            .selected_commit_diff
            .as_ref()
            .map(|diff| diff.files.len())
            .unwrap_or(0);

        let (total_lines_added, total_lines_removed) =
            self.selected_commit_diff_stats.unwrap_or((0, 0));

        let changed_file_entries: Vec<ChangedFileEntry> = self
            .selected_commit_diff
            .as_ref()
            .map(|diff| {
                let mut files = diff.files.iter().collect::<Vec<_>>();
                if !self.changed_files_view_mode.is_tree() {
                    files.sort_by_key(|file| file.status());
                }
                files
                    .into_iter()
                    .map(|file| ChangedFileEntry::from_commit_file(file, cx))
                    .collect()
            })
            .unwrap_or_default();
        let changed_file_entries = Rc::new(changed_file_entries);
        let tree_entries: Rc<Vec<ChangedFileTreeEntry>> = if self.changed_files_view_mode.is_tree()
        {
            Rc::new(build_changed_file_tree_entries(
                changed_file_entries.as_ref().clone(),
                &self.changed_files_expanded_dirs,
            ))
        } else {
            Rc::default()
        };

        v_flex()
            .min_w(px(300.))
            .h_full()
            .bg(cx.theme().colors().editor_background)
            .flex_basis(DefiniteLength::Fraction(
                self.commit_details_split_state.read(cx).right_ratio(),
            ))
            .child(
                v_flex()
                    .relative()
                    .w_full()
                    .p_2()
                    .gap_2()
                    .child(
                        div().absolute().top_2().right_2().child(
                            IconButton::new("close-detail", IconName::Close)
                                .icon_size(IconSize::Small)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected_entry_idx = None;
                                    this.selected_commit_diff = None;
                                    this.selected_commit_diff_stats = None;
                                    this.changed_files_expanded_dirs.clear();
                                    this._commit_diff_task = None;
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        v_flex()
                            .py_1()
                            .w_full()
                            .items_center()
                            .gap_1()
                            .child(avatar)
                            .child(
                                v_flex()
                                    .items_center()
                                    .child(Label::new(author_name))
                                    .child(
                                        Label::new(date_string)
                                            .color(Color::Muted)
                                            .size(LabelSize::Small),
                                    ),
                            ),
                    )
                    .children((!ref_names.is_empty()).then(|| {
                        h_flex().gap_1().flex_wrap().justify_center().children(
                            ref_names.iter().map(|name| {
                                let is_head = Self::is_head_ref(name.as_ref(), &head_branch_name);
                                self.render_ref_chip(name, accent_color, is_head, selected_idx, cx)
                            }),
                        )
                    }))
                    .child(
                        v_flex()
                            .ml_neg_1()
                            .gap_1p5()
                            .when(!author_email.is_empty(), |this| {
                                let copied_state: Entity<CopiedState> = window.use_keyed_state(
                                    "author-email-copy",
                                    cx,
                                    CopiedState::new,
                                );
                                let is_copied = copied_state.read(cx).is_copied();

                                let (icon, icon_color, tooltip_label) = if is_copied {
                                    (IconName::Check, Color::Success, "Email Copied!")
                                } else {
                                    (IconName::Envelope, Color::Muted, "Copy Email")
                                };

                                let copy_email = author_email.clone();
                                let author_email_for_tooltip = author_email.clone();

                                this.child(
                                    Button::new("author-email-copy", author_email.clone())
                                        .start_icon(
                                            Icon::new(icon).size(IconSize::Small).color(icon_color),
                                        )
                                        .label_size(LabelSize::Small)
                                        .truncate(true)
                                        .color(Color::Muted)
                                        .tooltip(move |_, cx| {
                                            Tooltip::with_meta(
                                                tooltip_label,
                                                None,
                                                author_email_for_tooltip.clone(),
                                                cx,
                                            )
                                        })
                                        .on_click(move |_, _, cx| {
                                            copied_state.update(cx, |state, _cx| {
                                                state.mark_copied();
                                            });
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                copy_email.to_string(),
                                            ));
                                            let state_id = copied_state.entity_id();
                                            cx.spawn(async move |cx| {
                                                cx.background_executor()
                                                    .timer(COPIED_STATE_DURATION)
                                                    .await;
                                                cx.update(|cx| {
                                                    cx.notify(state_id);
                                                })
                                            })
                                            .detach();
                                        }),
                                )
                            })
                            .child({
                                let copy_sha = full_sha.clone();
                                let copied_state: Entity<CopiedState> =
                                    window.use_keyed_state("sha-copy", cx, CopiedState::new);
                                let is_copied = copied_state.read(cx).is_copied();

                                let (icon, icon_color, tooltip_label) = if is_copied {
                                    (IconName::Check, Color::Success, "Commit SHA Copied!")
                                } else {
                                    (IconName::Hash, Color::Muted, "Copy Commit SHA")
                                };

                                Button::new("sha-button", &full_sha)
                                    .start_icon(
                                        Icon::new(icon).size(IconSize::Small).color(icon_color),
                                    )
                                    .label_size(LabelSize::Small)
                                    .truncate(true)
                                    .color(Color::Muted)
                                    .tooltip({
                                        let full_sha = full_sha.clone();
                                        move |_, cx| {
                                            Tooltip::with_meta(
                                                tooltip_label,
                                                None,
                                                full_sha.clone(),
                                                cx,
                                            )
                                        }
                                    })
                                    .on_click(move |_, _, cx| {
                                        copied_state.update(cx, |state, _cx| {
                                            state.mark_copied();
                                        });
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            copy_sha.to_string(),
                                        ));
                                        let state_id = copied_state.entity_id();
                                        cx.spawn(async move |cx| {
                                            cx.background_executor()
                                                .timer(COPIED_STATE_DURATION)
                                                .await;
                                            cx.update(|cx| {
                                                cx.notify(state_id);
                                            })
                                        })
                                        .detach();
                                    })
                            })
                            .when_some(remote.clone(), |this, remote| {
                                let provider_name = remote.host.name();
                                let icon = crate::get_provider_icon(provider_name.as_str());
                                let parsed_remote = ParsedGitRemote {
                                    owner: remote.owner.as_ref().into(),
                                    repo: remote.repo.as_ref().into(),
                                };
                                let params = BuildCommitPermalinkParams {
                                    sha: full_sha.as_ref(),
                                };
                                let url = remote
                                    .host
                                    .build_commit_permalink(&parsed_remote, params)
                                    .to_string();

                                this.child(
                                    Button::new(
                                        "view-on-provider",
                                        format!("View on {}", provider_name),
                                    )
                                    .start_icon(
                                        Icon::new(icon).size(IconSize::Small).color(Color::Muted),
                                    )
                                    .label_size(LabelSize::Small)
                                    .truncate(true)
                                    .color(Color::Muted)
                                    .on_click(
                                        move |_, _, cx| {
                                            cx.open_url(&url);
                                        },
                                    ),
                                )
                            }),
                    ),
            )
            .child(Divider::horizontal())
            .child(div().p_2().child(Label::new(commit_message)))
            .child(Divider::horizontal())
            .child(
                v_flex()
                    .min_w_0()
                    .p_2()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1()
                            .w_full()
                            .justify_between()
                            .child(
                                Label::new(format!(
                                    "{} Changed {}",
                                    changed_files_count,
                                    if changed_files_count == 1 {
                                        "File"
                                    } else {
                                        "Files"
                                    }
                                ))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child(DiffStat::new(
                                        "commit-diff-stat",
                                        total_lines_added,
                                        total_lines_removed,
                                    ))
                                    .child(
                                        IconButton::new(
                                            "toggle-changed-files-view",
                                            IconName::ListTree,
                                        )
                                        .shape(ui::IconButtonShape::Square)
                                        .icon_size(IconSize::Small)
                                        .toggle_state(self.changed_files_view_mode.is_tree())
                                        .tooltip({
                                            let tooltip = if self.changed_files_view_mode.is_tree()
                                            {
                                                "Show Flat View"
                                            } else {
                                                "Show Tree View"
                                            };
                                            move |_, cx| {
                                                Tooltip::for_action(
                                                    tooltip,
                                                    &ToggleChangedFilesView,
                                                    cx,
                                                )
                                            }
                                        })
                                        .on_click(
                                            cx.listener(|this, _, _window, cx| {
                                                this.changed_files_view_mode =
                                                    this.changed_files_view_mode.toggled();
                                                this.changed_files_scroll_handle
                                                    .scroll_to_item(0, ScrollStrategy::Top);
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("changed-files-container")
                            .flex_1()
                            .min_h_0()
                            .child({
                                let flat_entries = changed_file_entries;
                                let is_tree_view = self.changed_files_view_mode.is_tree();
                                let entry_count = if is_tree_view {
                                    tree_entries.len()
                                } else {
                                    flat_entries.len()
                                };
                                let commit_sha = full_sha.clone();
                                let repository = repository.downgrade();
                                let workspace = self.workspace.clone();
                                let git_graph = cx.weak_entity();
                                uniform_list(
                                    "changed-files-list",
                                    entry_count,
                                    move |range, _window, cx| {
                                        range
                                            .map(|ix| {
                                                if is_tree_view {
                                                    match &tree_entries[ix] {
                                                        ChangedFileTreeEntry::Directory(entry) => {
                                                            entry.render(ix, git_graph.clone(), cx)
                                                        }
                                                        ChangedFileTreeEntry::File(entry) => {
                                                            entry.entry.render(
                                                                ix,
                                                                entry.depth,
                                                                None,
                                                                commit_sha.clone(),
                                                                repository.clone(),
                                                                workspace.clone(),
                                                                cx,
                                                            )
                                                        }
                                                    }
                                                } else {
                                                    let directory_label = (!flat_entries[ix]
                                                        .dir_path
                                                        .is_empty())
                                                    .then(|| flat_entries[ix].dir_path.clone());
                                                    flat_entries[ix].render(
                                                        ix,
                                                        0,
                                                        directory_label,
                                                        commit_sha.clone(),
                                                        repository.clone(),
                                                        workspace.clone(),
                                                        cx,
                                                    )
                                                }
                                            })
                                            .collect()
                                    },
                                )
                                .size_full()
                                .ml_neg_1()
                                .track_scroll(&self.changed_files_scroll_handle)
                            })
                            .vertical_scrollbar_for(&self.changed_files_scroll_handle, window, cx),
                    ),
            )
            .child(Divider::horizontal())
            .child(
                h_flex().p_1p5().w_full().child(
                    Button::new("view-commit", "View Commit")
                        .full_width()
                        .style(ButtonStyle::OutlinedGhost)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_selected_commit_view(window, cx);
                        })),
                ),
            )
            .into_any_element()
    }

    fn render_graph_canvas(&self, window: &Window, cx: &mut Context<GitGraph>) -> impl IntoElement {
        let row_height = Self::row_height(window, cx);
        let visible_row_count = self.visible_row_count(window, cx);
        let table_state = self.table_interaction_state.read(cx);
        let viewport_height = table_state
            .scroll_handle
            .0
            .borrow()
            .last_item_size
            .map(|size| size.item.height)
            .unwrap_or(window.viewport_size().height);
        let loaded_commit_count = self.graph_data.commits.len();

        let content_height = row_height * loaded_commit_count;
        let max_scroll = (content_height - viewport_height).max(px(0.));
        let scroll_offset_y = (-table_state.scroll_offset().y).clamp(px(0.), max_scroll);

        let first_visible_row = (scroll_offset_y / row_height).floor() as usize;
        let vertical_scroll_offset = scroll_offset_y - (first_visible_row as f32 * row_height);

        let graph_viewport_width = self.graph_viewport_width(window, cx);
        let graph_width = if self.graph_canvas_content_width() > graph_viewport_width {
            self.graph_canvas_content_width()
        } else {
            graph_viewport_width
        };
        let last_visible_row = first_visible_row + visible_row_count + 1;

        let viewport_range = first_visible_row.min(loaded_commit_count.saturating_sub(1))
            ..(last_visible_row).min(loaded_commit_count);
        let rows = self.graph_data.commits[viewport_range.clone()].to_vec();
        let commit_lines: Vec<_> = self
            .graph_data
            .lines
            .iter()
            .filter(|line| {
                line.full_interval.start <= viewport_range.end
                    && line.full_interval.end >= viewport_range.start
            })
            .cloned()
            .collect();

        let mut lines: BTreeMap<usize, Vec<_>> = BTreeMap::new();

        let hovered_entry_idx = self.hovered_entry_idx;
        let selected_entry_idx = self.selected_entry_idx;
        let context_menu_entry_idx = self.context_menu.as_ref().map(|menu| menu.entry_idx);
        let is_focused = self.focus_handle.is_focused(window);
        let graph_canvas_bounds = self.graph_canvas_bounds.clone();

        gpui::canvas(
            move |_bounds, _window, _cx| {},
            move |bounds: Bounds<Pixels>, _: (), window: &mut Window, cx: &mut App| {
                graph_canvas_bounds.set(Some(bounds));

                window.paint_layer(bounds, |window| {
                    let accent_colors = cx.theme().accents();

                    let hover_bg = cx.theme().colors().element_hover.opacity(0.6);
                    let selected_bg = if is_focused {
                        cx.theme().colors().element_selected
                    } else {
                        cx.theme().colors().element_hover
                    };

                    for visible_row_idx in 0..rows.len() {
                        let absolute_row_idx = first_visible_row + visible_row_idx;
                        let is_hovered = hovered_entry_idx == Some(absolute_row_idx);
                        let is_selected = selected_entry_idx == Some(absolute_row_idx);
                        let is_context_menu_target =
                            context_menu_entry_idx == Some(absolute_row_idx);

                        if is_hovered || is_selected || is_context_menu_target {
                            let row_y = bounds.origin.y + visible_row_idx as f32 * row_height
                                - vertical_scroll_offset;

                            let row_bounds = Bounds::new(
                                point(bounds.origin.x, row_y),
                                gpui::Size {
                                    width: bounds.size.width,
                                    height: row_height,
                                },
                            );

                            let bg_color = if is_selected || is_context_menu_target {
                                selected_bg
                            } else {
                                hover_bg
                            };
                            window.paint_quad(gpui::fill(row_bounds, bg_color));
                        }
                    }

                    for (row_idx, row) in rows.into_iter().enumerate() {
                        let row_color = accent_colors.color_for_index(row.color_idx as u32);
                        let row_y_center =
                            bounds.origin.y + row_idx as f32 * row_height + row_height / 2.0
                                - vertical_scroll_offset;

                        let commit_x = lane_center_x(bounds, row.lane as f32);

                        draw_commit_circle(commit_x, row_y_center, row_color, window);
                    }

                    for line in commit_lines {
                        let Some((start_segment_idx, start_column)) =
                            line.get_first_visible_segment_idx(first_visible_row)
                        else {
                            continue;
                        };

                        let line_x = lane_center_x(bounds, start_column as f32);

                        let start_row = line.full_interval.start as i32 - first_visible_row as i32;

                        let from_y =
                            bounds.origin.y + start_row as f32 * row_height + row_height / 2.0
                                - vertical_scroll_offset
                                + COMMIT_CIRCLE_RADIUS;

                        let mut current_row = from_y;
                        let mut current_column = line_x;

                        let mut builder = PathBuilder::stroke(LINE_WIDTH);
                        builder.move_to(point(line_x, from_y));

                        let segments = &line.segments[start_segment_idx..];
                        let desired_curve_height = row_height / 3.0;
                        let desired_curve_width = LANE_WIDTH / 3.0;

                        for (segment_idx, segment) in segments.iter().enumerate() {
                            let is_last = segment_idx + 1 == segments.len();

                            match segment {
                                CommitLineSegment::Straight { to_row } => {
                                    let mut dest_row = to_row_center(
                                        to_row - first_visible_row,
                                        row_height,
                                        vertical_scroll_offset,
                                        bounds,
                                    );
                                    if is_last {
                                        dest_row -= COMMIT_CIRCLE_RADIUS;
                                    }

                                    let dest_point = point(current_column, dest_row);

                                    current_row = dest_point.y;
                                    builder.line_to(dest_point);
                                    builder.move_to(dest_point);
                                }
                                CommitLineSegment::Curve {
                                    to_column,
                                    on_row,
                                    curve_kind,
                                } => {
                                    let mut to_column = lane_center_x(bounds, *to_column as f32);

                                    let mut to_row = to_row_center(
                                        *on_row - first_visible_row,
                                        row_height,
                                        vertical_scroll_offset,
                                        bounds,
                                    );

                                    // This means that this branch was a checkout
                                    let going_right = to_column > current_column;
                                    let column_shift = if going_right {
                                        COMMIT_CIRCLE_RADIUS + COMMIT_CIRCLE_STROKE_WIDTH
                                    } else {
                                        -COMMIT_CIRCLE_RADIUS - COMMIT_CIRCLE_STROKE_WIDTH
                                    };

                                    match curve_kind {
                                        CurveKind::Checkout => {
                                            if is_last {
                                                to_column -= column_shift;
                                            }

                                            let available_curve_width =
                                                (to_column - current_column).abs();
                                            let available_curve_height =
                                                (to_row - current_row).abs();
                                            let curve_width =
                                                desired_curve_width.min(available_curve_width);
                                            let curve_height =
                                                desired_curve_height.min(available_curve_height);
                                            let signed_curve_width = if going_right {
                                                curve_width
                                            } else {
                                                -curve_width
                                            };
                                            let curve_start =
                                                point(current_column, to_row - curve_height);
                                            let curve_end =
                                                point(current_column + signed_curve_width, to_row);
                                            let curve_control = point(current_column, to_row);

                                            builder.move_to(point(current_column, current_row));
                                            builder.line_to(curve_start);
                                            builder.move_to(curve_start);
                                            builder.curve_to(curve_end, curve_control);
                                            builder.move_to(curve_end);
                                            builder.line_to(point(to_column, to_row));
                                        }
                                        CurveKind::Merge => {
                                            if is_last {
                                                to_row -= COMMIT_CIRCLE_RADIUS;
                                            }

                                            let merge_start = point(
                                                current_column + column_shift,
                                                current_row - COMMIT_CIRCLE_RADIUS,
                                            );
                                            let available_curve_width =
                                                (to_column - merge_start.x).abs();
                                            let available_curve_height =
                                                (to_row - merge_start.y).abs();
                                            let curve_width =
                                                desired_curve_width.min(available_curve_width);
                                            let curve_height =
                                                desired_curve_height.min(available_curve_height);
                                            let signed_curve_width = if going_right {
                                                curve_width
                                            } else {
                                                -curve_width
                                            };
                                            let curve_start = point(
                                                to_column - signed_curve_width,
                                                merge_start.y,
                                            );
                                            let curve_end =
                                                point(to_column, merge_start.y + curve_height);
                                            let curve_control = point(to_column, merge_start.y);

                                            builder.move_to(merge_start);
                                            builder.line_to(curve_start);
                                            builder.move_to(curve_start);
                                            builder.curve_to(curve_end, curve_control);
                                            builder.move_to(curve_end);
                                            builder.line_to(point(to_column, to_row));
                                        }
                                    }
                                    current_row = to_row;
                                    current_column = to_column;
                                    builder.move_to(point(current_column, current_row));
                                }
                            }
                        }

                        builder.close();
                        lines.entry(line.color_idx).or_default().push(builder);
                    }

                    for (color_idx, builders) in lines {
                        let line_color = accent_colors.color_for_index(color_idx as u32);

                        for builder in builders {
                            if let Ok(path) = builder.build() {
                                // we paint each color on it's own layer to stop overlapping lines
                                // of different colors changing the color of a line
                                window.paint_layer(bounds, |window| {
                                    window.paint_path(path, line_color);
                                });
                            }
                        }
                    }
                })
            },
        )
        .w(graph_width)
        .h_full()
    }

    fn row_at_position(
        &self,
        position_y: Pixels,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<usize> {
        let canvas_bounds = self.graph_canvas_bounds.get()?;
        let table_state = self.table_interaction_state.read(cx);
        let scroll_offset_y = -table_state.scroll_offset().y;

        let local_y = position_y - canvas_bounds.origin.y;

        if local_y >= px(0.) && local_y < canvas_bounds.size.height {
            let absolute_y = local_y + scroll_offset_y;
            let row_height = Self::row_height(window, cx);
            let absolute_row = (absolute_y / row_height).floor() as usize;

            if absolute_row < self.graph_data.commits.len() {
                return Some(absolute_row);
            }
        }

        None
    }

    fn handle_graph_mouse_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.row_at_position(event.position.y, window, cx) {
            if self.hovered_entry_idx != Some(row) {
                self.hovered_entry_idx = Some(row);
                cx.notify();
            }
        } else if self.hovered_entry_idx.is_some() {
            self.hovered_entry_idx = None;
            cx.notify();
        }
    }

    fn handle_entry_click(
        &mut self,
        entry_idx: usize,
        event: &ClickEvent,
        scroll_strategy: ScrollStrategy,
        focus_handle: Option<&FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Right-clicks open the context menu, not the details panel.
        if event.is_right_click() {
            return;
        }

        if let Some(focus_handle) = focus_handle {
            focus_handle.focus(window, cx);
        }

        self.select_entry(entry_idx, scroll_strategy, cx);

        if event.click_count() >= 2 {
            self.open_commit_view(entry_idx, window, cx);
        }
    }

    fn handle_graph_click(
        &mut self,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.row_at_position(event.position().y, window, cx) {
            self.handle_entry_click(row, event, ScrollStrategy::Nearest, None, window, cx);
        }
    }

    fn handle_entry_secondary_mouse_down(
        &mut self,
        entry_idx: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.deploy_entry_context_menu(event.position, entry_idx, None, window, cx);
        cx.stop_propagation();
    }

    fn handle_graph_secondary_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.row_at_position(event.position.y, window, cx) else {
            return;
        };

        self.handle_entry_secondary_mouse_down(row, event, window, cx);
    }

    fn handle_graph_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line_height = window.line_height();
        let delta = event.delta.pixel_delta(line_height);

        let table_state = self.table_interaction_state.read(cx);
        let current_offset = table_state.scroll_offset();

        let viewport_height = table_state.scroll_handle.viewport().size.height;

        let commit_count = match self.graph_data.max_commit_count {
            AllCommitCount::Loading(count) => count,
            AllCommitCount::FullyLoaded(count) => count,
            AllCommitCount::NotLoaded => self.graph_data.commits.len(),
        };
        let content_height = Self::row_height(window, cx) * commit_count;
        let max_vertical_scroll = (viewport_height - content_height).min(px(0.));

        let new_y = (current_offset.y + delta.y).clamp(max_vertical_scroll, px(0.));
        let new_offset = Point::new(current_offset.x, new_y);

        if new_offset != current_offset {
            table_state.set_scroll_offset(new_offset);
            cx.notify();
        }
    }

    fn commit_count_and_loading_state(&mut self, cx: &mut Context<Self>) -> (usize, bool) {
        match self.graph_data.max_commit_count {
            AllCommitCount::FullyLoaded(count) => (count, false),
            AllCommitCount::Loading(count) => {
                let is_loading = self
                    .get_repository(cx)
                    .map(|repository| {
                        repository.update(cx, |repository, cx| {
                            repository
                                .graph_data(self.log_source.clone(), self.log_order, 0..0, cx)
                                .is_loading
                        })
                    })
                    .unwrap_or(false);

                (count, is_loading)
            }
            AllCommitCount::NotLoaded => {
                let (commit_count, is_loading) = if let Some(repository) = self.get_repository(cx) {
                    repository.update(cx, |repository, cx| {
                        // Start loading the graph data if we haven't started already
                        let GraphDataResponse {
                            commits,
                            is_loading,
                            error: _,
                        } = repository.graph_data(
                            self.log_source.clone(),
                            self.log_order,
                            0..usize::MAX,
                            cx,
                        );
                        self.graph_data.add_commits(commits);
                        (commits.len(), is_loading)
                    })
                } else {
                    (0, false)
                };

                (commit_count, is_loading)
            }
        }
    }

    fn render_commit_view_resize_handle(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("commit-view-split-resize-container")
            .relative()
            .h_full()
            .flex_shrink_0()
            .w(px(1.))
            .bg(cx.theme().colors().border_variant)
            .child(
                div()
                    .id("commit-view-split-resize-handle")
                    .absolute()
                    .left(px(-RESIZE_HANDLE_WIDTH / 2.0))
                    .w(px(RESIZE_HANDLE_WIDTH))
                    .h_full()
                    .cursor_col_resize()
                    .block_mouse_except_scroll()
                    .on_click(cx.listener(|this, event: &ClickEvent, _window, cx| {
                        if event.click_count() >= 2 {
                            this.commit_details_split_state.update(cx, |state, _| {
                                state.on_double_click();
                            });
                        }
                        cx.stop_propagation();
                    }))
                    .on_drag(DraggedSplitHandle, |_, _, _, cx| cx.new(|_| gpui::Empty)),
            )
            .into_any_element()
    }
}

impl Render for GitGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // This happens when we changed branches, we should refresh our search as well
        if let QueryState::Pending(query) = &mut self.search_state.state {
            let query = std::mem::take(query);
            self.search_state.state = QueryState::Empty;
            self.search(query, cx);
        }
        let (commit_count, is_loading) = self.commit_count_and_loading_state(cx);

        let error = self.get_repository(cx).and_then(|repo| {
            repo.read(cx)
                .get_graph_data(self.log_source.clone(), self.log_order)
                .and_then(|data| data.error.clone())
        });

        let content = if commit_count == 0 {
            let message = if let Some(error) = &error {
                format!("Error loading: {}", error)
            } else if is_loading {
                "Loading".to_string()
            } else {
                "No commits found".to_string()
            };
            let label = Label::new(message)
                .color(Color::Muted)
                .size(LabelSize::Large);
            div()
                .size_full()
                .h_flex()
                .gap_1()
                .items_center()
                .justify_center()
                .child(label)
                .when(is_loading && error.is_none(), |this| {
                    this.child(self.render_loading_spinner(cx))
                })
        } else {
            let is_path_history = matches!(self.log_source, LogSource::Path(_));
            let header_resize_info =
                HeaderResizeInfo::from_redistributable(&self.column_widths, cx);
            let header_context = TableRenderContext::for_column_widths(
                Some(self.column_widths.read(cx).widths_to_render()),
                true,
            );
            let [
                graph_fraction,
                description_fraction,
                date_fraction,
                author_fraction,
                commit_fraction,
            ] = self.preview_column_fractions(window, cx);
            let table_fraction =
                description_fraction + date_fraction + author_fraction + commit_fraction;
            let table_width_config = self.table_column_width_config(window, cx);

            h_flex()
                .size_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .size_full()
                        .flex()
                        .flex_col()
                        .child(render_table_header(
                            if !is_path_history {
                                TableRow::from_vec(
                                    vec![
                                        Label::new("Graph")
                                            .color(Color::Muted)
                                            .truncate()
                                            .into_any_element(),
                                        Label::new("Description")
                                            .color(Color::Muted)
                                            .into_any_element(),
                                        Label::new("Date").color(Color::Muted).into_any_element(),
                                        Label::new("Author").color(Color::Muted).into_any_element(),
                                        Label::new("Commit").color(Color::Muted).into_any_element(),
                                    ],
                                    5,
                                )
                            } else {
                                TableRow::from_vec(
                                    vec![
                                        Label::new("Description")
                                            .color(Color::Muted)
                                            .into_any_element(),
                                        Label::new("Date").color(Color::Muted).into_any_element(),
                                        Label::new("Author").color(Color::Muted).into_any_element(),
                                        Label::new("Commit").color(Color::Muted).into_any_element(),
                                    ],
                                    4,
                                )
                            },
                            header_context,
                            Some(header_resize_info),
                            Some(self.column_widths.entity_id()),
                            cx,
                        ))
                        .child({
                            let row_height = Self::row_height(window, cx);
                            let selected_entry_idx = self.selected_entry_idx;
                            let hovered_entry_idx = self.hovered_entry_idx;
                            let context_menu_entry_idx =
                                self.context_menu.as_ref().map(|menu| menu.entry_idx);
                            let weak_self = cx.weak_entity();
                            let focus_handle = self.focus_handle.clone();
                            let table_focus_handle =
                                self.table_interaction_state.read(cx).focus_handle.clone();

                            let graph_canvas = div()
                                .id("graph-canvas")
                                .size_full()
                                .overflow_hidden()
                                .cursor_pointer()
                                .child(
                                    div()
                                        .size_full()
                                        .child(self.render_graph_canvas(window, cx)),
                                )
                                .on_scroll_wheel(cx.listener(Self::handle_graph_scroll))
                                .on_mouse_move(cx.listener(Self::handle_graph_mouse_move))
                                .on_click(cx.listener(Self::handle_graph_click))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(Self::handle_graph_secondary_mouse_down),
                                )
                                .on_hover(cx.listener(|this, &is_hovered: &bool, _, cx| {
                                    if !is_hovered && this.hovered_entry_idx.is_some() {
                                        this.hovered_entry_idx = None;
                                        cx.notify();
                                    }
                                }));

                            let commits_table = Table::new(4)
                                .interactable(&self.table_interaction_state)
                                .hide_row_borders()
                                .hide_row_hover()
                                .width_config(table_width_config)
                                .map_row(move |(index, row), window, cx| {
                                    let is_selected = selected_entry_idx == Some(index);
                                    let is_hovered = hovered_entry_idx == Some(index);
                                    let is_context_menu_target =
                                        context_menu_entry_idx == Some(index);
                                    let table_focus_handle = table_focus_handle.clone();
                                    let is_focused = focus_handle.is_focused(window)
                                        || table_focus_handle.is_focused(window);
                                    let weak = weak_self.clone();
                                    let weak_for_hover = weak.clone();
                                    let weak_for_context_menu = weak.clone();

                                    let hover_bg = cx.theme().colors().element_hover.opacity(0.6);
                                    let selected_bg = if is_focused {
                                        cx.theme().colors().element_selected
                                    } else {
                                        cx.theme().colors().element_hover
                                    };

                                    row.h(row_height)
                                        .cursor_pointer()
                                        .when(is_selected || is_context_menu_target, |row| {
                                            row.bg(selected_bg)
                                        })
                                        .when(
                                            is_hovered && !is_selected && !is_context_menu_target,
                                            |row| row.bg(hover_bg),
                                        )
                                        .on_hover(move |&is_hovered, _, cx| {
                                            weak_for_hover
                                                .update(cx, |this, cx| {
                                                    if is_hovered {
                                                        if this.hovered_entry_idx != Some(index) {
                                                            this.hovered_entry_idx = Some(index);
                                                            cx.notify();
                                                        }
                                                    } else if this.hovered_entry_idx == Some(index)
                                                    {
                                                        this.hovered_entry_idx = None;
                                                        cx.notify();
                                                    }
                                                })
                                                .ok();
                                        })
                                        .on_click(move |event, window, cx| {
                                            weak.update(cx, |this, cx| {
                                                this.handle_entry_click(
                                                    index,
                                                    event,
                                                    ScrollStrategy::Center,
                                                    Some(&table_focus_handle),
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .ok();
                                        })
                                        .on_mouse_down(
                                            MouseButton::Right,
                                            move |event: &MouseDownEvent, window, cx| {
                                                weak_for_context_menu
                                                    .update(cx, |this, cx| {
                                                        this.handle_entry_secondary_mouse_down(
                                                            index, event, window, cx,
                                                        );
                                                    })
                                                    .ok();
                                            },
                                        )
                                        .into_any_element()
                                })
                                .uniform_list(
                                    "git-graph-commits",
                                    commit_count,
                                    cx.processor(Self::render_table_rows),
                                );

                            bind_redistributable_columns(
                                div()
                                    .relative()
                                    .flex_1()
                                    .w_full()
                                    .overflow_hidden()
                                    .child(
                                        h_flex()
                                            .size_full()
                                            .when(!is_path_history, |this| {
                                                this.child(
                                                    div()
                                                        .w(DefiniteLength::Fraction(graph_fraction))
                                                        .h_full()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .child(graph_canvas),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .tab_index(2)
                                                    .tab_group()
                                                    .tab_stop(false)
                                                    .w(DefiniteLength::Fraction(table_fraction))
                                                    .h_full()
                                                    .min_w_0()
                                                    .child(commits_table),
                                            ),
                                    )
                                    .child(render_redistributable_columns_resize_handles(
                                        &self.column_widths,
                                        window,
                                        cx,
                                    )),
                                self.column_widths.clone(),
                            )
                        }),
                )
                .on_drag_move::<DraggedSplitHandle>(cx.listener(|this, event, window, cx| {
                    this.commit_details_split_state.update(cx, |state, cx| {
                        state.on_drag_move(event, window, cx);
                    });
                }))
                .on_drop::<DraggedSplitHandle>(cx.listener(|this, _event, _window, cx| {
                    this.commit_details_split_state.update(cx, |state, _cx| {
                        state.commit_ratio();
                    });
                }))
                .when(self.selected_entry_idx.is_some(), |this| {
                    this.child(self.render_commit_view_resize_handle(window, cx))
                        .child(self.render_commit_detail_panel(window, cx))
                })
        };

        div()
            .key_context("GitGraph")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .on_action(cx.listener(|this, _: &OpenCommitView, window, cx| {
                this.open_selected_commit_view(window, cx);
            }))
            .on_action(cx.listener(Self::copy_selected_commit_sha))
            .on_action(cx.listener(Self::copy_selected_commit_tag))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search_state
                    .editor
                    .update(cx, |editor, cx| editor.focus_handle(cx).focus(window, cx));
                this.activate_search_editor_if_focused(window, cx);
            }))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::scroll_up))
            .on_action(cx.listener(Self::scroll_down))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::toggle_changed_files_view))
            .on_action(cx.listener(Self::focus_next_tab_stop))
            .on_action(cx.listener(Self::focus_previous_tab_stop))
            .on_action(cx.listener(|this, _: &SelectNextMatch, _window, cx| {
                this.select_next_match(cx);
            }))
            .on_action(cx.listener(|this, _: &SelectPreviousMatch, _window, cx| {
                this.select_previous_match(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCaseSensitive, _window, cx| {
                this.search_state.case_sensitive = !this.search_state.case_sensitive;
                this.search_state.state.next_state();
                cx.emit(ItemEvent::Edit);
                cx.notify();
            }))
            .child(
                v_flex()
                    .size_full()
                    .child(self.render_search_bar(cx))
                    .child(div().flex_1().child(content)),
            )
            .children(self.context_menu.as_ref().map(|context_menu| {
                deferred(
                    anchored()
                        .position(context_menu.position)
                        .anchor(Anchor::TopLeft)
                        .child(context_menu.menu.clone()),
                )
                .with_priority(1)
            }))
            .on_action(cx.listener(|_, _: &buffer_search::Deploy, window, cx| {
                window.dispatch_action(Box::new(FocusSearch), cx);
                cx.stop_propagation();
            }))
    }
}

impl EventEmitter<ItemEvent> for GitGraph {}

impl Focusable for GitGraph {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Item for GitGraph {
    type Event = ItemEvent;

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::GitGraph))
    }

    fn tab_tooltip_content(&self, cx: &App) -> Option<TabTooltipContent> {
        let repo_name = self.get_repository(cx).and_then(|repo| {
            repo.read(cx)
                .work_directory_abs_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
        });
        let path_history_path = match &self.log_source {
            LogSource::Path(path) => Some(path.as_unix_str().to_string()),
            _ => None,
        };

        Some(TabTooltipContent::Custom(Box::new(Tooltip::element({
            move |_, _| {
                v_flex()
                    .child(Label::new(if path_history_path.is_some() {
                        "Path History"
                    } else {
                        "Git Graph"
                    }))
                    .when_some(path_history_path.clone(), |this, path| {
                        this.child(Label::new(path).color(Color::Muted).size(LabelSize::Small))
                    })
                    .when_some(repo_name.clone(), |this, name| {
                        this.child(Label::new(name).color(Color::Muted).size(LabelSize::Small))
                    })
                    .into_any_element()
            }
        }))))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        if let LogSource::Path(path) = &self.log_source {
            return path
                .as_ref()
                .file_name()
                .map(|name| SharedString::from(name.to_string()))
                .unwrap_or_else(|| SharedString::from(path.as_unix_str().to_string()));
        }

        self.get_repository(cx)
            .and_then(|repo| {
                repo.read(cx)
                    .work_directory_abs_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
            })
            .map_or_else(|| "Git Graph".into(), |name| SharedString::from(name))
    }

    fn show_toolbar(&self) -> bool {
        false
    }

    fn to_item_events(event: &Self::Event, f: &mut dyn FnMut(ItemEvent)) {
        f(*event)
    }
}

impl workspace::SerializableItem for GitGraph {
    fn serialized_item_kind() -> &'static str {
        "GitGraph"
    }

    fn cleanup(
        workspace_id: workspace::WorkspaceId,
        alive_items: Vec<workspace::ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<gpui::Result<()>> {
        workspace::delete_unloaded_items(
            alive_items,
            workspace_id,
            "git_graphs",
            &persistence::GitGraphsDb::global(cx),
            cx,
        )
    }

    fn deserialize(
        project: Entity<project::Project>,
        workspace: WeakEntity<Workspace>,
        workspace_id: workspace::WorkspaceId,
        item_id: workspace::ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<Self>>> {
        let db = persistence::GitGraphsDb::global(cx);
        let Some((
            repo_work_path,
            log_source_type,
            log_source_value,
            log_order,
            selected_sha,
            search_query,
            search_case_sensitive,
        )) = db.get_git_graph(item_id, workspace_id).ok().flatten()
        else {
            return Task::ready(Err(anyhow::anyhow!("No git graph to deserialize")));
        };

        let state = persistence::SerializedGitGraphState {
            log_source_type,
            log_source_value,
            log_order,
            selected_sha,
            search_query,
            search_case_sensitive,
        };

        let window_handle = window.window_handle();
        let project = project.read(cx);
        let git_store = project.git_store().clone();
        let wait = project.wait_for_initial_scan(cx);

        cx.spawn(async move |cx| {
            wait.await;

            cx.update_window(window_handle, |_, window, cx| {
                let path = repo_work_path.as_path();

                let repositories = git_store.read(cx).repositories();
                let repo_id = repositories.iter().find_map(|(&repo_id, repo)| {
                    if repo.read(cx).snapshot().work_directory_abs_path.as_ref() == path {
                        Some(repo_id)
                    } else {
                        None
                    }
                });

                let Some(repo_id) = repo_id else {
                    return Err(anyhow::anyhow!("Repository not found for path: {:?}", path));
                };

                let log_source = persistence::deserialize_log_source(&state);
                let log_order = persistence::deserialize_log_order(&state);

                let git_graph = cx.new(|cx| {
                    let mut graph =
                        GitGraph::new(repo_id, git_store, workspace, Some(log_source), window, cx);
                    graph.log_order = log_order;

                    if let Some(sha) = &state.selected_sha {
                        graph.select_commit_by_sha(sha.as_str(), cx);
                    }

                    graph
                });

                git_graph.update(cx, |graph, cx| {
                    graph.search_state.case_sensitive =
                        state.search_case_sensitive.unwrap_or(false);

                    if let Some(query) = &state.search_query
                        && !query.is_empty()
                    {
                        graph
                            .search_state
                            .editor
                            .update(cx, |editor, cx| editor.set_text(query.as_str(), window, cx));
                        graph.search(query.clone().into(), cx);
                    }
                });

                Ok(git_graph)
            })?
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: workspace::ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<gpui::Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let repo = self.get_repository(cx)?;
        let repo_working_path = repo
            .read(cx)
            .snapshot()
            .work_directory_abs_path
            .to_string_lossy()
            .to_string();

        let selected_sha = self
            .selected_entry_idx
            .and_then(|idx| self.graph_data.commits.get(idx))
            .map(|commit| commit.data.sha.to_string());

        let search_query = self.search_state.editor.read(cx).text(cx);
        let search_query = if search_query.is_empty() {
            None
        } else {
            Some(search_query)
        };

        let log_source_type = Some(persistence::serialize_log_source_type(&self.log_source));
        let log_source_value = persistence::serialize_log_source_value(&self.log_source);
        let log_order = Some(persistence::serialize_log_order(&self.log_order));
        let search_case_sensitive = Some(self.search_state.case_sensitive);

        let db = persistence::GitGraphsDb::global(cx);
        Some(cx.background_spawn(async move {
            db.save_git_graph(
                item_id,
                workspace_id,
                repo_working_path,
                log_source_type,
                log_source_value,
                log_order,
                selected_sha,
                search_query,
                search_case_sensitive,
            )
            .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        match event {
            ItemEvent::UpdateTab | ItemEvent::Edit => true,
            _ => false,
        }
    }
}

mod persistence;
#[cfg(any(test, feature = "test-support"))]
impl GitGraph {
    pub fn search_for_test(&mut self, query: SharedString, cx: &mut Context<Self>) {
        self.search(query, cx);
    }

    pub fn search_matches_for_test(&self) -> Vec<Oid> {
        self.search_state.matches.iter().copied().collect()
    }

    pub fn initial_commit_data_for_test(&self) -> Vec<Arc<InitialGraphCommitData>> {
        self.graph_data
            .commits
            .iter()
            .map(|commit| commit.data.clone())
            .collect()
    }

    pub fn commit_count_and_loading_state_for_test(
        &mut self,
        cx: &mut Context<Self>,
    ) -> (usize, bool) {
        self.commit_count_and_loading_state(cx)
    }

    pub fn log_source_for_test(&self) -> &LogSource {
        &self.log_source
    }
}

/// Generates a random commit DAG suitable for testing git graph rendering.
///
/// The commits are ordered newest-first (like git log output), so:
/// - Index 0 = most recent commit (HEAD)
/// - Last index = oldest commit (root, has no parents)
/// - Parents of commit at index I must have index > I
///
/// When `adversarial` is true, generates complex topologies with many branches
/// and octopus merges. Otherwise generates more realistic linear histories
/// with occasional branches.
#[cfg(any(test, feature = "test-support"))]
pub fn generate_random_commit_dag(
    rng: &mut rand::rngs::StdRng,
    num_commits: usize,
    adversarial: bool,
) -> Vec<Arc<InitialGraphCommitData>> {
    use rand::Rng as _;

    if num_commits == 0 {
        return Vec::new();
    }

    let mut commits: Vec<Arc<InitialGraphCommitData>> = Vec::with_capacity(num_commits);
    let oids: Vec<Oid> = (0..num_commits).map(|_| Oid::random(rng)).collect();

    for i in 0..num_commits {
        let sha = oids[i];

        let parents = if i == num_commits - 1 {
            smallvec![]
        } else {
            generate_parents_from_oids(rng, &oids, i, num_commits, adversarial)
        };

        let ref_names = if i == 0 {
            vec!["HEAD".into(), "main".into()]
        } else if adversarial && rng.random_bool(0.1) {
            vec![format!("branch-{i}").into()]
        } else {
            Vec::new()
        };

        commits.push(Arc::new(InitialGraphCommitData {
            sha,
            parents,
            ref_names,
        }));
    }

    commits
}

#[cfg(any(test, feature = "test-support"))]
fn generate_parents_from_oids(
    rng: &mut rand::rngs::StdRng,
    oids: &[Oid],
    current_idx: usize,
    num_commits: usize,
    adversarial: bool,
) -> SmallVec<[Oid; 1]> {
    use rand::{Rng as _, seq::SliceRandom as _};

    let remaining = num_commits - current_idx - 1;
    if remaining == 0 {
        return smallvec![];
    }

    if adversarial {
        let merge_chance = 0.4;
        let octopus_chance = 0.15;

        if remaining >= 3 && rng.random_bool(octopus_chance) {
            let num_parents = rng.random_range(3..=remaining.min(5));
            let mut parent_indices: Vec<usize> = (current_idx + 1..num_commits).collect();
            parent_indices.shuffle(rng);
            parent_indices
                .into_iter()
                .take(num_parents)
                .map(|idx| oids[idx])
                .collect()
        } else if remaining >= 2 && rng.random_bool(merge_chance) {
            let mut parent_indices: Vec<usize> = (current_idx + 1..num_commits).collect();
            parent_indices.shuffle(rng);
            parent_indices
                .into_iter()
                .take(2)
                .map(|idx| oids[idx])
                .collect()
        } else {
            let parent_idx = rng.random_range(current_idx + 1..num_commits);
            smallvec![oids[parent_idx]]
        }
    } else {
        let merge_chance = 0.15;
        let skip_chance = 0.1;

        if remaining >= 2 && rng.random_bool(merge_chance) {
            let first_parent = current_idx + 1;
            let second_parent = rng.random_range(current_idx + 2..num_commits);
            smallvec![oids[first_parent], oids[second_parent]]
        } else if rng.random_bool(skip_chance) && remaining >= 2 {
            let skip = rng.random_range(1..remaining.min(3));
            smallvec![oids[current_idx + 1 + skip]]
        } else {
            smallvec![oids[current_idx + 1]]
        }
    }
}

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
