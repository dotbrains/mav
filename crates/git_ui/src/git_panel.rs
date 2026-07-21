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
