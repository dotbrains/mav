use super::*;

pub async fn remove_root(root: RootPlan, cx: &mut AsyncApp) -> Result<()> {
    verify_created_by_mav(&root, cx).await?;

    let release_tasks: Vec<_> = root
        .affected_projects
        .iter()
        .map(|affected| {
            let project = affected.project.clone();
            let worktree_id = affected.worktree_id;
            project.update(cx, |project, cx| {
                let wait = project.wait_for_worktree_release(worktree_id, cx);
                project.remove_worktree(worktree_id, cx);
                wait
            })
        })
        .collect();

    if let Err(error) = remove_root_after_worktree_removal(&root, release_tasks, cx).await {
        rollback_root(&root, cx).await;
        return Err(error);
    }

    // The worktree is gone, so its registry record is now stale. If the
    // user later creates a new worktree at the same path outside Mav, a
    // leftover record would only be saved by the creation time check, so
    // remove it eagerly.
    cx.update(|cx| {
        git_ui::created_worktrees::forget_created_worktree(
            &root.root_path,
            root.remote_connection.as_ref(),
            cx,
        )
    })
    .await
    .log_err();

    Ok(())
}

/// Confirms that the worktree on disk is still the one Mav created, by
/// comparing the creation time of its git metadata directory against the
/// time recorded when Mav created it.
///
/// Outcomes:
/// - Creation time matches the recorded one: proceed.
/// - Worktree directory no longer exists: proceed — there is nothing on
///   disk to protect, and removal will only clean up git metadata.
/// - Creation time differs: the worktree was removed and recreated outside
///   Mav. The registry record is removed (so subsequent archival attempts
///   skip the worktree entirely) and an error is returned so the caller
///   leaves the directory untouched.
/// - Creation time cannot be read: return an error but keep the record,
///   since the failure may be transient (e.g. a disconnected remote).
async fn verify_created_by_mav(root: &RootPlan, cx: &mut AsyncApp) -> Result<()> {
    let receiver = root.worktree_repo.update(cx, |repo: &mut Repository, _cx| {
        repo.worktree_created_at(root.root_path.clone())
    });
    let created_at = receiver
        .await
        .map_err(|_| anyhow!("worktree creation time check was canceled"))?
        .with_context(|| {
            format!(
                "refusing to delete worktree at {}: failed to verify that Mav created it",
                root.root_path.display()
            )
        })?;

    match created_at {
        None => Ok(()),
        Some(created_at) if created_at == root.recorded_created_at => Ok(()),
        Some(_) => {
            cx.update(|cx| {
                git_ui::created_worktrees::forget_created_worktree(
                    &root.root_path,
                    root.remote_connection.as_ref(),
                    cx,
                )
            })
            .await
            .log_err();
            Err(anyhow!(
                "refusing to delete worktree at {}: it is not the worktree Mav created \
                 (it was likely removed and recreated outside Mav)",
                root.root_path.display()
            ))
        }
    }
}

async fn remove_root_after_worktree_removal(
    root: &RootPlan,
    release_tasks: Vec<Task<Result<()>>>,
    cx: &mut AsyncApp,
) -> Result<()> {
    for task in release_tasks {
        if let Err(error) = task.await {
            log::error!("Failed waiting for worktree release: {error:#}");
        }
    }

    let (repo, project) =
        find_or_create_repository(&root.main_repo_path, root.remote_connection.as_ref(), cx)
            .await?;

    // `Repository::remove_worktree` with `force = true` deletes the working
    // directory before running `git worktree remove --force`, so there's no
    // need to touch the filesystem here. For remote projects that cleanup
    // runs on the headless server via the `GitRemoveWorktree` RPC, which is
    // the only code path with access to the remote machine's filesystem.
    let receiver = repo.update(cx, |repo: &mut Repository, _cx| {
        repo.remove_worktree(root.root_path.clone(), true)
    });
    let result = receiver
        .await
        .map_err(|_| anyhow!("git worktree metadata cleanup was canceled"))?;
    // `project` may be a live workspace project or a temporary one created
    // by `find_or_create_repository`. In the temporary case we must keep it
    // alive until the repo removes the worktree
    drop(project);
    result.context("git worktree metadata cleanup failed")?;
    Ok(())
}

