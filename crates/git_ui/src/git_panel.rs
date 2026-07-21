use crate::askpass_modal::AskPassModal;
use crate::commit_modal::CommitModal;
use crate::commit_tooltip::{CommitAvatar, CommitTooltip};
use crate::commit_view::CommitView;
mod action_menus;
mod commit_actions;
mod commit_message_generation;
mod commit_render;
mod debug_output;
mod discard_actions;
mod editor_style;
mod entries;
mod entry_refresh;
mod footer_render;
mod history_tab;
mod lifecycle;
mod message_tooltip;
mod open_actions;
mod output;
mod panel_footer;
mod panel_menus;
mod remote_operations;
mod render_helpers;
mod repository_actions;
mod selection_navigation;
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
    pub fn entry_by_path(&self, path: &RepoPath) -> Option<usize> {
        self.entries_indices.get(path).copied()
    }

    pub fn select_entry_by_path(
        &mut self,
        path: ProjectPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(git_repo) = self.active_repository.as_ref() else {
            return;
        };

        let (repo_path, section) = {
            let repo = git_repo.read(cx);
            let Some(repo_path) = repo.project_path_to_repo_path(&path, cx) else {
                return;
            };

            let section = repo
                .status_for_path(&repo_path)
                .map(|status| status.status)
                .map(|status| {
                    if repo.had_conflict_on_last_merge_head_change(&repo_path) {
                        Section::Conflict
                    } else if status.is_created() {
                        Section::New
                    } else {
                        Section::Tracked
                    }
                });

            (repo_path, section)
        };

        let mut needs_rebuild = false;
        if let (Some(section), Some(tree_state)) = (section, self.view_mode.tree_state_mut()) {
            let mut current_dir = repo_path.parent();
            while let Some(dir) = current_dir {
                let key = TreeKey {
                    section,
                    path: RepoPath::from_rel_path(dir),
                };

                if tree_state.expanded_dirs.get(&key.path) == Some(&false) {
                    tree_state.expanded_dirs.insert(key.path.clone(), true);
                    needs_rebuild = true;
                }

                current_dir = dir.parent();
            }
        }

        if needs_rebuild {
            self.update_visible_entries(window, cx);
        }

        let Some(ix) = self.entry_by_path(&repo_path) else {
            return;
        };

        self.selected_entry = Some(ix);
        self.scroll_to_selected_entry(cx);
    }

    pub(crate) fn render_remote_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let branch = self.active_repository.as_ref()?.read(cx).branch.clone();
        if !self.can_push_and_pull(cx) {
            return None;
        }
        Some(
            h_flex()
                .gap_1()
                .flex_shrink_0()
                .when_some(branch, |this, branch| {
                    let focus_handle = Some(self.focus_handle(cx));

                    this.children(render_remote_button(
                        "remote-button",
                        &branch,
                        focus_handle,
                        true,
                        self.pending_remote_operation,
                    ))
                })
                .into_any_element(),
        )
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab = self.active_tab;

        let focus_handle = self.focus_handle.clone();
        let tab = |id: ElementId,
                   active: bool,
                   show_changes: bool,
                   label: SharedString,
                   set_active_tab: GitPanelTab,
                   tooltip_action: Box<dyn Action>| {
            let focus_handle = focus_handle.clone();

            h_flex()
                .cursor_pointer()
                .id(id)
                .h_full()
                .py_1()
                .gap_1()
                .flex_1()
                .justify_center()
                .hover(|s| s.bg(cx.theme().colors().element_hover))
                .border_b_1()
                .when(!active, |s| {
                    s.bg(cx.theme().colors().editor_background.opacity(0.6))
                        .border_color(cx.theme().colors().border.opacity(0.6))
                })
                .child(Label::new(label.clone()).when(!active, |this| this.color(Color::Muted)))
                .when(show_changes && self.changes_count > 0, |this| {
                    this.child(
                        Label::new(format!("({})", self.changes_count))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                })
                .tooltip(Tooltip::for_action_title_in(
                    format!("Toggle {} Tab", label),
                    tooltip_action.as_ref(),
                    &focus_handle,
                ))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_active_tab(set_active_tab, window, cx)
                }))
        };

        h_flex()
            .relative()
            .h(Tab::container_height(cx))
            .w_full()
            .child(tab(
                ElementId::Name("changes-tab".into()),
                active_tab == GitPanelTab::Changes,
                true,
                "Changes".into(),
                GitPanelTab::Changes,
                ActivateChangesTab.boxed_clone(),
            ))
            .child(Divider::vertical().color(ui::DividerColor::BorderFaded))
            .child(tab(
                ElementId::Name("history-tab".into()),
                active_tab != GitPanelTab::Changes,
                false,
                "History".into(),
                GitPanelTab::History,
                ActivateHistoryTab.boxed_clone(),
            ))
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let content = match (self.git_access, &self.active_repository) {
            (GitAccess::No, Some(repository)) => self.render_unsafe_repo_ui(repository, cx),
            (_, None) => self.render_uninitialized_ui(cx),
            (_, Some(_)) => self.render_no_changes_ui(cx),
        };

        v_flex()
            .gap_1p5()
            .flex_1()
            .items_center()
            .justify_center()
            .child(content)
    }

    fn render_no_changes_ui(&self, cx: &Context<Self>) -> AnyElement {
        let show_branch_diff = self.changes_count == 0 && !self.is_on_main_branch(cx);

        v_flex()
            .gap_1()
            .items_center()
            .child(Label::new("No changes to commit").color(Color::Muted))
            .when(show_branch_diff, |this| {
                this.child(
                    Button::new("view_branch_diff", "View Branch Diff")
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::Outlined)
                        .on_click(move |_, _, cx| {
                            cx.defer(move |cx| {
                                cx.dispatch_action(&BranchDiff);
                            })
                        }),
                )
            })
            .into_any_element()
    }

    fn render_unsafe_repo_ui(
        &self,
        active_repository: &Entity<Repository>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let directory = active_repository.update(cx, |repository, _cx| {
            repository.snapshot().work_directory_abs_path
        });

        let message = format!(
            "Detected dubious ownership in repository at {}. \
            This happens when the .git/ directory is not owned by the current user. \
            If you want to learn more about safe directories, visit git's documentation.",
            directory.display()
        );

        v_flex()
                .px_4()
                .gap_1()
                .child(Label::new(message).color(Color::Muted))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .child(
                            Button::new("trust_directory", "Trust Directory")
                            .label_size(LabelSize::Small)
                            .layer(ElevationIndex::ModalSurface)
                            .style(ButtonStyle::Filled)
                            .tooltip(Tooltip::text(
                                format!("git config --global --add safe.directory {}", directory.display())
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.add_safe_directory(window, cx);
                                })
                            )
                    )
                    .child(
                        Button::new("learn_more", "Learn More")
                            .label_size(LabelSize::Small)
                            .style(ButtonStyle::Outlined)
                            .end_icon(Icon::new(IconName::ArrowUpRight).size(IconSize::Small).color(Color::Muted))
                            .on_click(move |_, _, cx| cx.open_url("https://git-scm.com/docs/git-config#Documentation/git-config.txt-safedirectory"))
                    )
                )
                .into_any_element()
    }

    fn render_uninitialized_ui(&self, cx: &mut Context<Self>) -> AnyElement {
        let worktree_count = self.project.read(cx).visible_worktrees(cx).count();
        if worktree_count > 0 && self.active_repository.is_none() {
            v_flex()
                .gap_1()
                .items_center()
                .child(Label::new("No Git Repositories").color(Color::Muted))
                .child(
                    Button::new("initialize_repository", "Initialize Repository")
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::Outlined)
                        .tooltip(Tooltip::for_action_title_in(
                            "git init",
                            &git::Init,
                            &self.focus_handle,
                        ))
                        .on_click(move |_, _, cx| {
                            cx.defer(move |cx| {
                                cx.dispatch_action(&git::Init);
                            })
                        }),
                )
                .into_any_element()
        } else if worktree_count == 0 {
            let focus_handle = self.focus_handle.clone();
            ProjectEmptyState::new(
                "Git Panel",
                focus_handle.clone(),
                KeyBinding::for_action_in(&workspace::Open::default(), &focus_handle, cx),
            )
            .on_open_project(|_, window, cx| {
                telemetry::event!("Git Panel Add Project Clicked");
                window.dispatch_action(workspace::Open::default().boxed_clone(), cx);
            })
            .on_clone_repo(|_, window, cx| {
                telemetry::event!("Git Panel Clone Repo Clicked");
                window.dispatch_action(git::Clone.boxed_clone(), cx);
            })
            .into_any_element()
        } else {
            Empty.into_any_element()
        }
    }

    fn is_on_main_branch(&self, cx: &Context<Self>) -> bool {
        let Some(repo) = self.active_repository.as_ref() else {
            return false;
        };

        let Some(branch) = repo.read(cx).branch.as_ref() else {
            return false;
        };

        let branch_name = branch.name();
        matches!(branch_name, "main" | "master")
    }

    fn render_buffer_header_controls(
        &self,
        entity: &Entity<Self>,
        file: &Arc<dyn File>,
        _: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let repo = self.active_repository.as_ref()?.read(cx);
        let project_path = (file.worktree_id(cx), file.path().clone()).into();
        let repo_path = repo.project_path_to_repo_path(&project_path, cx)?;
        let ix = self.entry_by_path(&repo_path)?;
        let entry = self.entries.get(ix)?;

        let is_staging_or_staged = repo
            .pending_ops_for_path(&repo_path)
            .map(|ops| !ops.last_op_errored() && (ops.staging() || ops.staged()))
            .or_else(|| {
                repo.status_for_path(&repo_path)
                    .and_then(|status| status.status.staging().as_bool())
            })
            .or_else(|| {
                entry
                    .status_entry()
                    .and_then(|entry| entry.staging.as_bool())
            });

        let checkbox = Checkbox::new("stage-file", is_staging_or_staged.into())
            .disabled(!self.has_write_access(cx))
            .fill()
            .elevation(ElevationIndex::Surface)
            .on_click({
                let entry = entry.clone();
                let git_panel = entity.downgrade();
                move |_, window, cx| {
                    git_panel
                        .update(cx, |this, cx| {
                            this.toggle_staged_for_entry(&entry, window, cx);
                            cx.stop_propagation();
                        })
                        .ok();
                }
            });
        Some(
            h_flex()
                .id("start-slot")
                .text_lg()
                .child(checkbox)
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    // prevent the list item active state triggering when toggling checkbox
                    cx.stop_propagation();
                })
                .into_any_element(),
        )
    }

    fn render_entries(
        &self,
        has_write_access: bool,
        repo: Entity<Repository>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (is_tree_view, entry_count) = match &self.view_mode {
            GitPanelViewMode::Tree(state) => (true, state.logical_indices.len()),
            GitPanelViewMode::Flat => (false, self.entries.len()),
        };
        let repo = repo.downgrade();

        v_flex()
            .flex_1()
            .size_full()
            .overflow_hidden()
            .relative()
            .child(
                h_flex()
                    .flex_1()
                    .size_full()
                    .relative()
                    .overflow_hidden()
                    .child(
                        uniform_list(
                            "entries",
                            entry_count,
                            cx.processor(move |this, range: Range<usize>, window, cx| {
                                let Some(repo) = repo.upgrade() else {
                                    return Vec::new();
                                };
                                let repo = repo.read(cx);

                                let mut items = Vec::with_capacity(range.end - range.start);

                                for ix in range.into_iter().map(|ix| match &this.view_mode {
                                    GitPanelViewMode::Tree(state) => state.logical_indices[ix],
                                    GitPanelViewMode::Flat => ix,
                                }) {
                                    match &this.entries.get(ix) {
                                        Some(GitListEntry::Status(entry)) => {
                                            items.push(this.render_status_entry(
                                                ix,
                                                entry,
                                                0,
                                                has_write_access,
                                                repo,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::TreeStatus(entry)) => {
                                            items.push(this.render_status_entry(
                                                ix,
                                                &entry.entry,
                                                entry.depth,
                                                has_write_access,
                                                repo,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::Directory(entry)) => {
                                            items.push(this.render_directory_entry(
                                                ix,
                                                entry,
                                                has_write_access,
                                                window,
                                                cx,
                                            ));
                                        }
                                        Some(GitListEntry::Header(header)) => {
                                            items.push(this.render_list_header(
                                                ix,
                                                header,
                                                has_write_access,
                                                window,
                                                cx,
                                            ));
                                        }
                                        None => {}
                                    }
                                }

                                items
                            }),
                        )
                        .when(is_tree_view, |list| {
                            let indent_size = px(TREE_INDENT);
                            list.with_decoration(
                                ui::indent_guides(indent_size, IndentGuideColors::panel(cx))
                                    .with_compute_indents_fn(
                                        cx.entity(),
                                        |this, range, _window, _cx| {
                                            this.compute_visible_depths(range)
                                        },
                                    )
                                    .with_render_fn(cx.entity(), |_, params, _, _| {
                                        // Magic number to align the tree item is 3 here
                                        // because we're using 12px as the left-side padding
                                        // and 3 makes the alignment work with the bounding box of the icon
                                        let left_offset = px(TREE_INDENT + 3_f32);
                                        let indent_size = params.indent_size;
                                        let item_height = params.item_height;

                                        params
                                            .indent_guides
                                            .into_iter()
                                            .map(|layout| {
                                                let bounds = Bounds::new(
                                                    point(
                                                        layout.offset.x * indent_size + left_offset,
                                                        layout.offset.y * item_height,
                                                    ),
                                                    size(px(1.), layout.length * item_height),
                                                );
                                                RenderedIndentGuide {
                                                    bounds,
                                                    layout,
                                                    is_active: false,
                                                    hitbox: None,
                                                }
                                            })
                                            .collect()
                                    }),
                            )
                        })
                        .group("entries")
                        .size_full()
                        .flex_grow_1()
                        .with_width_from_item(self.max_width_item_index)
                        .track_scroll(&self.scroll_handle),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            this.deploy_panel_context_menu(event.position, window, cx)
                        }),
                    )
                    .custom_scrollbars(
                        Scrollbars::for_settings::<GitPanelScrollbarAccessor>()
                            .tracked_scroll_handle(&self.scroll_handle)
                            .with_track_along(
                                ScrollAxes::Horizontal,
                                cx.theme().colors().editor_background,
                            ),
                        window,
                        cx,
                    ),
            )
    }

    fn entry_label(&self, label: impl Into<SharedString>, color: Color) -> Label {
        Label::new(label.into()).color(color)
    }

    fn list_item_height(&self) -> Rems {
        rems(1.75)
    }

    fn render_list_header(
        &self,
        ix: usize,
        header: &GitHeaderEntry,
        has_write_access: bool,
        _window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id: ElementId = ElementId::Name(format!("header_{}", ix).into());
        let checkbox_id: ElementId = ElementId::Name(format!("header_{}_checkbox", ix).into());
        let group_name: SharedString = format!("header_{}", ix).into();
        let toggle_state = self.header_state(header.header);
        let section = header.header;
        let weak = cx.weak_entity();

        h_flex()
            .id(id)
            .cursor_pointer()
            .group(group_name)
            .h(self.list_item_height())
            .w_full()
            .pl_3()
            .pr_1()
            .gap_2()
            .justify_between()
            .hover(|s| s.bg(cx.theme().colors().ghost_element_hover))
            .border_1()
            .border_r_2()
            .child(
                Label::new(header.title())
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            )
            .child(
                Checkbox::new(checkbox_id, toggle_state)
                    .disabled(!has_write_access)
                    .fill()
                    .elevation(ElevationIndex::Surface),
            )
            .on_click(move |_, window, cx| {
                if !has_write_access {
                    return;
                }

                weak.update(cx, |this, cx| {
                    this.toggle_staged_for_entry(
                        &GitListEntry::Header(GitHeaderEntry { header: section }),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
                .ok();
            })
            .into_any_element()
    }

    pub fn load_commit_details(
        &self,
        sha: String,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<CommitDetails>> {
        let Some(repo) = self.active_repository.clone() else {
            return Task::ready(Err(anyhow::anyhow!("no active repo")));
        };
        repo.update(cx, |repo, cx| {
            let show = repo.show(sha);
            cx.spawn(async move |_, _| show.await?)
        })
    }

    fn deploy_entry_context_menu(
        &mut self,
        position: Point<Pixels>,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.entries.get(ix).and_then(|e| e.status_entry()) else {
            return;
        };
        let stage_title = if entry.status.staging().is_fully_staged() {
            "Unstage File"
        } else {
            "Stage File"
        };
        let restore_title = if entry.status.is_created() {
            "Trash File"
        } else {
            "Discard Changes"
        };
        let context_menu = ContextMenu::build(window, cx, |context_menu, _, _| {
            let is_created = entry.status.is_created();
            context_menu
                .context(self.focus_handle.clone())
                .action(stage_title, ToggleStaged.boxed_clone())
                .action(restore_title, git::RestoreFile::default().boxed_clone())
                .separator()
                .action_disabled_when(
                    !is_created,
                    "Add to .gitignore",
                    git::AddToGitignore.boxed_clone(),
                )
                .action_disabled_when(
                    !is_created,
                    "Add to .git/info/exclude",
                    git::AddToGitInfoExclude.boxed_clone(),
                )
                .separator()
                .action("Open Diff", menu::Confirm.boxed_clone())
                .action("Open Diff (File)", menu::SecondaryConfirm.boxed_clone())
                .action("View File", ViewFile.boxed_clone())
                .when(!is_created, |context_menu| {
                    context_menu
                        .separator()
                        .action("View File History", Box::new(git::FileHistory))
                })
        });
        self.selected_entry = Some(ix);
        self.set_context_menu(context_menu, position, window, cx);
    }

    fn deploy_panel_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let has_tracked_changes = self.has_tracked_changes();
        let has_staged_changes = self.has_staged_changes();
        let has_unstaged_changes = self.has_unstaged_changes();
        let has_new_changes = self.new_count > 0;
        let has_stash_items = self.stash_entries.entries.len() > 0;

        let context_menu = git_panel_context_menu(
            has_tracked_changes,
            has_staged_changes,
            has_unstaged_changes,
            has_new_changes,
            has_stash_items,
            self.focus_handle.clone(),
            window,
            cx,
        );
        self.set_context_menu(context_menu, position, window, cx);
    }

    fn set_context_menu(
        &mut self,
        context_menu: Entity<ContextMenu>,
        position: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(
            &context_menu,
            window,
            |this, _, _: &DismissEvent, window, cx| {
                if this.context_menu.as_ref().is_some_and(|context_menu| {
                    context_menu.0.focus_handle(cx).contains_focused(window, cx)
                }) {
                    cx.focus_self(window);
                }
                this.context_menu.take();
                cx.notify();
            },
        );
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn render_status_entry(
        &self,
        ix: usize,
        entry: &GitStatusEntry,
        depth: usize,
        has_write_access: bool,
        repo: &Repository,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let settings = GitPanelSettings::get_global(cx);
        let tree_view = settings.tree_view;
        let path_style = self.project.read(cx).path_style(cx);
        let git_path_style = ProjectSettings::get_global(cx).git.path_style;
        let display_name = entry.display_name(path_style);

        let selected = self.selected_entry == Some(ix);
        let marked = self.marked_entries.contains(&ix);
        let status_style = settings.status_style;
        let status = entry.status;
        let file_icon = if settings.file_icons {
            FileIcons::get_icon(entry.repo_path.as_std_path(), cx)
        } else {
            None
        };

        let has_conflict = status.is_conflicted();
        let is_modified = status.is_modified();
        let is_deleted = status.is_deleted();
        let is_created = status.is_created();

        let label_color = if status_style == StatusStyle::LabelColor {
            if has_conflict {
                Color::VersionControlConflict
            } else if is_created {
                Color::VersionControlAdded
            } else if is_modified {
                Color::VersionControlModified
            } else if is_deleted {
                // We don't want a bunch of red labels in the list
                Color::Disabled
            } else {
                Color::VersionControlAdded
            }
        } else {
            Color::Default
        };

        let path_color = if status.is_deleted() {
            Color::Disabled
        } else {
            Color::Muted
        };

        let id: ElementId = ElementId::Name(format!("entry_{}_{}", display_name, ix).into());
        let checkbox_wrapper_id: ElementId =
            ElementId::Name(format!("entry_{}_{}_checkbox_wrapper", display_name, ix).into());
        let checkbox_id: ElementId =
            ElementId::Name(format!("entry_{}_{}_checkbox", display_name, ix).into());

        let stage_status = GitPanel::stage_status_for_entry(entry, &repo);
        let mut is_staged: ToggleState = match stage_status {
            StageStatus::Staged => ToggleState::Selected,
            StageStatus::Unstaged => ToggleState::Unselected,
            StageStatus::PartiallyStaged => ToggleState::Indeterminate,
        };
        if self.show_placeholders && !self.has_staged_changes() && !entry.status.is_created() {
            is_staged = ToggleState::Selected;
        }

        let handle = cx.weak_entity();

        let selected_bg_alpha = 0.08;
        let marked_bg_alpha = 0.12;
        let state_opacity_step = 0.04;

        let info_color = cx.theme().status().info;

        let base_bg = match (selected, marked) {
            (true, true) => info_color.alpha(selected_bg_alpha + marked_bg_alpha),
            (true, false) => info_color.alpha(selected_bg_alpha),
            (false, true) => info_color.alpha(marked_bg_alpha),
            _ => cx.theme().colors().ghost_element_background,
        };

        let (hover_bg, active_bg) = if selected {
            (
                info_color.alpha(selected_bg_alpha + state_opacity_step),
                info_color.alpha(selected_bg_alpha + state_opacity_step * 2.0),
            )
        } else {
            (
                cx.theme().colors().ghost_element_hover,
                cx.theme().colors().ghost_element_active,
            )
        };

        let name_row = h_flex()
            .min_w_0()
            .flex_1()
            .gap_1()
            .when(settings.file_icons, |this| {
                this.child(
                    file_icon
                        .map(|file_icon| {
                            Icon::from_path(file_icon)
                                .size(IconSize::Small)
                                .color(Color::Muted)
                        })
                        .unwrap_or_else(|| {
                            Icon::new(IconName::File)
                                .size(IconSize::Small)
                                .color(Color::Muted)
                        }),
                )
            })
            .when(status_style != StatusStyle::LabelColor, |el| {
                el.child(git_status_icon(status))
            })
            .map(|this| {
                if tree_view {
                    this.pl(px(depth as f32 * TREE_INDENT)).child(
                        self.entry_label(display_name, label_color)
                            .when(status.is_deleted(), Label::strikethrough)
                            .truncate(),
                    )
                } else {
                    this.child(self.path_formatted(
                        entry.parent_dir(path_style),
                        path_color,
                        display_name,
                        label_color,
                        path_style,
                        git_path_style,
                        status.is_deleted(),
                    ))
                }
            });

        let id_for_diff_stat = id.clone();

        h_flex()
            .id(id)
            .h(self.list_item_height())
            .w_full()
            .pl_3()
            .pr_1()
            .gap_1p5()
            .border_1()
            .border_r_2()
            .when(selected && self.focus_handle.is_focused(window), |el| {
                el.border_color(cx.theme().colors().panel_focused_border)
            })
            .bg(base_bg)
            .hover(|s| s.bg(hover_bg))
            .active(|s| s.bg(active_bg))
            .child(name_row)
            .when(GitPanelSettings::get_global(cx).diff_stats, |el| {
                el.when_some(entry.diff_stat, move |this, stat| {
                    let id = format!("diff-stat-{}", id_for_diff_stat);
                    this.child(ui::DiffStat::new(
                        id,
                        stat.added as usize,
                        stat.deleted as usize,
                    ))
                })
            })
            .child(
                div()
                    .id(checkbox_wrapper_id)
                    .flex_none()
                    .occlude()
                    .cursor_pointer()
                    .child(
                        Checkbox::new(checkbox_id, is_staged)
                            .disabled(!has_write_access)
                            .fill()
                            .elevation(ElevationIndex::Surface)
                            .on_click_ext({
                                let entry = entry.clone();
                                let this = cx.weak_entity();
                                move |_, click, window, cx| {
                                    this.update(cx, |this, cx| {
                                        if !has_write_access {
                                            return;
                                        }
                                        if click.modifiers().shift {
                                            this.stage_bulk(ix, cx);
                                        } else {
                                            let list_entry =
                                                if GitPanelSettings::get_global(cx).tree_view {
                                                    GitListEntry::TreeStatus(GitTreeStatusEntry {
                                                        entry: entry.clone(),
                                                        depth,
                                                    })
                                                } else {
                                                    GitListEntry::Status(entry.clone())
                                                };
                                            this.toggle_staged_for_entry(&list_entry, window, cx);
                                        }
                                        cx.stop_propagation();
                                    })
                                    .ok();
                                }
                            })
                            .tooltip(move |_window, cx| {
                                let action = match stage_status {
                                    StageStatus::Staged => "Unstage",
                                    StageStatus::Unstaged | StageStatus::PartiallyStaged => "Stage",
                                };
                                let tooltip_name = action.to_string();

                                Tooltip::for_action(tooltip_name, &ToggleStaged, cx)
                            }),
                    ),
            )
            .on_click({
                cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.selected_entry = Some(ix);
                    cx.notify();
                    this.open_selected_entry_on_click(event.modifiers().secondary(), window, cx);
                })
            })
            .on_mouse_down(
                MouseButton::Right,
                move |event: &MouseDownEvent, window, cx| {
                    // why isn't this happening automatically? we are passing MouseButton::Right to `on_mouse_down`?
                    if event.button != MouseButton::Right {
                        return;
                    }

                    let Some(this) = handle.upgrade() else {
                        return;
                    };
                    this.update(cx, |this, cx| {
                        this.deploy_entry_context_menu(event.position, ix, window, cx);
                    });
                    cx.stop_propagation();
                },
            )
            .into_any_element()
    }

    fn render_directory_entry(
        &self,
        ix: usize,
        entry: &GitTreeDirEntry,
        has_write_access: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        // TODO: Have not yet plugged in self.marked_entries. Not sure when and why we need that
        let selected = self.selected_entry == Some(ix);
        let label_color = Color::Muted;

        let id: ElementId = ElementId::Name(format!("dir_{}_{}", entry.name, ix).into());
        let checkbox_id: ElementId =
            ElementId::Name(format!("dir_checkbox_{}_{}", entry.name, ix).into());
        let checkbox_wrapper_id: ElementId =
            ElementId::Name(format!("dir_checkbox_wrapper_{}_{}", entry.name, ix).into());

        let selected_bg_alpha = 0.08;
        let state_opacity_step = 0.04;

        let info_color = cx.theme().status().info;
        let colors = cx.theme().colors();

        let (base_bg, hover_bg, active_bg) = if selected {
            (
                info_color.alpha(selected_bg_alpha),
                info_color.alpha(selected_bg_alpha + state_opacity_step),
                info_color.alpha(selected_bg_alpha + state_opacity_step * 2.0),
            )
        } else {
            (
                colors.ghost_element_background,
                colors.ghost_element_hover,
                colors.ghost_element_active,
            )
        };

        let settings = GitPanelSettings::get_global(cx);
        let folder_icon = if settings.folder_icons {
            FileIcons::get_folder_icon(entry.expanded, entry.key.path.as_std_path(), cx)
        } else {
            FileIcons::get_chevron_icon(entry.expanded, cx)
        };
        let fallback_folder_icon = if settings.folder_icons {
            if entry.expanded {
                IconName::FolderOpen
            } else {
                IconName::Folder
            }
        } else {
            if entry.expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            }
        };

        let stage_status = if let Some(repo) = &self.active_repository {
            self.stage_status_for_directory(entry, repo.read(cx))
        } else {
            util::debug_panic!(
                "Won't have entries to render without an active repository in Git Panel"
            );
            StageStatus::PartiallyStaged
        };

        let toggle_state: ToggleState = match stage_status {
            StageStatus::Staged => ToggleState::Selected,
            StageStatus::Unstaged => ToggleState::Unselected,
            StageStatus::PartiallyStaged => ToggleState::Indeterminate,
        };

        let name_row = h_flex()
            .min_w_0()
            .gap_1()
            .pl(px(entry.depth as f32 * TREE_INDENT))
            .child(
                folder_icon
                    .map(|folder_icon| {
                        Icon::from_path(folder_icon)
                            .size(IconSize::Small)
                            .color(Color::Muted)
                    })
                    .unwrap_or_else(|| {
                        Icon::new(fallback_folder_icon)
                            .size(IconSize::Small)
                            .color(Color::Muted)
                    }),
            )
            .child(self.entry_label(entry.name.clone(), label_color).truncate());

        h_flex()
            .id(id)
            .h(self.list_item_height())
            .min_w_0()
            .w_full()
            .pl_3()
            .pr_1()
            .gap_1p5()
            .justify_between()
            .border_1()
            .border_r_2()
            .when(selected && self.focus_handle.is_focused(window), |el| {
                el.border_color(cx.theme().colors().panel_focused_border)
            })
            .bg(base_bg)
            .hover(|s| s.bg(hover_bg))
            .active(|s| s.bg(active_bg))
            .child(name_row)
            .child(
                div()
                    .id(checkbox_wrapper_id)
                    .flex_none()
                    .occlude()
                    .cursor_pointer()
                    .child(
                        Checkbox::new(checkbox_id, toggle_state)
                            .disabled(!has_write_access)
                            .fill()
                            .elevation(ElevationIndex::Surface)
                            .on_click({
                                let entry = entry.clone();
                                let this = cx.weak_entity();
                                move |_, window, cx| {
                                    this.update(cx, |this, cx| {
                                        if !has_write_access {
                                            return;
                                        }
                                        this.toggle_staged_for_entry(
                                            &GitListEntry::Directory(entry.clone()),
                                            window,
                                            cx,
                                        );
                                        cx.stop_propagation();
                                    })
                                    .ok();
                                }
                            })
                            .tooltip(move |_window, cx| {
                                let action = match stage_status {
                                    StageStatus::Staged => "Unstage",
                                    StageStatus::Unstaged | StageStatus::PartiallyStaged => "Stage",
                                };
                                Tooltip::simple(format!("{action} folder"), cx)
                            }),
                    ),
            )
            .on_click({
                let key = entry.key.clone();
                cx.listener(move |this, _event: &ClickEvent, window, cx| {
                    this.selected_entry = Some(ix);
                    this.toggle_directory(&key, window, cx);
                })
            })
            .into_any_element()
    }

    fn path_formatted(
        &self,
        directory: Option<String>,
        path_color: Color,
        file_name: String,
        label_color: Color,
        path_style: PathStyle,
        git_path_style: GitPathStyle,
        strikethrough: bool,
    ) -> Div {
        let file_name_first = git_path_style == GitPathStyle::FileNameFirst;
        let file_path_first = git_path_style == GitPathStyle::FilePathFirst;

        let file_name = format!("{} ", file_name);

        h_flex()
            .min_w_0()
            .overflow_hidden()
            .when(file_path_first, |this| this.flex_row_reverse())
            .child(
                div().flex_none().child(
                    self.entry_label(file_name, label_color)
                        .when(strikethrough, Label::strikethrough),
                ),
            )
            .when_some(directory, |this, dir| {
                let path_name = if file_name_first {
                    dir
                } else {
                    format!("{dir}{}", path_style.primary_separator())
                };

                this.child(
                    self.entry_label(path_name, path_color)
                        .truncate_start()
                        .when(strikethrough, Label::strikethrough),
                )
            })
    }

    fn has_write_access(&self, cx: &App) -> bool {
        !self.project.read(cx).is_read_only(cx)
    }

    pub fn load_commit_template(
        &self,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Option<GitCommitTemplate>>> {
        let Some(repo) = self.active_repository.clone() else {
            return Task::ready(Err(anyhow::anyhow!("no active repo")));
        };
        repo.update(cx, |repo, cx| {
            let rx = repo.load_commit_template_text();
            cx.spawn(async move |_, _| rx.await?)
        })
    }

    pub fn amend_pending(&self) -> bool {
        self.amend_pending
    }

    /// Sets the pending amend state, ensuring that the original commit message
    /// is either saved, when `value` is `true` and there's no pending amend, or
    /// restored, when `value` is `false` and there's a pending amend.
    pub fn set_amend_pending(&mut self, value: bool, cx: &mut Context<Self>) {
        if value && !self.amend_pending {
            let current_message = self.commit_message_buffer(cx).read(cx).text();
            self.original_commit_message = if current_message.trim().is_empty() {
                None
            } else {
                Some(current_message)
            };
        } else if !value && self.amend_pending {
            let message = self.original_commit_message.take().unwrap_or_default();
            self.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, message)], None, cx);
            });
        }

        self.amend_pending = value;
        self.serialize(cx);
        cx.notify();
    }

    pub fn signoff_enabled(&self) -> bool {
        self.signoff_enabled
    }

    pub fn set_signoff_enabled(&mut self, value: bool, cx: &mut Context<Self>) {
        self.signoff_enabled = value;
        self.serialize(cx);
        cx.notify();
    }

    pub fn toggle_signoff_enabled(
        &mut self,
        _: &Signoff,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_signoff_enabled(!self.signoff_enabled, cx);
    }

    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let serialized_panel = match workspace
            .read_with(&cx, |workspace, cx| {
                Self::serialization_key(workspace).map(|key| (key, KeyValueStore::global(cx)))
            })
            .ok()
            .flatten()
        {
            Some((serialization_key, kvp)) => cx
                .background_spawn(async move { kvp.read_kvp(&serialization_key) })
                .await
                .context("loading git panel")
                .log_err()
                .flatten()
                .map(|panel| serde_json::from_str::<SerializedGitPanel>(&panel))
                .transpose()
                .log_err()
                .flatten(),
            None => None,
        };

        workspace.update_in(&mut cx, |workspace, window, cx| {
            GitPanel::new_with_serialized_panel(workspace, serialized_panel, window, cx)
        })
    }

    fn stage_bulk(&mut self, mut index: usize, cx: &mut Context<'_, Self>) {
        let Some(op) = self.bulk_staging.as_ref() else {
            return;
        };
        let Some(mut anchor_index) = self.entry_by_path(&op.anchor) else {
            return;
        };
        if let Some(entry) = self.entries.get(index)
            && let Some(entry) = entry.status_entry()
        {
            self.set_bulk_staging_anchor(entry.repo_path.clone(), cx);
        }
        if index < anchor_index {
            std::mem::swap(&mut index, &mut anchor_index);
        }
        let entries = self
            .entries
            .get(anchor_index..=index)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| entry.status_entry().cloned())
            .collect::<Vec<_>>();
        self.change_file_stage(true, entries, cx);
    }

    fn set_bulk_staging_anchor(&mut self, path: RepoPath, cx: &mut Context<'_, GitPanel>) {
        let Some(repo) = self.active_repository.as_ref() else {
            return;
        };
        self.bulk_staging = Some(BulkStaging {
            repo_id: repo.read(cx).id,
            anchor: path,
        });
    }

    pub(crate) fn toggle_amend_pending(&mut self, cx: &mut Context<Self>) {
        self.set_amend_pending(!self.amend_pending, cx);
        if self.amend_pending {
            self.load_last_commit_message(cx);
        }
    }
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

