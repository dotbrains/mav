use super::*;

/// Restores a previously archived worktree back to disk from its DB record.
///
/// Creates the git worktree at the original commit (the branch never moved
/// during archival since WIP commits are detached), switches to the branch,
/// then uses [`restore_archive_checkpoint`] to reconstruct the staged/
/// unstaged state from the WIP commit trees.
pub async fn restore_worktree_via_git(
    row: &ArchivedGitWorktree,
    remote_connection: Option<&RemoteConnectionOptions>,
    cx: &mut AsyncApp,
) -> Result<PathBuf> {
    let (main_repo, _temp_project) =
        find_or_create_repository(&row.main_repo_path, remote_connection, cx).await?;

    let worktree_path = &row.worktree_path;
    let app_state = current_app_state(cx).context("no app state available")?;
    let already_exists = app_state.fs.metadata(worktree_path).await?.is_some();

    let created_new_worktree = if already_exists {
        let is_git_worktree =
            resolve_git_worktree_to_main_repo(app_state.fs.as_ref(), worktree_path)
                .await
                .is_some();

        if !is_git_worktree {
            let rx = main_repo.update(cx, |repo, _cx| repo.repair_worktrees());
            rx.await
                .map_err(|_| anyhow!("worktree repair was canceled"))?
                .context("failed to repair worktrees")?;
        }
        false
    } else {
        // Create worktree at the original commit — the branch still points
        // here because archival used detached commits.
        let rx = main_repo.update(cx, |repo, _cx| {
            repo.create_worktree_detached(worktree_path.clone(), row.original_commit_hash.clone())
        });
        rx.await
            .map_err(|_| anyhow!("worktree creation was canceled"))?
            .context("failed to create worktree")?;
        true
    };

    let (wt_repo, _temp_wt_project) =
        match find_or_create_repository(worktree_path, remote_connection, cx).await {
            Ok(result) => result,
            Err(error) => {
                remove_new_worktree_on_error(created_new_worktree, &main_repo, worktree_path, cx)
                    .await;
                return Err(error);
            }
        };

    if let Some(branch_name) = &row.branch_name {
        // Attempt to check out the branch the worktree was previously on.
        let checkout_result = wt_repo
            .update(cx, |repo, _cx| repo.change_branch(branch_name.clone()))
            .await;

        match checkout_result.map_err(|e| anyhow!("{e}")).flatten() {
            Ok(()) => {
                // Branch checkout succeeded. Check whether the branch has moved since
                // we archived the worktree, by comparing HEAD to the expected SHA.
                let head_sha = wt_repo
                    .update(cx, |repo, _cx| repo.head_sha())
                    .await
                    .map_err(|e| anyhow!("{e}"))
                    .and_then(|r| r);

                match head_sha {
                    Ok(Some(sha)) if sha == row.original_commit_hash => {
                        // Branch still points at the original commit; we're all done!
                    }
                    Ok(Some(sha)) => {
                        // The branch has moved. We don't want to restore the worktree to
                        // a different filesystem state, so checkout the original commit
                        // in detached HEAD state.
                        log::info!(
                            "Branch '{branch_name}' has moved since archival (now at {sha}); \
                             restoring worktree in detached HEAD at {}",
                            row.original_commit_hash
                        );
                        let detach_result = main_repo
                            .update(cx, |repo, _cx| {
                                repo.checkout_branch_in_worktree(
                                    row.original_commit_hash.clone(),
                                    row.worktree_path.clone(),
                                    false,
                                )
                            })
                            .await;

                        if let Err(error) = detach_result.map_err(|e| anyhow!("{e}")).flatten() {
                            log::warn!(
                                "Failed to detach HEAD at {}: {error:#}",
                                row.original_commit_hash
                            );
                        }
                    }
                    Ok(None) => {
                        log::warn!(
                            "head_sha unexpectedly returned None after checking out \"{branch_name}\"; \
                             proceeding in current HEAD state."
                        );
                    }
                    Err(error) => {
                        log::warn!(
                            "Failed to read HEAD after checking out \"{branch_name}\": {error:#}"
                        );
                    }
                }
            }
            Err(checkout_error) => {
                // We weren't able to check out the branch, most likely because it was deleted.
                // This is fine; users will often delete old branches! We'll try to recreate it.
                log::debug!(
                    "change_branch('{branch_name}') failed: {checkout_error:#}, trying create_branch"
                );
                let create_result = wt_repo
                    .update(cx, |repo, _cx| {
                        repo.create_branch(branch_name.clone(), None)
                    })
                    .await;

                if let Err(error) = create_result.map_err(|e| anyhow!("{e}")).flatten() {
                    log::warn!(
                        "Failed to create branch '{branch_name}': {error:#}; \
                         restored worktree will be in detached HEAD state."
                    );
                }
            }
        }
    }

    // Restore the staged/unstaged state from the WIP commit trees.
    // read-tree --reset -u applies the unstaged tree (including deletions)
    // to the working directory, then a bare read-tree sets the index to
    // the staged tree without touching the working directory.
    let restore_rx = wt_repo.update(cx, |repo, _cx| {
        repo.restore_archive_checkpoint(
            row.staged_commit_hash.clone(),
            row.unstaged_commit_hash.clone(),
        )
    });
    if let Err(error) = restore_rx
        .await
        .map_err(|_| anyhow!("restore_archive_checkpoint canceled"))
        .and_then(|r| r)
    {
        remove_new_worktree_on_error(created_new_worktree, &main_repo, worktree_path, cx).await;
        return Err(error.context("failed to restore archive checkpoint"));
    }

    if created_new_worktree {
        // Re-register the restored worktree as Mav-created so it can be
        // archived again later.
        git_ui::created_worktrees::record_created_worktree_for_repo(
            &wt_repo,
            worktree_path,
            remote_connection,
            cx,
        )
        .await;
    }

    Ok(worktree_path.clone())
}

async fn remove_new_worktree_on_error(
    created_new_worktree: bool,
    main_repo: &Entity<Repository>,
    worktree_path: &PathBuf,
    cx: &mut AsyncApp,
) {
    if created_new_worktree {
        let rx = main_repo.update(cx, |repo, _cx| {
            repo.remove_worktree(worktree_path.clone(), true)
        });
        rx.await.ok().and_then(|r| r.log_err());
    }
}