/// Finds a live `Repository` entity for the given path, or creates a temporary
/// project to obtain one.
///
/// `Repository` entities can only be obtained through a `Project` because
/// `GitStore` (which creates and manages `Repository` entities) is owned by
/// `Project`. When no open workspace contains the repo we need, we spin up a
/// headless project just to get a `Repository` handle. For local paths this is
/// a `Project::local`; for remote paths we build a `Project::remote` through
/// the connection pool (reusing the existing SSH transport), which requires
/// the caller to pass the matching `RemoteConnectionOptions` so we only match
/// and fall back onto projects that share the same remote identity. The
/// caller keeps the returned `Entity<Project>` alive for the duration of the
/// git operations, then drops it.
///
/// Future improvement: decoupling `GitStore` from `Project` so that
/// `Repository` entities can be created standalone would eliminate this
/// temporary-project workaround.
async fn find_or_create_repository(
    repo_path: &Path,
    remote_connection: Option<&RemoteConnectionOptions>,
    cx: &mut AsyncApp,
) -> Result<(Entity<Repository>, Entity<Project>)> {
    let repo_path_owned = repo_path.to_path_buf();
    let remote_connection_owned = remote_connection.cloned();

    // First, try to find a live repository in any open workspace whose
    // remote connection matches (so a local `/project` and a remote
    // `/project` are not confused).
    let live_repo = cx.update(|cx| {
        all_open_workspaces(cx)
            .into_iter()
            .filter_map(|workspace| {
                let project = workspace.read(cx).project().clone();
                let project_connection = project.read(cx).remote_connection_options(cx);
                if !same_remote_connection_identity(
                    project_connection.as_ref(),
                    remote_connection_owned.as_ref(),
                ) {
                    return None;
                }
                Some((
                    project
                        .read(cx)
                        .repositories(cx)
                        .values()
                        .find(|repo| {
                            repo.read(cx).snapshot().work_directory_abs_path.as_ref()
                                == repo_path_owned.as_path()
                        })
                        .cloned()?,
                    project.clone(),
                ))
            })
            .next()
    });

    if let Some((repo, project)) = live_repo {
        return Ok((repo, project));
    }

    let app_state =
        current_app_state(cx).context("no app state available for temporary project")?;

    // For remote paths, create a fresh RemoteClient through the connection
    // pool (reusing the existing SSH transport) and build a temporary
    // remote project. Each RemoteClient gets its own server-side headless
    // project, so there are no RPC routing conflicts with other projects.
    let temp_project = if let Some(connection) = remote_connection_owned {
        let remote_client = cx
            .update(|cx| {
                if !remote::has_active_connection(&connection, cx) {
                    anyhow::bail!("cannot open repository on disconnected remote machine");
                }
                Ok(remote_connection::connect_reusing_pool(connection, cx))
            })?
            .await?
            .context("remote connection was canceled")?;

        cx.update(|cx| {
            Project::remote(
                remote_client,
                app_state.client.clone(),
                app_state.node_runtime.clone(),
                app_state.user_store.clone(),
                app_state.languages.clone(),
                app_state.fs.clone(),
                false,
                cx,
            )
        })
    } else {
        cx.update(|cx| {
            Project::local(
                app_state.client.clone(),
                app_state.node_runtime.clone(),
                app_state.user_store.clone(),
                app_state.languages.clone(),
                app_state.fs.clone(),
                None,
                LocalProjectFlags::default(),
                cx,
            )
        })
    };

    let repo_path_for_worktree = repo_path.to_path_buf();
    let create_worktree = temp_project.update(cx, |project, cx| {
        project.create_worktree(repo_path_for_worktree, true, cx)
    });
    let _worktree = create_worktree.await?;
    let initial_scan = temp_project.read_with(cx, |project, cx| project.wait_for_initial_scan(cx));
    initial_scan.await;

    let repo_path_for_find = repo_path.to_path_buf();
    let repo = temp_project
        .update(cx, |project, cx| {
            project
                .repositories(cx)
                .values()
                .find(|repo| {
                    repo.read(cx).snapshot().work_directory_abs_path.as_ref()
                        == repo_path_for_find.as_path()
                })
                .cloned()
        })
        .context("failed to resolve temporary repository handle")?;

    let barrier = repo.update(cx, |repo: &mut Repository, _cx| repo.barrier());
    barrier
        .await
        .map_err(|_| anyhow!("temporary repository barrier canceled"))?;
    Ok((repo, temp_project))
}

/// Re-adds the worktree to every affected project after a failed
/// [`remove_root`].
async fn rollback_root(root: &RootPlan, cx: &mut AsyncApp) {
    for affected in &root.affected_projects {
        let task = affected.project.update(cx, |project, cx| {
            project.create_worktree(root.root_path.clone(), true, cx)
        });
        task.await.log_err();
    }
}
