use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use collections::HashSet;
use fuzzy::StringMatchCandidate;
use git::repository::Worktree as GitWorktree;
use gpui::{
    Action, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement, IntoElement, Modifiers, ModifiersChangedEvent, ParentElement, PromptLevel,
    Render, SharedString, Styled, Subscription, Task, TaskExt, WeakEntity, Window, actions,
};
use picker::{Picker, PickerDelegate, PickerEditorPosition};
use project::Project;
use project::git_store::RepositoryEvent;
use ui::{
    Button, CommonAnimationExt as _, Divider, HighlightedLabel, IconButton, KeyBinding, ListItem,
    ListItemSpacing, ListSubHeader, Tooltip, prelude::*,
};
use util::ResultExt as _;
use util::paths::PathExt;
use workspace::{
    ModalView, MultiWorkspace, Workspace, dock::DockPosition, notifications::DetachAndPromptErr,
};

use crate::git_panel::show_error_toast;
use crate::worktree_service::{RemoteBranchName, WorktreeCreateTarget, worktree_create_targets};
use mav_actions::{
    CreateWorktree, NewWorktreeBranchTarget, OpenWorktreeInNewWindow, SwitchWorktree,
};

actions!(
    worktree_picker,
    [
        /// Deletes the selected git worktree.
        DeleteWorktree,
        /// Force deletes the selected git worktree.
        ForceDeleteWorktree
    ]
);

