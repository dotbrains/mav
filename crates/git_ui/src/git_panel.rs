use crate::askpass_modal::AskPassModal;
use crate::commit_modal::CommitModal;
use crate::commit_tooltip::{CommitAvatar, CommitTooltip};
use crate::commit_view::CommitView;
mod action_menus;
mod commit_actions;
mod commit_message_generation;
mod commit_render;
mod context_menus;
mod debug_output;
mod discard_actions;
mod editor_style;
mod entries;
mod entry_refresh;
mod entry_render;
mod footer_render;
mod history_tab;
mod lifecycle;
mod list_render;
mod message_tooltip;
mod open_actions;
mod output;
mod panel_footer;
mod panel_menus;
mod panel_render;
mod panel_state;
mod panel_traits;
mod remote_operations;
mod render_helpers;
mod repository_actions;
mod selection_navigation;
mod selection_paths;
mod serialization;
mod settings_actions;
mod staging;
mod stash_actions;
mod status_state;

use crate::git_panel_settings::GitPanelScrollbarAccessor;
use crate::project_diff::{BranchDiff, Diff, ProjectDiff};
use crate::remote_output::{self, RemoteAction, SuccessMessage};
use crate::solo_diff_view::SoloDiffView;
use crate::{branch_picker, picker_prompt, render_remote_button};
pub(crate) use editor_style::git_commit_editor_style;
use editor_style::panel_editor_container;
pub use entries::GitStatusEntry;
use entries::*;
use message_tooltip::GitPanelMessageTooltip;
pub(crate) use output::{open_output, show_error_toast};
use panel_footer::PanelRepoFooter;
use panel_menus::{TrashCancel, git_panel_context_menu, git_panel_view_options_menu, prompt};
pub(crate) use panel_traits::{GitPanelAddon, commit_title_exceeds_limit};

use crate::{
    git_panel_settings::GitPanelSettings, git_status_icon, repository_selector::RepositorySelector,
};
use agent_settings::{AgentSettings, UserAgentsMd};
use anyhow::Context as _;
use askpass::AskPassDelegate;
use collections::{BTreeMap, HashMap, HashSet};
use db::kvp::KeyValueStore;
use editor::{Editor, EditorElement, EditorMode, MultiBuffer, MultiBufferOffset, SizingBehavior};
use editor::{EditorStyle, RewrapOptions};
use file_icons::FileIcons;
use futures::StreamExt as _;
use futures::channel::oneshot::Canceled;
use git::Oid;
use git::commit::ParsedCommitMessage;
use git::repository::{
    Branch, CommitData, CommitDetails, CommitOptions, CommitSummary, DiffType, FetchOptions,
    GitCommitTemplate, GitCommitter, LogOrder, LogSource, PushOptions, Remote, RemoteCommandOutput,
    ResetMode, Upstream, UpstreamTracking, UpstreamTrackingStatus, get_git_committer,
};
use git::stash::GitStash;
use git::status::{DiffStat, StageStatus};
use git::{Amend, Commit, Signoff, ToggleStaged, repository::RepoPath, status::FileStatus};
use git::{
    ExpandCommitEditor, GitHostingProviderRegistry, GitRemote, RestoreTrackedFiles, StageAll,
    StashAll, StashApply, StashPop, ToggleFillCommitEditor, TrashUntrackedFiles, UnstageAll,
    ViewFile, parse_git_remote_url,
};
use gpui::{
    AbsoluteLength, Action, Anchor, AsyncApp, AsyncWindowContext, Bounds, ClickEvent, DismissEvent,
    Empty, Entity, EventEmitter, FocusHandle, Focusable, KeyContext, MouseButton, MouseDownEvent,
    Point, PromptLevel, ScrollStrategy, Subscription, Task, TaskExt, TextStyle,
    UniformListScrollHandle, WeakEntity, actions, anchored, deferred, point, size, uniform_list,
};
use itertools::Itertools;
use language::{Buffer, BufferEvent, File};
use language_model::{
    CompletionIntent, ConfiguredModel, Event as LanguageModelEvent, LanguageModelRegistry,
    LanguageModelRequest, LanguageModelRequestMessage, Role,
};
use mav_actions::{DecreaseBufferFontSize, IncreaseBufferFontSize, ResetBufferFontSize};
use menu;
use multi_buffer::ExcerptBoundaryInfo;
use notifications::status_toast::StatusToast;
use panel::PanelHeader;
use project::git_store::GitAccess;
use project::{
    Fs, Project, ProjectPath,
    git_store::{
        CommitDataState, GitStoreEvent, Repository, RepositoryEvent, RepositoryId, pending_op,
    },
    project_settings::{GitPathStyle, ProjectSettings},
};
use prompt_store::RULES_FILE_NAMES;
use proto::RpcError;
use serde::{Deserialize, Serialize};
use settings::{
    GitPanelClickBehavior, GitPanelGroupBy, GitPanelSortBy, Settings, SettingsStore, StatusStyle,
    update_settings_file,
};
use smallvec::SmallVec;
use std::cell::Cell;
use std::future::Future;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::{sync::Arc, time::Duration, usize};
use strum::{IntoEnumIterator, VariantNames};
use theme_settings::ThemeSettings;
use time::OffsetDateTime;
use ui::{
    ButtonLike, Checkbox, ContextMenu, ContextMenuEntry, Divider, ElevationIndex,
    IndentGuideColors, KeyBinding, PopoverMenu, ProjectEmptyState, RenderedIndentGuide, ScrollAxes,
    Scrollbars, SplitButton, Tab, TintColor, Tooltip, WithScrollbar, prelude::*,
};
use util::paths::PathStyle;
use util::{ResultExt, TryFutureExt, markdown::MarkdownInlineCode, maybe, rel_path::RelPath};
use workspace::SERIALIZATION_THROTTLE_TIME;
use workspace::{
    Item, Workspace,
    dock::{DockPosition, Panel, PanelEvent},
    notifications::{DetachAndPromptErr, NotificationId, NotifyTaskExt},
};

