use super::*;

/// Saves the worktree's full git state so it can be restored later.
///
/// This creates two detached commits (via [`create_archive_checkpoint`] on
/// the `GitRepository` trait) that capture the staged and unstaged state
/// without moving any branch ref. The commits are:
///   - "WIP staged": a tree matching the current index, parented on HEAD
///   - "WIP unstaged": a tree with all files (including untracked),
///     parented on the staged commit
///
/// After creating the commits, this function:
///   1. Records the commit SHAs, branch name, and paths in a DB record.
///   2. Links every thread referencing this worktree to that record.
///   3. Creates a git ref on the main repo to prevent GC of the commits.
///
/// On success, returns the archived worktree DB row ID for rollback.
pub async fn persist_worktree_state(root: &RootPlan, cx: &mut AsyncApp) -> Result<i64> {
    let worktree_repo = root.worktree_repo.clone();

    let original_commit_hash = worktree_repo
        .update(cx, |repo, _cx| repo.head_sha())
        .await
        .map_err(|_| anyhow!("head_sha canceled"))?
        .context("failed to read original HEAD SHA")?
        .context("HEAD SHA is None")?;

    // Create two detached WIP commits without moving the branch.
    let checkpoint_rx = worktree_repo.update(cx, |repo, _cx| repo.create_archive_checkpoint());
    let (staged_commit_hash, unstaged_commit_hash) = checkpoint_rx
        .await
        .map_err(|_| anyhow!("create_archive_checkpoint canceled"))?
        .context("failed to create archive checkpoint")?;

    // Create DB record
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));
    let worktree_path_str = root.root_path.to_string_lossy().to_string();
    let main_repo_path_str = root.main_repo_path.to_string_lossy().to_string();
    let branch_name = root.branch_name.clone().or_else(|| {
        worktree_repo.read_with(cx, |repo, _cx| {
            repo.snapshot()
                .branch
                .as_ref()
                .map(|branch| branch.name().to_string())
        })
    });

    let db_result = store
        .read_with(cx, |store, cx| {
            store.create_archived_worktree(
                worktree_path_str.clone(),
                main_repo_path_str.clone(),
                branch_name.clone(),
                staged_commit_hash.clone(),
                unstaged_commit_hash.clone(),
                original_commit_hash.clone(),
                cx,
            )
        })
        .await
        .context("failed to create archived worktree DB record");
    let archived_worktree_id = match db_result {
        Ok(id) => id,
        Err(error) => {
            return Err(error);
        }
    };

    // Link all threads on this worktree to the archived record
    let thread_ids: Vec<ThreadId> = store.read_with(cx, |store, _cx| {
        store
            .entries()
            .filter(|thread| {
                thread
                    .folder_paths()
                    .paths()
                    .iter()
                    .any(|p| p.as_path() == root.root_path)
            })
            .map(|thread| thread.thread_id)
            .collect()
    });

    for thread_id in &thread_ids {
        let link_result = store
            .read_with(cx, |store, cx| {
                store.link_thread_to_archived_worktree(*thread_id, archived_worktree_id, cx)
            })
            .await;
        if let Err(error) = link_result {
            if let Err(delete_error) = store
                .read_with(cx, |store, cx| {
                    store.delete_archived_worktree(archived_worktree_id, cx)
                })
                .await
            {
                log::error!(
                    "Failed to delete archived worktree DB record during link rollback: \
                     {delete_error:#}"
                );
            }
            return Err(error.context("failed to link thread to archived worktree"));
        }
    }

    // Create git ref on main repo to prevent GC of the detached commits.
    // This is fatal: without the ref, git gc will eventually collect the
    // WIP commits and a later restore will silently fail.
    let ref_name = archived_worktree_ref_name(archived_worktree_id);
    let (main_repo, _temp_project) =
        find_or_create_repository(&root.main_repo_path, root.remote_connection.as_ref(), cx)
            .await
            .context("could not open main repo to create archive ref")?;
    let rx = main_repo.update(cx, |repo, _cx| {
        repo.update_ref(ref_name.clone(), unstaged_commit_hash.clone())
    });
    rx.await
        .map_err(|_| anyhow!("update_ref canceled"))
        .and_then(|r| r)
        .with_context(|| format!("failed to create ref {ref_name} on main repo"))?;
    // See note in `remove_root_after_worktree_removal`: this may be a live
    // or temporary project; dropping only matters in the temporary case.
    drop(_temp_project);

    Ok(archived_worktree_id)
}

/// Undoes a successful [`persist_worktree_state`] by deleting the git ref
/// on the main repo and removing the DB record. Since the WIP commits are
/// detached (they don't move any branch), no git reset is needed — the
/// commits will be garbage-collected once the ref is removed.
pub async fn rollback_persist(archived_worktree_id: i64, root: &RootPlan, cx: &mut AsyncApp) {
    // Delete the git ref on main repo
    if let Ok((main_repo, _temp_project)) =
        find_or_create_repository(&root.main_repo_path, root.remote_connection.as_ref(), cx).await
    {
        let ref_name = archived_worktree_ref_name(archived_worktree_id);
        let rx = main_repo.update(cx, |repo, _cx| repo.delete_ref(ref_name));
        rx.await.ok().and_then(|r| r.log_err());
        // See note in `remove_root_after_worktree_removal`: this may be a
        // live or temporary project; dropping only matters in the temporary
        // case.
        drop(_temp_project);
    }

    // Delete the DB record
    let store = cx.update(|cx| ThreadMetadataStore::global(cx));
    if let Err(error) = store
        .read_with(cx, |store, cx| {
            store.delete_archived_worktree(archived_worktree_id, cx)
        })
        .await
    {
        log::error!("Failed to delete archived worktree DB record during rollback: {error:#}");
    }
}