impl Render for GitPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let project = self.project.read(cx);
        let has_entries = !self.entries.is_empty();
        let has_write_access = self.has_write_access(cx);

        #[cfg(feature = "call")]
        let has_co_authors = self
            .workspace
            .upgrade()
            .and_then(|_workspace| {
                call::ActiveCall::try_global(cx).and_then(|call| call.read(cx).room().cloned())
            })
            .is_some_and(|room| {
                self.load_local_committer(cx);
                let room = room.read(cx);
                room.remote_participants()
                    .values()
                    .any(|remote_participant| remote_participant.can_write())
            });
        #[cfg(not(feature = "call"))]
        let has_co_authors = false;

        v_flex()
            .id("git_panel")
            .key_context(self.dispatch_context(window, cx))
            .track_focus(&self.focus_handle)
            .when(has_write_access && !project.is_read_only(cx), |this| {
                this.on_action(cx.listener(Self::toggle_staged_for_selected))
                    .on_action(cx.listener(Self::stage_range))
                    .on_action(cx.listener(GitPanel::on_commit))
                    .on_action(cx.listener(GitPanel::on_amend))
                    .on_action(cx.listener(GitPanel::toggle_signoff_enabled))
                    .on_action(cx.listener(Self::stage_all))
                    .on_action(cx.listener(Self::unstage_all))
                    .on_action(cx.listener(Self::stage_selected))
                    .on_action(cx.listener(Self::unstage_selected))
                    .on_action(cx.listener(Self::restore_tracked_files))
                    .on_action(cx.listener(Self::revert_selected))
                    .on_action(cx.listener(Self::add_to_gitignore))
                    .on_action(cx.listener(Self::add_to_git_info_exclude))
                    .on_action(cx.listener(Self::clean_all))
                    .on_action(cx.listener(Self::generate_commit_message_action))
                    .on_action(cx.listener(Self::stash_all))
                    .on_action(cx.listener(Self::stash_pop))
            })
            .on_action(cx.listener(Self::collapse_selected_entry))
            .on_action(cx.listener(Self::expand_selected_entry))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::first_entry))
            .on_action(cx.listener(Self::next_entry))
            .on_action(cx.listener(Self::previous_entry))
            .on_action(cx.listener(Self::last_entry))
            .on_action(cx.listener(Self::close_panel))
            .on_action(cx.listener(Self::open_diff))
            .on_action(cx.listener(Self::open_solo_diff))
            .on_action(cx.listener(Self::view_file))
            .on_action(cx.listener(Self::focus_changes_list))
            .on_action(cx.listener(Self::focus_editor))
            .on_action(cx.listener(Self::expand_commit_editor))
            .when(has_write_access && has_co_authors, |git_panel| {
                git_panel.on_action(cx.listener(Self::toggle_fill_co_authors))
            })
            .on_action(cx.listener(Self::set_sort_by_path))
            .on_action(cx.listener(Self::set_sort_by_name))
            .on_action(cx.listener(Self::set_group_by_none))
            .on_action(cx.listener(Self::set_group_by_status))
            .on_action(cx.listener(Self::toggle_tree_view))
            .on_action(cx.listener(Self::increase_font_size))
            .on_action(cx.listener(Self::decrease_font_size))
            .on_action(cx.listener(Self::reset_font_size))
            .on_action(cx.listener(Self::activate_changes_tab))
            .on_action(cx.listener(Self::activate_history_tab))
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .size_full()
                    .when(!self.commit_editor_expanded, |this| {
                        this.child(self.render_tab_bar(cx))
                    })
                    .map(|this| match self.active_tab {
                        GitPanelTab::Changes => this
                            .children(self.render_changes_header(window, cx))
                            .when(!self.commit_editor_expanded, |this| {
                                this.map(|this| {
                                    if let Some(repo) = self.active_repository.clone()
                                        && has_entries
                                    {
                                        this.child(self.render_entries(
                                            has_write_access,
                                            repo,
                                            window,
                                            cx,
                                        ))
                                    } else {
                                        this.child(self.render_empty_state(cx).into_any_element())
                                    }
                                })
                            })
                            .children(self.render_footer(window, cx))
                            .when(self.amend_pending, |this| {
                                this.child(self.render_pending_amend(cx))
                            })
                            .when(!self.amend_pending, |this| {
                                this.children(self.render_previous_commit(window, cx))
                            }),
                        GitPanelTab::History => this.child(self.render_history_tab(window, cx)),
                    })
                    .into_any_element(),
            )
            .children(self.context_menu.as_ref().map(|(menu, position, _)| {
                deferred(
                    anchored()
                        .position(*position)
                        .anchor(Anchor::TopLeft)
                        .child(menu.clone()),
                )
                .with_priority(1)
            }))
    }
}