const GIT_PANEL_KEY: &str = "GitPanel";
const UPDATE_DEBOUNCE: Duration = Duration::from_millis(50);
// TODO: We should revise this part. It seems the indentation width is not aligned with the one in project panel
const TREE_INDENT: f32 = 16.0;

actions!(
    git_panel,
    [
        /// Closes the git panel.
        Close,
        /// Toggles the git panel.
        Toggle,
        /// Toggles focus on the git panel.
        ToggleFocus,
        /// Opens the git panel menu.
        OpenMenu,
        /// Focuses on the commit message editor.
        FocusEditor,
        /// Focuses on the changes list.
        FocusChanges,
        /// Select next git panel menu item, and show it in the diff view
        NextEntry,
        /// Select previous git panel menu item, and show it in the diff view
        PreviousEntry,
        /// Select first git panel menu item, and show it in the diff view
        FirstEntry,
        /// Select last git panel menu item, and show it in the diff view
        LastEntry,
        /// Toggles automatic co-author suggestions.
        ToggleFillCoAuthors,
        /// Sorts entries by path.
        SetSortByPath,
        /// Sorts entries by name.
        SetSortByName,
        /// Disables grouping entries by status.
        SetGroupByNone,
        /// Groups entries by status.
        SetGroupByStatus,
        /// Toggles showing entries in tree vs flat view.
        ToggleTreeView,
        /// Expands the selected entry to show its children.
        ExpandSelectedEntry,
        /// Collapses the selected entry to hide its children.
        CollapseSelectedEntry,
        /// Activates the Changes tab.
        ActivateChangesTab,
        /// Activates the History tab.
        ActivateHistoryTab,
    ]
);

actions!(
    dev,
    [
        /// Shows the current git job queue debug state for the active repository.
        ShowGitJobQueue,
    ]
);

// We only allow a single remote operation at a time to avoid concurrent
// credential prompts and competing ref/working-tree updates.
#[derive(Clone, Copy)]
pub(crate) enum RemoteOperationKind {
    Fetch,
    Pull,
    Push,
}

pub fn register(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
        workspace.toggle_panel_focus::<GitPanel>(window, cx);
    });
    workspace.register_action(|workspace, _: &Toggle, window, cx| {
        if !workspace.toggle_panel_focus::<GitPanel>(window, cx) {
            workspace.close_panel::<GitPanel>(window, cx);
        }
    });
    workspace.register_action(|workspace, _: &ExpandCommitEditor, window, cx| {
        CommitModal::toggle(workspace, None, window, cx)
    });
    workspace.register_action(|workspace, _: &ToggleFillCommitEditor, window, cx| {
        if let Some(panel) = workspace.panel::<GitPanel>(cx) {
            panel.update(cx, |panel, cx| {
                panel.toggle_fill_commit_editor(&Default::default(), window, cx)
            });
        }
    });
    workspace.register_action(|workspace, _: &git::Init, window, cx| {
        if let Some(panel) = workspace.panel::<GitPanel>(cx) {
            panel.update(cx, |panel, cx| panel.git_init(window, cx));
        }
    });
    workspace.register_action(|workspace, _: &ShowGitJobQueue, window, cx| {
        if let Some(panel) = workspace.panel::<GitPanel>(cx) {
            panel.update(cx, |panel, cx| {
                panel.show_git_job_queue(window, cx);
            });
        }
    });
}

