use super::*;

/// Classifies the project's visible worktrees into git-managed repositories
/// and non-git paths. Each unique repository is returned only once.
pub fn classify_worktrees(
    project: &Project,
    cx: &gpui::App,
) -> (Vec<Entity<Repository>>, Vec<PathBuf>) {
    let repositories = project.repositories(cx).clone();
    let mut git_repos: Vec<Entity<Repository>> = Vec::new();
    let mut non_git_paths: Vec<PathBuf> = Vec::new();
    let mut seen_repo_ids = HashSet::default();

    for worktree in project.visible_worktrees(cx) {
        let wt_path = worktree.read(cx).abs_path();

        let matching_repo = repositories
            .iter()
            .filter_map(|(id, repo)| {
                let work_dir = repo.read(cx).work_directory_abs_path.clone();
                if wt_path.starts_with(work_dir.as_ref()) {
                    Some((*id, repo.clone(), work_dir.as_ref().components().count()))
                } else {
                    None
                }
            })
            .max_by(
                |(left_id, _left_repo, left_depth), (right_id, _right_repo, right_depth)| {
                    left_depth
                        .cmp(right_depth)
                        .then_with(|| left_id.cmp(right_id))
                },
            );

        if let Some((id, repo, _)) = matching_repo {
            if seen_repo_ids.insert(id) {
                git_repos.push(repo);
            }
        } else {
            non_git_paths.push(wt_path.to_path_buf());
        }
    }

    (git_repos, non_git_paths)
}

/// Resolves a branch target into the ref the new worktree should be based on.
/// Returns `None` for `CurrentBranch`, meaning "use the current HEAD".
pub fn resolve_worktree_branch_target(branch_target: &NewWorktreeBranchTarget) -> Option<String> {
    match branch_target {
        NewWorktreeBranchTarget::CurrentBranch => None,
        NewWorktreeBranchTarget::ExistingBranch { name } => Some(name.clone()),
        NewWorktreeBranchTarget::RemoteBranch {
            remote_name,
            branch_name,
        } => Some(format!("refs/remotes/{remote_name}/{branch_name}")),
    }
}

fn remote_branch_to_fetch(branch_target: &NewWorktreeBranchTarget) -> Option<(&str, &str)> {
    match branch_target {
        NewWorktreeBranchTarget::RemoteBranch {
            remote_name,
            branch_name,
        } => Some((remote_name, branch_name)),
        NewWorktreeBranchTarget::CurrentBranch | NewWorktreeBranchTarget::ExistingBranch { .. } => {
            None
        }
    }
}

fn create_worktree_askpass_delegate(
    workspace: WeakEntity<Workspace>,
    operation: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> AskPassDelegate {
    let operation = operation.into();
    let window = window.window_handle();
    AskPassDelegate::new(&mut cx.to_async(), move |prompt, tx, cx| {
        window
            .update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.toggle_modal(window, cx, |window, cx| {
                        AskPassModal::new(operation.clone(), prompt.into(), tx, window, cx)
                    });
                })
            })
            .ok();
    })
}

