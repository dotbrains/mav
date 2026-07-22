use super::*;

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