#[derive(Debug, Clone)]
pub enum Event {
    Focus,
}

pub struct GitPanel {
    pub(crate) active_repository: Option<Entity<Repository>>,
    pub(crate) commit_editor: Entity<Editor>,
    /// Whether the commit editor should fill the vertical height of the panel.
    commit_editor_expanded: bool,
    conflicted_count: usize,
    conflicted_staged_count: usize,
    add_coauthors: bool,
    generate_commit_message_task: Option<Task<Option<()>>>,
    entries: Vec<GitListEntry>,
    view_mode: GitPanelViewMode,
    tree_expanded_dirs: HashMap<RepoPath, bool>,
    entries_indices: HashMap<RepoPath, usize>,
    single_staged_entry: Option<GitStatusEntry>,
    single_tracked_entry: Option<GitStatusEntry>,
    focus_handle: FocusHandle,
    fs: Arc<dyn Fs>,
    new_count: usize,
    entry_count: usize,
    changes_count: usize,
    diff_stat_total: DiffStat,
    new_staged_count: usize,
    pending_commit: Option<Task<()>>,
    pending_remote_operation: Option<RemoteOperationKind>,
    amend_pending: bool,
    original_commit_message: Option<String>,
    pending_commit_message_restores: BTreeMap<String, SerializedCommitMessage>,
    signoff_enabled: bool,
    pending_serialization: Task<()>,
    pub(crate) project: Entity<Project>,
    scroll_handle: UniformListScrollHandle,
    max_width_item_index: Option<usize>,
    selected_entry: Option<usize>,
    marked_entries: Vec<usize>,
    tracked_count: usize,
    tracked_staged_count: usize,
    update_visible_entries_task: Task<()>,
    reopen_commit_buffer_task: Task<()>,
    pub(crate) workspace: WeakEntity<Workspace>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    modal_open: bool,
    show_placeholders: bool,
    // Only read to compute collaborative co-authors, which requires the `call` feature.
    #[cfg_attr(not(feature = "call"), allow(dead_code))]
    local_committer: Option<GitCommitter>,
    local_committer_task: Option<Task<()>>,
    commit_template: Option<GitCommitTemplate>,
    bulk_staging: Option<BulkStaging>,
    stash_entries: GitStash,
    active_tab: GitPanelTab,
    commit_history_scroll_handle: UniformListScrollHandle,
    commit_history_shas: Option<Vec<Oid>>,
    focused_history_entry: Option<usize>,
    history_keyboard_nav: bool,
    _commit_message_buffer_subscription: Option<Subscription>,
    _repo_subscriptions: Vec<Subscription>,

    _settings_subscription: Subscription,
    git_access: GitAccess,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BulkStaging {
    repo_id: RepositoryId,
    anchor: RepoPath,
}

const MAX_PANEL_EDITOR_LINES: usize = 6;

pub(crate) fn commit_message_editor(
    commit_message_buffer: Entity<Buffer>,
    placeholder: Option<SharedString>,
    project: Entity<Project>,
    in_panel: bool,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Editor {
    let buffer = cx.new(|cx| MultiBuffer::singleton(commit_message_buffer, cx));
    let max_lines = if in_panel { MAX_PANEL_EDITOR_LINES } else { 18 };
    let mut commit_editor = Editor::new(
        EditorMode::AutoHeight {
            min_lines: max_lines,
            max_lines: Some(max_lines),
        },
        buffer,
        None,
        window,
        cx,
    );
    commit_editor.set_collaboration_hub(Box::new(project));
    commit_editor.set_use_autoclose(false);
    commit_editor.set_show_gutter(false, cx);
    commit_editor.set_use_modal_editing(true);
    commit_editor.set_show_wrap_guides(false, cx);
    commit_editor.set_show_indent_guides(false, cx);
    let placeholder = placeholder.unwrap_or("Enter commit message".into());
    commit_editor.set_placeholder_text(&placeholder, window, cx);
    commit_editor
}

impl GitPanel {
    pub fn selected_file_history_target(&self) -> Option<(Entity<Repository>, RepoPath)> {
        let entry = self.get_selected_entry()?.status_entry()?;
        let repository = self.active_repository.clone()?;
        if entry.status.is_created() {
            return None;
        }
        Some((repository, entry.repo_path.clone()))
    }
}

#[cfg(any(test, feature = "test-support"))]
impl GitPanel {
    pub fn new_test(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        Self::new(workspace, window, cx)
    }

    pub fn active_repository(&self) -> Option<&Entity<Repository>> {
        self.active_repository.as_ref()
    }
}

#[cfg(test)]
mod tests;