async fn fetch_remote_for_worktree_base(
    git_repos: &[Entity<Repository>],
    remote_name: String,
    askpass_delegates: Vec<AskPassDelegate>,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<()> {
    if askpass_delegates.len() != git_repos.len() {
        return Err(anyhow!(
            "Unable to fetch {remote_name}: missing credential prompt delegate"
        ));
    }

    let fetches = cx.update(|_, cx| {
        git_repos
            .iter()
            .cloned()
            .zip(askpass_delegates)
            .map(|(repo, askpass)| {
                repo.update(cx, |repo, cx| {
                    repo.fetch(
                        FetchOptions::Remote(Remote {
                            name: remote_name.clone().into(),
                        }),
                        askpass,
                        cx,
                    )
                })
            })
            .collect::<Vec<_>>()
    })?;

    for fetch in futures::future::join_all(fetches).await {
        fetch??;
    }

    Ok(())
}

/// Kicks off an async git-worktree creation for each repository. Returns:
///
/// - `creation_infos`: a vec of `(repo, new_path, receiver)` tuples.
/// - `path_remapping`: `(old_work_dir, new_worktree_path)` pairs for remapping editor tabs.
///
/// Multiple entries in `git_repos` can be linked worktrees of the *same*
/// underlying repository (e.g. a project that has both the main checkout and
/// one of its linked worktrees open as separate Mav worktrees). Those entries
/// resolve to the same target path via [`Repository::path_for_new_linked_worktree`],
/// so we create the new worktree only once and remap every contributing
/// work directory onto it. Without this dedup, the second `git worktree add`
/// fails with "already exists".
fn start_worktree_creations(
    git_repos: &[Entity<Repository>],
    worktree_name: Option<String>,
    existing_worktree_names: &[String],
    existing_worktree_paths: &HashSet<PathBuf>,
    base_ref: Option<String>,
    worktree_directory_setting: &str,
    rng: &mut impl rand::Rng,
    cx: &mut gpui::App,
) -> anyhow::Result<(
    Vec<(
        Entity<Repository>,
        PathBuf,
        futures::channel::oneshot::Receiver<anyhow::Result<()>>,
    )>,
    Vec<(PathBuf, PathBuf)>,
)> {
    let mut creation_infos = Vec::new();
    let mut path_remapping = Vec::new();
    let mut scheduled_paths: HashSet<PathBuf> = HashSet::default();

    let worktree_name = worktree_name.unwrap_or_else(|| {
        let existing_refs: Vec<&str> = existing_worktree_names.iter().map(|s| s.as_str()).collect();
        worktree_names::generate_worktree_name(&existing_refs, rng)
            .unwrap_or_else(|| "worktree".to_string())
    });

    for repo in git_repos {
        let (work_dir, new_path, receiver) = repo.update(cx, |repo, _cx| {
            let new_path =
                repo.path_for_new_linked_worktree(&worktree_name, worktree_directory_setting)?;
            if existing_worktree_paths.contains(&new_path) {
                anyhow::bail!("A worktree already exists at {}", new_path.display());
            }
            let work_dir = repo.work_directory_abs_path.clone();
            // Only the first repo that resolves to a given target path
            // actually creates the worktree; subsequent linked worktrees of
            // the same repository just contribute a path remapping.
            let receiver = if scheduled_paths.contains(&new_path) {
                None
            } else {
                let target = git::repository::CreateWorktreeTarget::Detached {
                    base_sha: base_ref.clone(),
                };
                Some(repo.create_worktree(target, new_path.clone()))
            };
            anyhow::Ok((work_dir, new_path, receiver))
        })?;
        path_remapping.push((work_dir.to_path_buf(), new_path.clone()));
        if let Some(receiver) = receiver {
            scheduled_paths.insert(new_path.clone());
            creation_infos.push((repo.clone(), new_path, receiver));
        }
    }

    Ok((creation_infos, path_remapping))
}

/// Waits for every in-flight worktree creation to complete. If any
/// creation fails, all successfully-created worktrees are rolled back
/// (removed) so the project isn't left in a half-migrated state.
pub async fn await_and_rollback_on_failure(
    creation_infos: Vec<(
        Entity<Repository>,
        PathBuf,
        futures::channel::oneshot::Receiver<anyhow::Result<()>>,
    )>,
    fs: Arc<dyn Fs>,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<Vec<PathBuf>> {
    let mut created_paths: Vec<PathBuf> = Vec::new();
    let mut repos_and_paths: Vec<(Entity<Repository>, PathBuf)> = Vec::new();
    let mut first_error: Option<anyhow::Error> = None;

    for (repo, new_path, receiver) in creation_infos {
        repos_and_paths.push((repo.clone(), new_path.clone()));
        match receiver.await {
            Ok(Ok(())) => {
                created_paths.push(new_path);
            }
            Ok(Err(err)) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
            Err(_canceled) => {
                if first_error.is_none() {
                    first_error = Some(anyhow!("Worktree creation was canceled"));
                }
            }
        }
    }

    let Some(err) = first_error else {
        return Ok(created_paths);
    };

    // Rollback all attempted worktrees
    let mut rollback_futures = Vec::new();
    for (rollback_repo, rollback_path) in &repos_and_paths {
        let receiver = cx
            .update(|_, cx| {
                rollback_repo.update(cx, |repo, _cx| {
                    repo.remove_worktree(rollback_path.clone(), true)
                })
            })
            .ok();

        rollback_futures.push((rollback_path.clone(), receiver));
    }

    let mut rollback_failures: Vec<String> = Vec::new();
    for (path, receiver_opt) in rollback_futures {
        let mut git_remove_failed = false;

        if let Some(receiver) = receiver_opt {
            match receiver.await {
                Ok(Ok(())) => {}
                Ok(Err(rollback_err)) => {
                    log::error!(
                        "git worktree remove failed for {}: {rollback_err}",
                        path.display()
                    );
                    git_remove_failed = true;
                }
                Err(canceled) => {
                    log::error!(
                        "git worktree remove failed for {}: {canceled}",
                        path.display()
                    );
                    git_remove_failed = true;
                }
            }
        } else {
            log::error!(
                "failed to dispatch git worktree remove for {}",
                path.display()
            );
            git_remove_failed = true;
        }

        if git_remove_failed {
            if let Err(fs_err) = fs
                .remove_dir(
                    &path,
                    fs::RemoveOptions {
                        recursive: true,
                        ignore_if_not_exists: true,
                    },
                )
                .await
            {
                let msg = format!("{}: failed to remove directory: {fs_err}", path.display());
                log::error!("{}", msg);
                rollback_failures.push(msg);
            }
        }
    }
    let mut error_message = format!("Failed to create worktree: {err}");
    if !rollback_failures.is_empty() {
        error_message.push_str("\n\nFailed to clean up: ");
        error_message.push_str(&rollback_failures.join(", "));
    }
    Err(anyhow!(error_message))
}

/// Propagates worktree trust from the source workspace to the new workspace.
/// If the source project's worktrees are all trusted, the new worktree paths
/// will also be trusted automatically.
fn maybe_propagate_worktree_trust(
    source_workspace: &WeakEntity<Workspace>,
    new_workspace: &Entity<Workspace>,
    paths: &[PathBuf],
    cx: &mut AsyncWindowContext,
) {
    cx.update(|_, cx| {
        if ProjectSettings::get_global(cx).session.trust_all_worktrees {
            return;
        }
        let source_is_trusted = source_workspace
            .upgrade()
            .map(|workspace| {
                let source_worktree_store = workspace.read(cx).project().read(cx).worktree_store();
                !TrustedWorktrees::has_restricted_worktrees(&source_worktree_store, cx)
            })
            .unwrap_or(false);

        if !source_is_trusted {
            return;
        }

        let worktree_store = new_workspace.read(cx).project().read(cx).worktree_store();
        let paths_to_trust: HashSet<_> = paths
            .iter()
            .filter_map(|path| {
                let (worktree, _) = worktree_store.read(cx).find_worktree(path, cx)?;
                Some(PathTrust::Worktree(worktree.read(cx).id()))
            })
            .collect();

        if !paths_to_trust.is_empty() {
            if let Some(trusted_store) = TrustedWorktrees::try_get_global(cx) {
                trusted_store.update(cx, |store, cx| {
                    store.trust(&worktree_store, paths_to_trust, cx);
                });
            }
        }
    })
    .ok();

    // After trust propagation, refresh the security modal on the new workspace
    // so it dismisses itself if there are no more restricted worktrees.
    cx.update(|window, cx| {
        new_workspace.update(cx, |workspace, cx| {
            workspace.show_worktree_trust_security_modal(false, window, cx);
        });
    })
    .ok();
}