pub struct WorktreePicker {
    picker: Entity<Picker<WorktreePickerDelegate>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl WorktreePicker {
    pub fn new(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focused_dock = workspace
            .upgrade()
            .and_then(|workspace| workspace.read(cx).focused_dock_position(window, cx));
        Self::new_inner(project, workspace, focused_dock, false, window, cx)
    }

    pub fn new_modal(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        focused_dock: Option<DockPosition>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(project, workspace, focused_dock, true, window, cx)
    }

    fn new_inner(
        project: Entity<Project>,
        workspace: WeakEntity<Workspace>,
        focused_dock: Option<DockPosition>,
        show_footer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let project_ref = project.read(cx);

        let active_worktree_paths: HashSet<PathBuf> = project_ref
            .visible_worktrees(cx)
            .map(|wt| wt.read(cx).abs_path().to_path_buf())
            .collect();

        let project_worktree_paths = active_worktree_paths.clone();

        let has_multiple_repositories = project_ref.repositories(cx).len() > 1;
        let repository = project_ref.active_repository(cx);

        let current_branch_name = repository.as_ref().and_then(|repo| {
            repo.read(cx)
                .branch
                .as_ref()
                .map(|branch| branch.name().to_string())
        });

        let all_worktrees_request = repository
            .clone()
            .map(|repository| repository.update(cx, |repository, _| repository.worktrees()));

        let default_branch_request = repository.clone().map(|repository| {
            repository.update(cx, |repository, _| repository.default_branch(true))
        });

        let initial_matches = vec![WorktreeEntry::CreateFromCurrentBranch];

        let delegate = WorktreePickerDelegate {
            matches: initial_matches,
            all_worktrees: Vec::new(),
            project_worktree_paths,
            selected_index: 0,
            project,
            workspace,
            focused_dock,
            current_branch_name,
            default_branch: None,
            has_multiple_repositories,
            focus_handle: cx.focus_handle(),
            show_footer,
            modifiers: Modifiers::default(),
            active_worktree_paths,
            hovered_delete_index: None,
            deleting_worktree_paths: HashSet::default(),
        };

        let picker = cx.new(|cx| {
            Picker::list(delegate, window, cx)
                .list_measure_all()
                .show_scrollbar(true)
                .embedded()
        });

        let picker_focus_handle = picker.focus_handle(cx);
        picker.update(cx, |picker, _| {
            picker.delegate.focus_handle = picker_focus_handle;
        });

        let mut subscriptions = Vec::new();

        {
            let picker_handle = picker.downgrade();
            cx.spawn_in(window, async move |_this, cx| {
                let all_worktrees: Vec<_> = match all_worktrees_request {
                    Some(req) => match req.await {
                        Ok(Ok(worktrees)) => {
                            worktrees.into_iter().filter(|wt| !wt.is_bare).collect()
                        }
                        Ok(Err(err)) => {
                            log::warn!("WorktreePicker: git worktree list failed: {err}");
                            return anyhow::Ok(());
                        }
                        Err(_) => {
                            log::warn!("WorktreePicker: worktree request was cancelled");
                            return anyhow::Ok(());
                        }
                    },
                    None => Vec::new(),
                };

                let default_branch = match default_branch_request {
                    Some(req) => req.await.ok().and_then(Result::ok).flatten(),
                    None => None,
                };

                picker_handle.update_in(cx, |picker, window, cx| {
                    picker.delegate.all_worktrees = all_worktrees;
                    picker.delegate.default_branch =
                        default_branch.and_then(|branch| RemoteBranchName::parse(&branch));
                    picker.delegate.refresh_project_worktree_paths(window, cx);
                    picker.refresh(window, cx);
                })?;

                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        }

        if let Some(repo) = &repository {
            let picker_entity = picker.downgrade();
            subscriptions.push(cx.subscribe_in(
                repo,
                window,
                move |_this, repo, event: &RepositoryEvent, window, cx| {
                    if matches!(event, RepositoryEvent::GitWorktreeListChanged) {
                        let worktrees_request = repo.update(cx, |repo, _| repo.worktrees());
                        let picker = picker_entity.clone();
                        cx.spawn_in(window, async move |_, cx| {
                            let all_worktrees: Vec<_> = worktrees_request
                                .await??
                                .into_iter()
                                .filter(|wt| !wt.is_bare)
                                .collect();
                            picker.update_in(cx, |picker, window, cx| {
                                picker.delegate.all_worktrees = all_worktrees;
                                picker.refresh(window, cx);
                            })?;
                            anyhow::Ok(())
                        })
                        .detach_and_log_err(cx);
                    }
                },
            ));
        }

        subscriptions.push(cx.subscribe(&picker, |_, _, _, cx| {
            cx.emit(DismissEvent);
        }));

        Self {
            focus_handle: picker.focus_handle(cx),
            picker,
            _subscriptions: subscriptions,
        }
    }

    fn handle_modifiers_changed(
        &mut self,
        ev: &ModifiersChangedEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picker.update(cx, |picker, cx| {
            picker.delegate.modifiers = ev.modifiers;
            cx.notify();
        });
    }
}

impl Focusable for WorktreePicker {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for WorktreePicker {}
impl EventEmitter<DismissEvent> for WorktreePicker {}

impl Render for WorktreePicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("WorktreePicker")
            .elevation_3(cx)
            .child(self.picker.clone())
            .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
            .on_mouse_down_out(cx.listener(|_, _, _, cx| {
                cx.emit(DismissEvent);
            }))
            .on_action(cx.listener(|this, _: &DeleteWorktree, window, cx| {
                this.picker.update(cx, |picker, cx| {
                    let ix = picker.delegate.selected_index;
                    picker.delegate.delete_worktree(ix, false, window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &ForceDeleteWorktree, window, cx| {
                this.picker.update(cx, |picker, cx| {
                    let ix = picker.delegate.selected_index;
                    picker.delegate.delete_worktree(ix, true, window, cx);
                });
            }))
    }
}

#[derive(Clone)]
enum WorktreeEntry {
    CreateFromCurrentBranch,
    CreateFromDefaultBranch {
        default_branch: RemoteBranchName,
    },
    Separator,
    SectionHeader(SharedString),
    Worktree {
        worktree: GitWorktree,
        positions: Vec<usize>,
    },
    CreateNamed {
        name: String,
        from_branch: Option<RemoteBranchName>,
        disabled_reason: Option<String>,
    },
}

struct WorktreePickerDelegate {
    matches: Vec<WorktreeEntry>,
    all_worktrees: Vec<GitWorktree>,
    project_worktree_paths: HashSet<PathBuf>,
    active_worktree_paths: HashSet<PathBuf>,
    selected_index: usize,
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    focused_dock: Option<DockPosition>,
    current_branch_name: Option<String>,
    default_branch: Option<RemoteBranchName>,
    has_multiple_repositories: bool,
    focus_handle: FocusHandle,
    show_footer: bool,
    modifiers: Modifiers,
    hovered_delete_index: Option<usize>,
    deleting_worktree_paths: HashSet<PathBuf>,
}

fn remove_worktree_command(path: &Path, force: bool) -> String {
    if force {
        format!("worktree remove --force {}", path.display())
    } else {
        format!("worktree remove {}", path.display())
    }
}

struct WorktreeRemoveForceDeletePrompt {
    required_error_substrings: &'static [&'static str],
    message: fn(&str) -> String,
}

impl WorktreeRemoveForceDeletePrompt {
    fn matches(&self, normalized_error_message: &str) -> bool {
        self.required_error_substrings
            .iter()
            .all(|substring| normalized_error_message.contains(substring))
    }
}

const WORKTREE_REMOVE_FORCE_DELETE_PROMPTS: &[WorktreeRemoveForceDeletePrompt] =
    &[WorktreeRemoveForceDeletePrompt {
        required_error_substrings: &[
            "contains modified or untracked files",
            "use --force to delete it",
        ],
        message: dirty_worktree_force_delete_prompt,
    }];

fn dirty_worktree_force_delete_prompt(display_name: &str) -> String {
    format!("Worktree \"{display_name}\" contains modified or untracked files. Force delete it?")
}

fn force_delete_prompt_for_worktree_remove_error(
    error: &anyhow::Error,
    display_name: &str,
) -> Option<String> {
    let normalized_error_message = error.to_string().to_lowercase();
    WORKTREE_REMOVE_FORCE_DELETE_PROMPTS
        .iter()
        .find(|prompt| prompt.matches(&normalized_error_message))
        .map(|prompt| (prompt.message)(display_name))
}

struct DeleteWorktreeTooltip {
    picker: WeakEntity<Picker<WorktreePickerDelegate>>,
    focus_handle: FocusHandle,
    delete_index: usize,
    _subscription: Subscription,
}

impl DeleteWorktreeTooltip {
    fn new(
        picker: Entity<Picker<WorktreePickerDelegate>>,
        focus_handle: FocusHandle,
        delete_index: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let subscription = cx.observe(&picker, |_, _, cx| cx.notify());
        Self {
            picker: picker.downgrade(),
            focus_handle,
            delete_index,
            _subscription: subscription,
        }
    }
}

impl Render for DeleteWorktreeTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let force_delete = self
            .picker
            .read_with(cx, |picker, _| {
                picker
                    .delegate
                    .is_force_delete_hovering_index(self.delete_index)
            })
            .unwrap_or(false);

        if force_delete {
            Tooltip::for_action_in(
                "Force Delete Worktree",
                &ForceDeleteWorktree,
                &self.focus_handle,
                cx,
            )
            .into_any_element()
        } else {
            Tooltip::with_meta_in(
                "Delete Worktree",
                Some(&DeleteWorktree),
                "Hold alt to force delete",
                &self.focus_handle,
                cx,
            )
            .into_any_element()
        }
    }
}

#[path = "worktree_picker/confirm.rs"]
mod confirm;
#[path = "worktree_picker/delegate_core.rs"]
mod delegate_core;
#[path = "worktree_picker/footer.rs"]
mod footer;
#[path = "worktree_picker/matching.rs"]
mod matching;
#[path = "worktree_picker/remote_open.rs"]
mod remote_open;
#[path = "worktree_picker/render_match.rs"]
mod render_match;
#[cfg(test)]
#[path = "worktree_picker/tests/mod.rs"]
mod tests;

pub use remote_open::open_remote_worktree;

impl PickerDelegate for WorktreePickerDelegate {
    type ListItem = AnyElement;

    fn name() -> &'static str {
        "worktree picker"
    }

    fn placeholder_text(&self, _window: &mut Window, _cx: &mut App) -> Arc<str> {
        "Select or type to create a worktree…".into()
    }

    fn editor_position(&self) -> PickerEditorPosition {
        PickerEditorPosition::Start
    }

    fn match_count(&self) -> usize {
        self.matches.len()
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

    fn can_select(&self, ix: usize, _window: &mut Window, _cx: &mut Context<Picker<Self>>) -> bool {
        !matches!(
            self.matches.get(ix),
            Some(WorktreeEntry::Separator | WorktreeEntry::SectionHeader(_))
        )
    }

    fn update_matches(
        &mut self,
        query: String,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        self.update_matches_impl(query, window, cx)
    }

    fn confirm(&mut self, secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        self.confirm_impl(secondary, window, cx);
    }

    fn dismissed(&mut self, _window: &mut Window, _cx: &mut Context<Picker<Self>>) {}

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        self.render_match_impl(ix, selected, window, cx)
    }

    fn render_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<AnyElement> {
        self.render_footer_impl(window, cx)
    }
}