impl Focusable for GitPanel {
    fn focus_handle(&self, cx: &App) -> gpui::FocusHandle {
        if self.entries.is_empty() || self.commit_editor_expanded {
            self.commit_editor.focus_handle(cx)
        } else {
            self.focus_handle.clone()
        }
    }
}

impl EventEmitter<Event> for GitPanel {}

impl EventEmitter<PanelEvent> for GitPanel {}

pub(crate) struct GitPanelAddon {
    pub(crate) workspace: WeakEntity<Workspace>,
}

impl editor::Addon for GitPanelAddon {
    fn to_any(&self) -> &dyn std::any::Any {
        self
    }

    fn render_buffer_header_controls(
        &self,
        _excerpt_info: &ExcerptBoundaryInfo,
        buffer: &language::BufferSnapshot,
        window: &Window,
        cx: &App,
    ) -> Option<AnyElement> {
        let file = buffer.file()?;
        let git_panel = self.workspace.upgrade()?.read(cx).panel::<GitPanel>(cx)?;

        git_panel
            .read(cx)
            .render_buffer_header_controls(&git_panel, file, window, cx)
    }
}

impl Panel for GitPanel {
    fn persistent_name() -> &'static str {
        "GitPanel"
    }

    fn panel_key() -> &'static str {
        GIT_PANEL_KEY
    }

    fn position(&self, _: &Window, cx: &App) -> DockPosition {
        GitPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(&mut self, position: DockPosition, _: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.git_panel.get_or_insert_default().dock = Some(position.into())
        });
    }

    fn default_size(&self, _: &Window, cx: &App) -> Pixels {
        GitPanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::GitBranch)
    }

    fn button_visible(&self, cx: &App) -> bool {
        GitPanelSettings::get_global(cx).button
    }

    fn icon_tooltip(&self, _window: &Window, _cx: &App) -> Option<&'static str> {
        Some("Git Panel")
    }

    fn icon_label(&self, _: &Window, cx: &App) -> Option<String> {
        if !GitPanelSettings::get_global(cx).show_count_badge {
            return None;
        }
        let total = self.changes_count;
        (total > 0).then(|| total.to_string())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _: &Window, cx: &App) -> bool {
        GitPanelSettings::get_global(cx).starts_open
    }

    fn activation_priority(&self) -> u32 {
        3
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.git_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

impl PanelHeader for GitPanel {}

pub(crate) fn commit_title_exceeds_limit(title: &str, max_length: usize) -> bool {
    max_length > 0 && title.chars().count() > max_length
}

#[cfg(test)]
mod tests {
    use git::{
        repository::repo_path,
        status::{StatusCode, UnmergedStatus, UnmergedStatusCode},
    };
    use gpui::{TestAppContext, UpdateGlobal, VisualTestContext, px};
    use indoc::indoc;
    use project::FakeFs;
    use serde_json::json;
    use settings::SettingsStore;
    use theme::LoadThemes;
    use util::path;
    use util::rel_path::rel_path;

    use workspace::MultiWorkspace;

    use super::*;

    fn init_test(cx: &mut gpui::TestAppContext) {
        zlog::init_test();

        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(LoadThemes::JustBase, cx);
            language_model::init(cx);
            editor::init(cx);
            crate::init(cx);
        });
    }

    fn register_git_commit_language(project: &Entity<Project>, cx: &mut VisualTestContext) {
        project.read_with(cx, |project, _| {
            project.languages().add(Arc::new(language::Language::new(
                language::LanguageConfig {
                    name: "Git Commit".into(),
                    ..Default::default()
                },
                None,
            )));
        });
    }

    fn entry_index_for_repo_path(panel: &GitPanel, repo_path: &RepoPath) -> Option<usize> {
        panel.entries.iter().position(|entry| {
            entry
                .status_entry()
                .is_some_and(|entry| &entry.repo_path == repo_path)
        })
    }

    async fn await_git_panel_entries(panel: &Entity<GitPanel>, cx: &mut VisualTestContext) {
        let handle = cx.update_window_entity(panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;
    }

    fn assert_editor_opened_with_path(
        workspace: &Entity<Workspace>,
        expected_path: &Path,
        cx: &mut VisualTestContext,
    ) {
        workspace.update_in(cx, |workspace, _window, cx| {
            let editor = workspace
                .item_of_type::<editor::Editor>(cx)
                .expect("Editor should exist after View File");
            let file_path = editor
                .read(cx)
                .active_buffer(cx)
                .expect("Buffer should have an active buffer")
                .read(cx)
                .file()
                .cloned()
                .expect("Buffer should have a file");
            assert_eq!(file_path.path().as_ref().as_std_path(), expected_path);
        });
    }

    async fn setup_git_panel_with_changes(
        cx: &mut TestAppContext,
        tree: serde_json::Value,
        status_entries: &[(&str, git::status::StatusCode)],
    ) -> (
        Entity<Project>,
        Entity<Workspace>,
        Entity<GitPanel>,
        VisualTestContext,
    ) {
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(path!("/project"), tree).await;

        if !status_entries.is_empty() {
            fs.set_status_for_repo(
                path!("/project/.git").as_ref(),
                &status_entries
                    .iter()
                    .map(|(path, status)| (*path, status.worktree()))
                    .collect::<Vec<_>>(),
            );
        }

        let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let panel = workspace.update_in(&mut cx, GitPanel::new);
        await_git_panel_entries(&panel, &mut cx).await;

        (project, workspace, panel, cx)
    }

    #[gpui::test]
    async fn test_view_file_tracked(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "tracked": "tracked\n",
            }),
        )
        .await;

        fs.set_head_and_index_for_repo(
            path!("/project/.git").as_ref(),
            &[("tracked", "old tracked\n".into())],
        );

        let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        let panel = workspace.update_in(&mut cx, GitPanel::new);
        await_git_panel_entries(&panel, &mut cx).await;

        let entry_index = panel
            .read_with(&cx, |panel, _| {
                entry_index_for_repo_path(panel, &repo_path("tracked"))
            })
            .expect("tracked file should exist in the changes list");

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.selected_entry = Some(entry_index);
            panel.view_file(&ViewFile, window, cx);
        });
        cx.run_until_parked();

        assert_editor_opened_with_path(&workspace, Path::new("tracked"), &mut cx);
    }

    #[gpui::test]
    async fn test_view_file_untracked(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "tracked": "tracked\n",
                "untracked": "\n",
            }),
        )
        .await;

        fs.set_head_and_index_for_repo(
            path!("/project/.git").as_ref(),
            &[("tracked", "old tracked\n".into())],
        );

        let project = Project::test(fs, [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let mut cx = VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().sort_by = Some(GitPanelSortBy::Path);
                })
            });
        });

        let panel = workspace.update_in(&mut cx, GitPanel::new);
        await_git_panel_entries(&panel, &mut cx).await;

        let entry_index = panel
            .read_with(&cx, |panel, _| {
                entry_index_for_repo_path(panel, &repo_path("untracked"))
            })
            .expect("untracked file should exist in the changes list");

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.selected_entry = Some(entry_index);
            panel.view_file(&ViewFile, window, cx);
        });
        cx.run_until_parked();

        assert_editor_opened_with_path(&workspace, Path::new("untracked"), &mut cx);
    }

    #[gpui::test]
    async fn test_view_file_tree_view(cx: &mut TestAppContext) {
        init_test(cx);

        let (_project, workspace, panel, mut cx) = setup_git_panel_with_changes(
            cx,
            json!({
                ".git": {},
                "src": {
                    "a": {
                        "foo.rs": "fn foo() {}",
                    },
                },
            }),
            &[("src/a/foo.rs", StatusCode::Modified)],
        )
        .await;

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().tree_view = Some(true);
                })
            });
        });
        await_git_panel_entries(&panel, &mut cx).await;

        let entry_index = panel
            .read_with(&cx, |panel, _| {
                entry_index_for_repo_path(panel, &repo_path("src/a/foo.rs"))
            })
            .expect("foo.rs should exist in the tree view changes list");

        panel.update_in(&mut cx, |panel, window, cx| {
            panel.selected_entry = Some(entry_index);
            panel.view_file(&ViewFile, window, cx);
        });
        cx.run_until_parked();

        assert_editor_opened_with_path(&workspace, Path::new("src/a/foo.rs"), &mut cx);
    }

    #[test]
    fn test_format_git_error_toast_message_prefers_raw_rpc_message() {
        let rpc_error = RpcError::from_proto(
            &proto::Error {
                message:
                    "Your local changes to the following files would be overwritten by merge\n"
                        .to_string(),
                code: proto::ErrorCode::Internal as i32,
                tags: Default::default(),
            },
            "Pull",
        );

        let message = format_git_error_toast_message(&rpc_error);
        assert_eq!(
            message,
            "Your local changes to the following files would be overwritten by merge"
        );
    }

    #[test]
    fn test_format_git_error_toast_message_prefers_raw_rpc_message_when_wrapped() {
        let rpc_error = RpcError::from_proto(
            &proto::Error {
                message:
                    "Your local changes to the following files would be overwritten by merge\n"
                        .to_string(),
                code: proto::ErrorCode::Internal as i32,
                tags: Default::default(),
            },
            "Pull",
        );
        let wrapped = rpc_error.context("sending pull request");

        let message = format_git_error_toast_message(&wrapped);
        assert_eq!(
            message,
            "Your local changes to the following files would be overwritten by merge"
        );
    }

    #[gpui::test]
    async fn test_entry_worktree_paths(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "mav": {
                    ".git": {},
                    "crates": {
                        "gpui": {
                            "gpui.rs": "fn main() {}"
                        },
                        "util": {
                            "util.rs": "fn do_it() {}"
                        }
                    }
                },
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/mav/.git")),
            &[
                ("crates/gpui/gpui.rs", StatusCode::Modified.worktree()),
                ("crates/util/util.rs", StatusCode::Modified.worktree()),
            ],
        );

        let project =
            Project::test(fs.clone(), [path!("/root/mav/crates/gpui").as_ref()], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        pretty_assertions::assert_eq!(
            entries,
            [
                GitListEntry::Header(GitHeaderEntry {
                    header: Section::Tracked
                }),
                GitListEntry::Status(GitStatusEntry {
                    repo_path: repo_path("crates/gpui/gpui.rs"),
                    status: StatusCode::Modified.worktree(),
                    staging: StageStatus::Unstaged,
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                }),
                GitListEntry::Status(GitStatusEntry {
                    repo_path: repo_path("crates/util/util.rs"),
                    status: StatusCode::Modified.worktree(),
                    staging: StageStatus::Unstaged,
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },),
            ],
        );

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;
        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        pretty_assertions::assert_eq!(
            entries,
            [
                GitListEntry::Header(GitHeaderEntry {
                    header: Section::Tracked
                }),
                GitListEntry::Status(GitStatusEntry {
                    repo_path: repo_path("crates/gpui/gpui.rs"),
                    status: StatusCode::Modified.worktree(),
                    staging: StageStatus::Unstaged,
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                }),
                GitListEntry::Status(GitStatusEntry {
                    repo_path: repo_path("crates/util/util.rs"),
                    status: StatusCode::Modified.worktree(),
                    staging: StageStatus::Unstaged,
                    diff_stat: Some(DiffStat {
                        added: 1,
                        deleted: 1,
                    }),
                },),
            ],
        );
    }

    #[gpui::test]
    async fn test_discard_prompt_escapes_markdown_in_file_name(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "__somefile__": "modified\n",
                },
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project/.git")),
            &[("__somefile__", StatusCode::Modified.worktree())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(1);
            panel.revert_selected(&git::RestoreFile::default(), window, cx);
        });

        let (message, _detail) = cx
            .pending_prompt()
            .expect("discard should show a confirmation prompt");

        assert_eq!(
            message,
            "Are you sure you want to discard changes to `__somefile__`?"
        );
    }

    #[gpui::test]
    async fn test_bulk_staging(cx: &mut TestAppContext) {
        use GitListEntry::*;

        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}",
                        "lib.rs": "pub fn hello() {}",
                        "utils.rs": "pub fn util() {}"
                    },
                    "tests": {
                        "test.rs": "fn test() {}"
                    },
                    "new_file.txt": "new content",
                    "another_new.rs": "// new file",
                    "conflict.txt": "conflicted content"
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project/.git")),
            &[
                ("src/main.rs", StatusCode::Modified.worktree()),
                ("src/lib.rs", StatusCode::Modified.worktree()),
                ("tests/test.rs", StatusCode::Modified.worktree()),
                ("new_file.txt", FileStatus::Untracked),
                ("another_new.rs", FileStatus::Untracked),
                ("src/utils.rs", FileStatus::Untracked),
                (
                    "conflict.txt",
                    UnmergedStatus {
                        first_head: UnmergedStatusCode::Updated,
                        second_head: UnmergedStatusCode::Updated,
                    }
                    .into(),
                ),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Header(GitHeaderEntry { header: Section::Conflict }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::Tracked }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::New }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            ],
        );

        let second_status_entry = entries[3].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&second_status_entry, window, cx);
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(7);
            panel.stage_range(&git::StageRange, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Header(GitHeaderEntry { header: Section::Conflict }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::Tracked }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Header(GitHeaderEntry { header: Section::New }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            ],
        );

        let third_status_entry = entries[4].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&third_status_entry, window, cx);
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(9);
            panel.stage_range(&git::StageRange, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Header(GitHeaderEntry { header: Section::Conflict }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::Tracked }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Header(GitHeaderEntry { header: Section::New }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Staged, .. }),
            ],
        );
    }

    #[gpui::test]
    async fn test_bulk_staging_with_sort_by_paths(cx: &mut TestAppContext) {
        use GitListEntry::*;

        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}",
                        "lib.rs": "pub fn hello() {}",
                        "utils.rs": "pub fn util() {}"
                    },
                    "tests": {
                        "test.rs": "fn test() {}"
                    },
                    "new_file.txt": "new content",
                    "another_new.rs": "// new file",
                    "conflict.txt": "conflicted content"
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project/.git")),
            &[
                ("src/main.rs", StatusCode::Modified.worktree()),
                ("src/lib.rs", StatusCode::Modified.worktree()),
                ("tests/test.rs", StatusCode::Modified.worktree()),
                ("new_file.txt", FileStatus::Untracked),
                ("another_new.rs", FileStatus::Untracked),
                ("src/utils.rs", FileStatus::Untracked),
                (
                    "conflict.txt",
                    UnmergedStatus {
                        first_head: UnmergedStatusCode::Updated,
                        second_head: UnmergedStatusCode::Updated,
                    }
                    .into(),
                ),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Header(GitHeaderEntry { header: Section::Conflict }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::Tracked }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Header(GitHeaderEntry { header: Section::New }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { staging: StageStatus::Unstaged, .. }),
            ],
        );

        assert_entry_paths(
            &entries,
            &[
                None,
                Some("conflict.txt"),
                None,
                Some("src/lib.rs"),
                Some("src/main.rs"),
                Some("tests/test.rs"),
                None,
                Some("another_new.rs"),
                Some("new_file.txt"),
                Some("src/utils.rs"),
            ],
        );

        let second_status_entry = entries[3].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&second_status_entry, window, cx);
        });

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().group_by =
                        Some(GitPanelGroupBy::None);
                })
            });
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(7);
            panel.stage_range(&git::StageRange, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Unmerged(..), staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
            ],
        );

        assert_entry_paths(
            &entries,
            &[
                Some("another_new.rs"),
                Some("conflict.txt"),
                Some("new_file.txt"),
                Some("src/lib.rs"),
                Some("src/main.rs"),
                Some("src/utils.rs"),
                Some("tests/test.rs"),
            ],
        );

        let third_status_entry = entries[4].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&third_status_entry, window, cx);
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(9);
            panel.stage_range(&git::StageRange, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());
        #[rustfmt::skip]
        pretty_assertions::assert_matches!(
            entries.as_slice(),
            &[
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Unmerged(..), staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Staged, .. }),
                Status(GitStatusEntry { status: FileStatus::Untracked, staging: StageStatus::Unstaged, .. }),
                Status(GitStatusEntry { status: FileStatus::Tracked(..), staging: StageStatus::Unstaged, .. }),
            ],
        );

        assert_entry_paths(
            &entries,
            &[
                Some("another_new.rs"),
                Some("conflict.txt"),
                Some("new_file.txt"),
                Some("src/lib.rs"),
                Some("src/main.rs"),
                Some("src/utils.rs"),
                Some("tests/test.rs"),
            ],
        );
    }

    #[gpui::test]
    async fn test_amend_commit_message_handling(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        let panel = workspace.update_in(cx, GitPanel::new);

        // Test: User has commit message, enables amend (saves message), then disables (restores message)
        panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "Initial commit message")], None, cx);
            });

            panel.set_amend_pending(true, cx);
            assert!(panel.original_commit_message.is_some());

            panel.set_amend_pending(false, cx);
            let current_message = panel.commit_message_buffer(cx).read(cx).text();
            assert_eq!(current_message, "Initial commit message");
            assert!(panel.original_commit_message.is_none());
        });

        // Test: User has empty commit message, enables amend, then disables (clears message)
        panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "")], None, cx);
            });

            panel.set_amend_pending(true, cx);
            assert!(panel.original_commit_message.is_none());

            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "Previous commit message")], None, cx);
            });

            panel.set_amend_pending(false, cx);
            let current_message = panel.commit_message_buffer(cx).read(cx).text();
            assert_eq!(current_message, "");
        });
    }

    #[gpui::test]
    async fn test_commit_message_restored_after_reconnect(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project-a": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                },
                "project-b": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project-a/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );
        fs.set_status_for_repo(
            Path::new(path!("/root/project-b/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );

        let project = Project::test(
            fs.clone(),
            [
                Path::new(path!("/root/project-a")),
                Path::new(path!("/root/project-b")),
            ],
            cx,
        )
        .await;
        let (repository_a, repository_b) = project.read_with(cx, |project, cx| {
            let git_store = project.git_store().clone();
            let mut repository_a = None;
            let mut repository_b = None;
            for repository in git_store.read(cx).repositories().values() {
                let work_directory_abs_path = &repository.read(cx).work_directory_abs_path;
                if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-a")) {
                    repository_a = Some(repository.clone());
                } else if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-b")) {
                    repository_b = Some(repository.clone());
                }
            }
            (
                repository_a.expect("should have repository for project-a"),
                repository_b.expect("should have repository for project-b"),
            )
        });
        repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));

        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        register_git_commit_language(&project, cx);
        let panel = workspace.update_in(cx, GitPanel::new);
        cx.run_until_parked();

        let message_a = "Restore repository A message";
        panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, message_a)], None, cx);
            });
        });

        repository_b.update(cx, |repository, cx| repository.set_as_active_repository(cx));
        cx.run_until_parked();

        let message_b = "Restore repository B message";
        let serialized_panel = panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, message_b)], None, cx);
            });

            SerializedGitPanel {
                signoff_enabled: false,
                commit_messages: panel.serialized_commit_messages(cx),
            }
        });

        for repository in [&repository_a, &repository_b] {
            let buffer = repository.read_with(cx, |repository, _| {
                repository
                    .commit_message_buffer()
                    .expect("repository commit message buffer should be open")
                    .clone()
            });
            buffer.update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "")], None, cx);
            });
        }

        let restored_panel = workspace.update_in(cx, |workspace, window, cx| {
            GitPanel::new_with_serialized_panel(workspace, Some(serialized_panel), window, cx)
        });
        cx.run_until_parked();

        restored_panel.read_with(cx, |panel, cx| {
            assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), message_b);
        });

        repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));
        cx.run_until_parked();

        restored_panel.read_with(cx, |panel, cx| {
            assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), message_a);
        });

        restored_panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "")], None, cx);
            });
        });

        let mismatched_serialized_panel = SerializedGitPanel {
            signoff_enabled: false,
            commit_messages: BTreeMap::from_iter([(
                path!("/root/other-project").to_string(),
                SerializedCommitMessage {
                    message: Some(message_a.to_string()),
                    original_message: None,
                    ..Default::default()
                },
            )]),
        };
        let mismatched_panel = workspace.update_in(cx, |workspace, window, cx| {
            GitPanel::new_with_serialized_panel(
                workspace,
                Some(mismatched_serialized_panel),
                window,
                cx,
            )
        });
        cx.run_until_parked();

        mismatched_panel.read_with(cx, |panel, cx| {
            // The draft is not restored because the serialized work directory
            // does not match the active repository, so it cannot leak across
            // repositories.
            assert_eq!(panel.commit_message_buffer(cx).read(cx).text(), "");
        });
    }

    #[gpui::test]
    async fn test_amend_state_is_per_repository(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project-a": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                },
                "project-b": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project-a/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );
        fs.set_status_for_repo(
            Path::new(path!("/root/project-b/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );

        let project = Project::test(
            fs.clone(),
            [
                Path::new(path!("/root/project-a")),
                Path::new(path!("/root/project-b")),
            ],
            cx,
        )
        .await;
        let (repository_a, repository_b) = project.read_with(cx, |project, cx| {
            let git_store = project.git_store().clone();
            let mut repository_a = None;
            let mut repository_b = None;
            for repository in git_store.read(cx).repositories().values() {
                let work_directory_abs_path = &repository.read(cx).work_directory_abs_path;
                if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-a")) {
                    repository_a = Some(repository.clone());
                } else if work_directory_abs_path.as_ref() == Path::new(path!("/root/project-b")) {
                    repository_b = Some(repository.clone());
                }
            }
            (
                repository_a.expect("should have repository for project-a"),
                repository_b.expect("should have repository for project-b"),
            )
        });
        repository_a.update(cx, |repository, cx| repository.set_as_active_repository(cx));

        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        register_git_commit_language(&project, cx);
        let panel = workspace.update_in(cx, GitPanel::new);
        cx.run_until_parked();

        // Enter an amend on repository A, then simulate the amend flow loading
        // the last commit message into the editor.
        panel.update(cx, |panel, cx| {
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "Draft for A")], None, cx);
            });
            panel.set_amend_pending(true, cx);
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                let start = buffer.anchor_before(0);
                let end = buffer.anchor_after(buffer.len());
                buffer.edit([(start..end, "Amended message")], None, cx);
            });
            assert!(panel.amend_pending());
        });

        // Switching the active repository away exits the amend state instead of
        // carrying it over to repository B.
        repository_b.update(cx, |repository, cx| repository.set_as_active_repository(cx));
        cx.run_until_parked();

        panel.update(cx, |panel, cx| {
            assert!(!panel.amend_pending());
            // Only the active repository may serialize a pending amend, and we
            // just left repository A's amend, so nothing is left pending.
            let serialized = panel.serialized_commit_messages(cx);
            assert!(serialized.values().all(|message| !message.amend_pending));
        });

        // Repository A's pre-amend draft is restored, discarding the amend edit.
        let buffer_a = repository_a.read_with(cx, |repository, _| {
            repository
                .commit_message_buffer()
                .expect("repository commit message buffer should be open")
                .clone()
        });
        buffer_a.read_with(cx, |buffer, _| {
            assert_eq!(buffer.text(), "Draft for A");
        });
    }

    #[gpui::test]
    async fn test_amend(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({
                "project": {
                    ".git": {},
                    "src": {
                        "main.rs": "fn main() {}"
                    }
                }
            }),
        )
        .await;

        fs.set_status_for_repo(
            Path::new(path!("/root/project/.git")),
            &[("src/main.rs", StatusCode::Modified.worktree())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        // Wait for the project scanning to finish so that `head_commit(cx)` is
        // actually set, otherwise no head commit would be available from which
        // to fetch the latest commit message from.
        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);
        panel.read_with(cx, |panel, cx| {
            assert!(panel.active_repository.is_some());
            assert!(panel.head_commit(cx).is_some());
        });

        panel.update_in(cx, |panel, window, cx| {
            // Update the commit editor's message to ensure that its contents
            // are later restored, after amending is finished.
            panel.commit_message_buffer(cx).update(cx, |buffer, cx| {
                buffer.set_text("refactor: update main.rs", cx);
            });

            // Start amending the previous commit.
            panel.focus_editor(&Default::default(), window, cx);
            panel.on_amend(&Amend, window, cx);
        });

        // Since `GitPanel.amend` attempts to fetch the latest commit message in
        // a background task, we need to wait for it to complete before being
        // able to assert that the commit message editor's state has been
        // updated.
        cx.run_until_parked();

        panel.update_in(cx, |panel, window, cx| {
            assert_eq!(
                panel.commit_message_buffer(cx).read(cx).text(),
                "initial commit"
            );
            assert_eq!(
                panel.original_commit_message,
                Some("refactor: update main.rs".to_string())
            );

            // Finish amending the previous commit.
            panel.focus_editor(&Default::default(), window, cx);
            panel.on_amend(&Amend, window, cx);
        });

        // Since the actual commit logic is run in a background task, we need to
        // await its completion to actually ensure that the commit message
        // editor's contents are set to the original message and haven't been
        // cleared.
        cx.run_until_parked();

        panel.update_in(cx, |panel, _window, cx| {
            // After amending, the commit editor's message should be restored to
            // the original message.
            assert_eq!(
                panel.commit_message_buffer(cx).read(cx).text(),
                "refactor: update main.rs"
            );
            assert!(panel.original_commit_message.is_none());
        });
    }

    #[gpui::test]
    async fn test_open_diff(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "tracked": "tracked\n",
                "untracked": "\n",
            }),
        )
        .await;

        fs.set_head_and_index_for_repo(
            path!("/project/.git").as_ref(),
            &[("tracked", "old tracked\n".into())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
        let panel = workspace.update_in(cx, GitPanel::new);

        // Disable status grouping and wait for entries to be updated,
        // as there should no longer be separators between Tracked and Untracked
        // files.
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().group_by =
                        Some(GitPanelGroupBy::None);
                })
            });
        });

        cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        })
        .await;

        // Confirm that `Open Diff` still works for the untracked file, updating
        // the Project Diff's active path.
        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(1);
            panel.open_diff(&menu::Confirm, window, cx);
        });
        cx.run_until_parked();

        workspace.update_in(cx, |workspace, _window, cx| {
            let active_path = workspace
                .item_of_type::<ProjectDiff>(cx)
                .expect("ProjectDiff should exist")
                .read(cx)
                .active_project_path(cx)
                .expect("active_project_path should exist");

            assert_eq!(active_path.path, rel_path("untracked").into_arc());
        });
    }

    #[gpui::test]
    async fn test_remote_operation_serialization(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
            }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
        let panel = workspace.update_in(cx, GitPanel::new);

        panel.update(cx, |panel, cx| {
            // The first remote operation starts and records its kind, which the
            // button uses to render an "in progress" tooltip.
            assert!(panel.start_remote_operation(RemoteOperationKind::Fetch, cx));
            assert!(matches!(
                panel.pending_remote_operation,
                Some(RemoteOperationKind::Fetch)
            ));

            // A second remote operation is refused while one is pending, even a
            // different kind: we serialize all remote ops.
            assert!(!panel.start_remote_operation(RemoteOperationKind::Push, cx));

            // Clearing the pending operation re-opens the gate.
            panel.clear_remote_operation(cx);
            assert!(panel.pending_remote_operation.is_none());
            assert!(panel.start_remote_operation(RemoteOperationKind::Pull, cx));
        });
    }

    #[gpui::test]
    async fn test_tree_view_without_status_grouping_combines_statuses(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "src": {
                    "main.rs": "fn main() {}",
                    "utils.rs": "pub fn util() {}",
                },
                "tests": {
                    "main_test.rs": "#[test] fn test_main() {}",
                },
            }),
        )
        .await;

        fs.set_status_for_repo(
            path!("/project/.git").as_ref(),
            &[
                ("src/main.rs", StatusCode::Modified.worktree()),
                ("src/utils.rs", FileStatus::Untracked),
                ("tests/main_test.rs", StatusCode::Modified.worktree()),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    let git_panel = settings.git_panel.get_or_insert_default();
                    git_panel.tree_view = Some(true);
                    git_panel.group_by = Some(GitPanelGroupBy::None);
                })
            });
        });

        let panel = workspace.update_in(cx, GitPanel::new);
        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });

        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        panel.read_with(cx, |panel, _| {
            assert!(
                panel
                    .entries
                    .iter()
                    .all(|entry| !matches!(entry, GitListEntry::Header(_))),
                "status headers should not be shown when grouping is disabled",
            );

            let tree_state = panel
                .view_mode
                .tree_state()
                .expect("tree view state should exist");
            let src_key = panel
                .entries
                .iter()
                .find_map(|entry| match entry {
                    GitListEntry::Directory(dir) if dir.key.path == repo_path("src") => {
                        Some(&dir.key)
                    }
                    _ => None,
                })
                .expect("src directory should exist in tree view");
            let src_descendants = tree_state
                .directory_descendants
                .get(src_key)
                .expect("src descendants should be tracked");

            assert!(
                src_descendants
                    .iter()
                    .any(|entry| entry.repo_path == repo_path("src/main.rs"))
            );
            assert!(
                src_descendants
                    .iter()
                    .any(|entry| entry.repo_path == repo_path("src/utils.rs"))
            );
        });
    }

    #[gpui::test]
    async fn test_tree_view_reveals_collapsed_parent_on_select_entry_by_path(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "src": {
                    "a": {
                        "foo.rs": "fn foo() {}",
                    },
                    "b": {
                        "bar.rs": "fn bar() {}",
                    },
                },
            }),
        )
        .await;

        fs.set_status_for_repo(
            path!("/project/.git").as_ref(),
            &[
                ("src/a/foo.rs", StatusCode::Modified.worktree()),
                ("src/b/bar.rs", StatusCode::Modified.worktree()),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().tree_view = Some(true);
                })
            });
        });

        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let src_key = panel.read_with(cx, |panel, _| {
            panel
                .entries
                .iter()
                .find_map(|entry| match entry {
                    GitListEntry::Directory(dir) if dir.key.path == repo_path("src") => {
                        Some(dir.key.clone())
                    }
                    _ => None,
                })
                .expect("src directory should exist in tree view")
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_directory(&src_key, window, cx);
        });

        panel.read_with(cx, |panel, _| {
            let state = panel
                .view_mode
                .tree_state()
                .expect("tree view state should exist");
            assert_eq!(state.expanded_dirs.get(&src_key.path).copied(), Some(false));
        });

        let worktree_id =
            cx.read(|cx| project.read(cx).worktrees(cx).next().unwrap().read(cx).id());
        let project_path = ProjectPath {
            worktree_id,
            path: RelPath::unix("src/a/foo.rs").unwrap().into_arc(),
        };

        panel.update_in(cx, |panel, window, cx| {
            panel.select_entry_by_path(project_path, window, cx);
        });

        panel.read_with(cx, |panel, _| {
            let state = panel
                .view_mode
                .tree_state()
                .expect("tree view state should exist");
            assert_eq!(state.expanded_dirs.get(&src_key.path).copied(), Some(true));

            let selected_ix = panel.selected_entry.expect("selection should be set");
            assert!(state.logical_indices.contains(&selected_ix));

            let selected_entry = panel
                .entries
                .get(selected_ix)
                .and_then(|entry| entry.status_entry())
                .expect("selected entry should be a status entry");
            assert_eq!(selected_entry.repo_path, repo_path("src/a/foo.rs"));
        });
    }

    #[gpui::test]
    async fn test_tree_view_select_next_at_last_visible_collapsed_directory(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "bar": {
                    "bar1.py": "print('bar1')",
                    "bar2.py": "print('bar2')",
                },
                "foo": {
                    "foo1.py": "print('foo1')",
                    "foo2.py": "print('foo2')",
                },
                "foobar.py": "print('foobar')",
            }),
        )
        .await;

        fs.set_status_for_repo(
            path!("/project/.git").as_ref(),
            &[
                ("bar/bar1.py", StatusCode::Modified.worktree()),
                ("bar/bar2.py", StatusCode::Modified.worktree()),
                ("foo/foo1.py", StatusCode::Modified.worktree()),
                ("foo/foo2.py", StatusCode::Modified.worktree()),
                ("foobar.py", FileStatus::Untracked),
            ],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();
        cx.update(|_window, cx| {
            SettingsStore::update_global(cx, |store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.git_panel.get_or_insert_default().tree_view = Some(true);
                })
            });
        });

        let panel = workspace.update_in(cx, GitPanel::new);
        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });

        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let foo_key = panel.read_with(cx, |panel, _| {
            panel
                .entries
                .iter()
                .find_map(|entry| match entry {
                    GitListEntry::Directory(dir) if dir.key.path == repo_path("foo") => {
                        Some(dir.key.clone())
                    }
                    _ => None,
                })
                .expect("foo directory should exist in tree view")
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_directory(&foo_key, window, cx);
        });

        let foo_idx = panel.read_with(cx, |panel, _| {
            let state = panel
                .view_mode
                .tree_state()
                .expect("tree view state should exist");
            assert_eq!(state.expanded_dirs.get(&foo_key.path).copied(), Some(false));

            let foo_idx = panel
                .entries
                .iter()
                .enumerate()
                .find_map(|(index, entry)| match entry {
                    GitListEntry::Directory(dir) if dir.key.path == repo_path("foo") => Some(index),
                    _ => None,
                })
                .expect("foo directory should exist in tree view");

            let foo_logical_idx = state
                .logical_indices
                .iter()
                .position(|&index| index == foo_idx)
                .expect("foo directory should be visible");
            let next_logical_idx = state.logical_indices[foo_logical_idx + 1];
            assert!(matches!(
                panel.entries.get(next_logical_idx),
                Some(GitListEntry::Header(GitHeaderEntry {
                    header: Section::New
                }))
            ));

            foo_idx
        });

        panel.update_in(cx, |panel, window, cx| {
            panel.selected_entry = Some(foo_idx);
            panel.select_next(&menu::SelectNext, window, cx);
        });

        panel.read_with(cx, |panel, _| {
            let selected_idx = panel.selected_entry.expect("selection should be set");
            let selected_entry = panel
                .entries
                .get(selected_idx)
                .and_then(|entry| entry.status_entry())
                .expect("selected entry should be a status entry");
            assert_eq!(selected_entry.repo_path, repo_path("foobar.py"));
        });
    }

    fn assert_entry_paths(entries: &[GitListEntry], expected_paths: &[Option<&str>]) {
        assert_eq!(entries.len(), expected_paths.len());
        for (entry, expected_path) in entries.iter().zip(expected_paths) {
            assert_eq!(
                entry.status_entry().map(|status| status
                    .repo_path
                    .as_ref()
                    .as_std_path()
                    .to_string_lossy()
                    .to_string()),
                expected_path.map(|s| s.to_string())
            );
        }
    }

    #[test]
    fn test_compress_diff_no_truncation() {
        let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,2 @@
            -old
            +new
        "};
        let result = GitPanel::compress_commit_diff(diff, 1000);
        assert_eq!(result, diff);
    }

    #[test]
    fn test_compress_diff_truncate_long_lines() {
        let long_line = "🦀".repeat(300);
        let diff = indoc::formatdoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,3 @@
             context
            +{}
             more context
        ", long_line};
        let result = GitPanel::compress_commit_diff(&diff, 100);
        assert!(result.contains("...[truncated]"));
        assert!(result.len() < diff.len());
    }

    #[test]
    fn test_compress_diff_truncate_hunks() {
        let diff = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,2 @@
             context
            -old1
            +new1
            @@ -5,2 +5,2 @@
             context 2
            -old2
            +new2
            @@ -10,2 +10,2 @@
             context 3
            -old3
            +new3
        "};
        let result = GitPanel::compress_commit_diff(diff, 100);
        let expected = indoc! {"
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,2 @@
             context
            -old1
            +new1
            [...skipped 2 hunks...]
        "};
        assert_eq!(result, expected);
    }

    #[test]
    fn test_commit_message_prompt_includes_user_agents_md_before_project_rules() {
        let prompt = GitPanel::build_commit_message_prompt(
            "Write a commit message.",
            Some("Use terse commit messages."),
            Some("Use the git_ui prefix."),
            Some("Follow the configured commit message format."),
            "Update generated message",
            "diff --git a/file b/file",
        );

        assert!(prompt.contains("Use terse commit messages."));
        assert!(prompt.contains("Use the git_ui prefix."));
        assert!(prompt.contains("Follow the configured commit message format."));
        assert!(prompt.contains("Update generated message"));
        assert!(prompt.contains("diff --git a/file b/file"));

        let user_agents_md_index = prompt.find("<rules>").unwrap();
        let project_rules_index = prompt.find("<project_rules>").unwrap();
        let instructions_index = prompt.find("<commit_message_instructions>").unwrap();
        assert!(user_agents_md_index < project_rules_index);
        assert!(project_rules_index < instructions_index);
    }

    #[test]
    fn test_commit_message_prompt_omits_blank_instructions() {
        let prompt = GitPanel::build_commit_message_prompt(
            "Write a commit message.",
            None,
            None,
            Some("   \n  "),
            "",
            "diff --git a/file b/file",
        );

        assert!(!prompt.contains("<commit_message_instructions>"));
    }

    #[gpui::test]
    async fn test_suggest_commit_message(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "tracked": "tracked\n",
                "untracked": "\n",
            }),
        )
        .await;

        fs.set_head_and_index_for_repo(
            path!("/project/.git").as_ref(),
            &[("tracked", "old tracked\n".into())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        let entries = panel.read_with(cx, |panel, _| panel.entries.clone());

        // GitPanel
        // - Tracked:
        // - [] tracked
        // - Untracked
        // - [] untracked
        //
        // The commit message should now read:
        // "Update tracked"
        let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
        assert_eq!(message, Some("Update tracked".to_string()));

        let first_status_entry = entries[1].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&first_status_entry, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        // GitPanel
        // - Tracked:
        // - [x] tracked
        // - Untracked
        // - [] untracked
        //
        // The commit message should still read:
        // "Update tracked"
        let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
        assert_eq!(message, Some("Update tracked".to_string()));

        let second_status_entry = entries[3].clone();
        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&second_status_entry, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        // GitPanel
        // - Tracked:
        // - [x] tracked
        // - Untracked
        // - [x] untracked
        //
        // The commit message should now read:
        // "Enter commit message"
        // (which means we should see None returned).
        let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
        assert!(message.is_none());

        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&first_status_entry, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        // GitPanel
        // - Tracked:
        // - [] tracked
        // - Untracked
        // - [x] untracked
        //
        // The commit message should now read:
        // "Update untracked"
        let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
        assert_eq!(message, Some("Create untracked".to_string()));

        panel.update_in(cx, |panel, window, cx| {
            panel.toggle_staged_for_entry(&second_status_entry, window, cx);
        });

        cx.read(|cx| {
            project
                .read(cx)
                .worktrees(cx)
                .next()
                .unwrap()
                .read(cx)
                .as_local()
                .unwrap()
                .scan_complete()
        })
        .await;

        cx.executor().run_until_parked();

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        // GitPanel
        // - Tracked:
        // - [] tracked
        // - Untracked
        // - [] untracked
        //
        // The commit message should now read:
        // "Update tracked"
        let message = panel.update(cx, |panel, cx| panel.suggest_commit_message(cx));
        assert_eq!(message, Some("Update tracked".to_string()));
    }

    #[test]
    fn test_git_output_handler_strips_ansi_codes() {
        let cases = [
            ("no escape codes here\n", "no escape codes here\n"),
            ("\x1b[31mhello\x1b[0m", "hello"),
            ("\x1b[1;32mfoo\x1b[0m bar", "foo bar"),
            ("progress 10%\rprogress 100%\n", "progress 100%\n"),
        ];

        for (input, expected) in cases {
            assert_eq!(terminal::strip_ansi_text(input.as_bytes()), expected);
        }
    }

    #[test]
    fn test_commit_title_exceeds_limit() {
        // ASCII only
        let within_ascii = "abcde";
        let exceeds_ascii = "abcdef";
        assert!(!commit_title_exceeds_limit(within_ascii, 5));
        assert!(commit_title_exceeds_limit(exceeds_ascii, 5));

        // Multi-byte characters are counted as grapheme clusters
        let within_japanese = "あいうえお"; // 5 chars, 15 bytes
        let exceeds_japanese = "あいうえおか"; // 6 chars, 18 bytes
        assert!(!commit_title_exceeds_limit(within_japanese, 5));
        assert!(commit_title_exceeds_limit(exceeds_japanese, 5));

        // Mixed ASCII + multi-byte
        let within_mixed = "abcあ";
        let exceeds_mixed = "abcああ";
        assert!(!commit_title_exceeds_limit(within_mixed, 4));
        assert!(commit_title_exceeds_limit(exceeds_mixed, 4));

        // Emoji counts as one character each
        let within_emoji = "🚀";
        let exceeds_emoji = "🚀🚀";
        assert!(!commit_title_exceeds_limit(within_emoji, 1));
        assert!(commit_title_exceeds_limit(exceeds_emoji, 1));

        // A max_length of 0 disables the limit check
        assert!(!commit_title_exceeds_limit(
            "anything goes when disabled",
            0
        ));
        assert!(!commit_title_exceeds_limit("", 0));

        // Empty title never exceeds a positive limit
        assert!(!commit_title_exceeds_limit("", 72));
    }

    #[gpui::test]
    async fn test_dispatch_context_with_focus_states(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/project"),
            json!({
                ".git": {},
                "tracked": "tracked\n",
            }),
        )
        .await;

        fs.set_head_and_index_for_repo(
            path!("/project/.git").as_ref(),
            &[("tracked", "old tracked\n".into())],
        );

        let project = Project::test(fs.clone(), [Path::new(path!("/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
        let panel = workspace.update_in(cx, GitPanel::new);

        let handle = cx.update_window_entity(&panel, |panel, _, _| {
            std::mem::replace(&mut panel.update_visible_entries_task, Task::ready(()))
        });
        cx.executor().advance_clock(2 * UPDATE_DEBOUNCE);
        handle.await;

        // Case 1: Focus the commit editor — should have "CommitEditor" but NOT "menu"/"ChangesList"
        panel.update_in(cx, |panel, window, cx| {
            panel.focus_editor(&FocusEditor, window, cx);
            let editor_is_focused = panel.commit_editor.read(cx).is_focused(window);
            assert!(
                editor_is_focused,
                "commit editor should be focused after focus_editor action"
            );
            let context = panel.dispatch_context(window, cx);
            assert!(
                context.contains("GitPanel"),
                "should always have GitPanel context"
            );
            assert!(
                context.contains("CommitEditor"),
                "should have CommitEditor context when commit editor is focused"
            );
            assert!(
                !context.contains("menu"),
                "should not have menu context when commit editor is focused"
            );
            assert!(
                !context.contains("ChangesList"),
                "should not have ChangesList context when commit editor is focused"
            );
        });

        // Case 2: Focus the panel's focus handle directly — should have "menu" and "ChangesList".
        // We force a draw via simulate_resize to ensure the dispatch tree is populated,
        // since contains_focused() depends on the rendered dispatch tree.
        panel.update_in(cx, |panel, window, cx| {
            panel.focus_handle.focus(window, cx);
        });
        cx.simulate_resize(gpui::size(px(800.), px(600.)));

        panel.update_in(cx, |panel, window, cx| {
            let context = panel.dispatch_context(window, cx);
            assert!(
                context.contains("GitPanel"),
                "should always have GitPanel context"
            );
            assert!(
                context.contains("menu"),
                "should have menu context when changes list is focused"
            );
            assert!(
                context.contains("ChangesList"),
                "should have ChangesList context when changes list is focused"
            );
            assert!(
                !context.contains("CommitEditor"),
                "should not have CommitEditor context when changes list is focused"
            );
        });

        // Case 3: Switch back to commit editor and verify context switches correctly
        panel.update_in(cx, |panel, window, cx| {
            panel.focus_editor(&FocusEditor, window, cx);
        });

        panel.update_in(cx, |panel, window, cx| {
            let context = panel.dispatch_context(window, cx);
            assert!(
                context.contains("CommitEditor"),
                "should have CommitEditor after switching focus back to editor"
            );
            assert!(
                !context.contains("menu"),
                "should not have menu after switching focus back to editor"
            );
        });

        // Case 4: Re-focus changes list and verify it transitions back correctly
        panel.update_in(cx, |panel, window, cx| {
            panel.focus_handle.focus(window, cx);
        });
        cx.simulate_resize(gpui::size(px(800.), px(600.)));

        panel.update_in(cx, |panel, window, cx| {
            assert!(
                panel.focus_handle.contains_focused(window, cx),
                "panel focus handle should report contains_focused when directly focused"
            );
            let context = panel.dispatch_context(window, cx);
            assert!(
                context.contains("menu"),
                "should have menu context after re-focusing changes list"
            );
            assert!(
                context.contains("ChangesList"),
                "should have ChangesList context after re-focusing changes list"
            );
        });
    }

    #[gpui::test]
    async fn test_fill_commit_editor_toggle(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            "/root",
            json!({ "project": { ".git": {}, "src": { "main.rs": "fn main() {}" } } }),
        )
        .await;

        let project = Project::test(fs.clone(), [Path::new(path!("/root/project"))], cx).await;
        let window_handle =
            cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
        let workspace = window_handle
            .read_with(cx, |mw, _| mw.workspace().clone())
            .unwrap();
        let cx = &mut VisualTestContext::from_window(window_handle.into(), cx);
        cx.executor().run_until_parked();

        let panel = workspace.update_in(cx, GitPanel::new);

        panel.update_in(cx, |panel, window, cx| {
            assert!(!panel.commit_editor_expanded);
            assert!(matches!(
                panel.commit_editor.read(cx).mode().clone(),
                EditorMode::AutoHeight { .. }
            ));

            panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
            assert!(panel.commit_editor_expanded);
            assert!(matches!(
                panel.commit_editor.read(cx).mode().clone(),
                EditorMode::Full { .. }
            ));

            panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
            assert!(!panel.commit_editor_expanded);
            assert!(matches!(
                panel.commit_editor.read(cx).mode().clone(),
                EditorMode::AutoHeight { .. }
            ));
        });
    }

    #[gpui::test]
    async fn test_focus_handle(cx: &mut TestAppContext) {
        init_test(cx);

        let (_project, workspace, panel, mut cx) = setup_git_panel_with_changes(
            cx,
            json!({
                ".git": {},
                "tracked": "tracked\n",
            }),
            &[("tracked", StatusCode::Modified)],
        )
        .await;

        workspace.update_in(&mut cx, |workspace, window, cx| {
            workspace.add_panel(panel.clone(), window, cx);
        });

        // With changes present and the editor not expanded, the panel's own
        // focus handle should be returned, in order for
        // `git_panel::ToggleFocus` to focus on the panel itself.
        panel.update_in(&mut cx, |panel, _window, cx| {
            assert!(!panel.entries.is_empty());
            assert!(!panel.commit_editor_expanded);
            assert_eq!(panel.focus_handle(cx), panel.focus_handle.clone());
        });

        // Expand the editor so we can later confirm that toggling focus
        // actually focuses on the commit editor, seeing as it has been
        // expanded.
        panel.update_in(&mut cx, |panel, window, cx| {
            panel.toggle_fill_commit_editor(&ToggleFillCommitEditor, window, cx);
            assert!(panel.commit_editor_expanded);
        });

        cx.dispatch_action(super::ToggleFocus);
        panel.update_in(&mut cx, |panel, window, cx| {
            assert!(panel.commit_editor.focus_handle(cx).is_focused(window));
        });
    }
}
