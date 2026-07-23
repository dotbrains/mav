use super::*;

/// Handles the `CreateWorktree` action generically, without any agent panel involvement.
/// Creates a new git worktree, opens the workspace, restores layout and files.
/// Errors are surfaced to the user via toasts; the new workspace handle is
/// discarded. Use [`create_worktree_workspace`] when you need the resulting
/// workspace (e.g., the `create_thread` agent tool spawns a thread in it).
pub fn handle_create_worktree(
    workspace: &mut Workspace,
    action: &mav_actions::CreateWorktree,
    window: &mut gpui::Window,
    fallback_focused_dock: Option<DockPosition>,
    cx: &mut gpui::Context<Workspace>,
) {
    let task = create_worktree_workspace_inner(
        workspace,
        action,
        window,
        fallback_focused_dock,
        RemoteBranchFetchMode::Fetch,
        // The user explicitly asked to create a worktree, so foreground it.
        true,
        cx,
    );
    task.detach_and_log_err(cx);
}

/// Outcome of [`create_worktree_workspace`].
pub struct CreatedWorktreeWorkspace {
    /// The newly opened workspace.
    pub workspace: Entity<Workspace>,
    /// True when the project contained more than one Mav worktree backed by
    /// the same underlying git repository, so they were consolidated into a
    /// single new worktree (they resolve to the same target path). Callers
    /// that care — like the `create_thread` agent tool — can use this to warn
    /// that the result may not reflect every source worktree's state.
    pub consolidated_worktrees: bool,
}

/// Same as [`handle_create_worktree`], but returns a `Task` that resolves to
/// the new workspace once worktree creation and post-open setup are
/// complete. The caller receives errors as `Result`s and is expected to
/// handle them. Note that a small set of early failures (no git repositories,
/// disconnected remote, mid-creation `git fetch` failure) still surface a
/// toast on the source workspace so the user understands why the action
/// didn't take effect; the same error is also returned to the caller.
///
/// Used by the `create_thread` agent tool to spawn a sibling thread inside
/// the newly-opened workspace.
///
/// The new workspace is opened in the **background** (added as a retained
/// tab without switching to it or moving focus), and it's a clean checkout
/// rather than inheriting the source workspace's open files and dock layout.
/// This mirrors how the agent's non-worktree threads are created in the
/// background rather than yanking the user away from what they're doing.
pub fn create_worktree_workspace(
    workspace: &mut Workspace,
    action: &mav_actions::CreateWorktree,
    window: &mut gpui::Window,
    fallback_focused_dock: Option<DockPosition>,
    cx: &mut gpui::Context<Workspace>,
) -> Task<anyhow::Result<CreatedWorktreeWorkspace>> {
    create_worktree_workspace_inner(
        workspace,
        action,
        window,
        fallback_focused_dock,
        RemoteBranchFetchMode::Fetch,
        // Agent-created worktree workspaces open in the background.
        false,
        cx,
    )
}

fn create_worktree_workspace_inner(
    workspace: &mut Workspace,
    action: &mav_actions::CreateWorktree,
    window: &mut gpui::Window,
    fallback_focused_dock: Option<DockPosition>,
    remote_branch_fetch_mode: RemoteBranchFetchMode,
    activate: bool,
    cx: &mut gpui::Context<Workspace>,
) -> Task<anyhow::Result<CreatedWorktreeWorkspace>> {
    let project = workspace.project().clone();

    if project.read(cx).repositories(cx).is_empty() {
        return Task::ready(Err(anyhow!(
            "create_worktree: no git repository in the project"
        )));
    }
    if project.read(cx).is_via_collab() {
        return Task::ready(Err(anyhow!(
            "create_worktree: not supported in collab projects"
        )));
    }

    // Guard against concurrent creation. We treat a concurrent creation as
    // a hard error here so the caller can surface it; the user-facing
    // wrapper [`handle_create_worktree`] swallows the error via
    // `detach_and_log_err`, matching the pre-existing silent return.
    if workspace.active_worktree_creation().label.is_some() {
        return Task::ready(Err(anyhow!("A worktree creation is already in progress")));
    }

    let previous_state =
        workspace.capture_state_for_worktree_switch(window, fallback_focused_dock, cx);
    let workspace_handle = workspace.weak_handle();
    let window_handle = window.window_handle().downcast::<MultiWorkspace>();
    let remote_connection_options = project.read(cx).remote_connection_options(cx);

    let (git_repos, non_git_paths) = classify_worktrees(project.read(cx), cx);

    if git_repos.is_empty() {
        let toast_workspace = cx.entity();
        show_error_toast(
            toast_workspace,
            "worktree create",
            anyhow!("No git repositories found in the project"),
            cx,
        );
        return Task::ready(Err(anyhow!("No git repositories found in the project")));
    }

    if remote_connection_options.is_some() {
        let is_disconnected = project
            .read(cx)
            .remote_client()
            .is_some_and(|client| client.read(cx).is_disconnected());
        if is_disconnected {
            let toast_workspace = cx.entity();
            show_error_toast(
                toast_workspace,
                "worktree create",
                anyhow!("Cannot create worktree: remote connection is not active"),
                cx,
            );
            return Task::ready(Err(anyhow!(
                "Cannot create worktree: remote connection is not active"
            )));
        }
    }

    let worktree_name = action.worktree_name.clone();
    let branch_target = action.branch_target.clone();
    let fetch_askpass_delegates = if remote_branch_fetch_mode.should_fetch() {
        remote_branch_to_fetch(&branch_target)
            .map(|(remote_name, _branch_name)| {
                git_repos
                    .iter()
                    .map(|_| {
                        create_worktree_askpass_delegate(
                            workspace_handle.clone(),
                            format!("git fetch {remote_name}"),
                            window,
                            cx,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let display_name: SharedString = worktree_name
        .as_deref()
        .unwrap_or("worktree")
        .to_string()
        .into();

    workspace.set_active_worktree_creation(Some(display_name), false, cx);

    cx.spawn_in(window, async move |_workspace_entity, mut cx| {
        let result = do_create_worktree(
            git_repos,
            non_git_paths,
            worktree_name.clone(),
            branch_target.clone(),
            fetch_askpass_delegates,
            remote_branch_fetch_mode,
            previous_state,
            workspace_handle.clone(),
            window_handle,
            remote_connection_options,
            activate,
            &mut cx,
        )
        .await;

        if let Err(err) = &result {
            log::error!("Failed to create worktree: {err}");
            workspace_handle
                .update(cx, |workspace, cx| {
                    workspace.set_active_worktree_creation(None, false, cx);
                    if let Some(fetch_error) = err.downcast_ref::<WorktreeFetchError>() {
                        let toast = cx.new(|cx| {
                            WorktreeFetchFailedToast::new(
                                workspace.weak_handle(),
                                worktree_name,
                                branch_target,
                                fallback_focused_dock,
                                fetch_error,
                                cx,
                            )
                        });
                        workspace.toggle_status_toast(toast, cx);
                    } else {
                        show_error_toast(cx.entity(), "worktree create", anyhow!("{err:#}"), cx);
                    }
                })
                .ok();
        }

        result
    })
}
