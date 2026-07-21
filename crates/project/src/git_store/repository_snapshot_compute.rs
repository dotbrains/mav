use super::*;

/// This snapshot computes the repository state on the foreground thread while
/// running the git commands on the background thread. We update branch, head,
/// remotes, and worktrees first so the UI can react sooner, then compute file
/// state and emit those events immediately after.
pub(super) async fn compute_snapshot(
    this: Entity<Repository>,
    backend: Arc<dyn GitRepository>,
    cx: &mut AsyncApp,
) -> RepositorySnapshot {
    log::debug!("starting compute snapshot");

    let (id, work_directory_abs_path, prev_snapshot) = this.update(cx, |this, _| {
        this.paths_needing_status_update.clear();
        (
            this.id,
            this.work_directory_abs_path.clone(),
            this.snapshot.clone(),
        )
    });

    let branches_future = {
        let backend = backend.clone();
        async move { backend.branches().await.log_err().unwrap_or_default() }
    };
    let head_commit_future = {
        let backend = backend.clone();
        async move { backend.show("HEAD".to_string()).await.ok() }
    };
    let worktrees_future = {
        let backend = backend.clone();
        async move { backend.worktrees().await.log_err().unwrap_or_default() }
    };
    let (branches, head_commit, all_worktrees) =
        futures::future::join3(branches_future, head_commit_future, worktrees_future).await;
    log::debug!("fetched branches, head commit, worktrees");

    let BranchesScanResult {
        branches,
        error: branch_list_error,
    } = branches;
    let branch = branches.iter().find(|branch| branch.is_head).cloned();
    let branch_list: Arc<[Branch]> = branches.into();

    let linked_worktrees: Arc<[GitWorktree]> = all_worktrees
        .into_iter()
        .filter(|wt| wt.path != *work_directory_abs_path)
        .collect();

    let mut remote_urls = backend.remote_urls().await;
    let remote_origin_url = remote_urls.remove("origin");
    let remote_upstream_url = remote_urls.remove("upstream");

    log::debug!("fetched remotes");

    let snapshot = this.update(cx, |this, cx| {
        let head_changed =
            branch != this.snapshot.branch || head_commit != this.snapshot.head_commit;
        let branch_list_changed = *branch_list != *this.snapshot.branch_list;
        let branch_list_error_changed = branch_list_error != this.snapshot.branch_list_error;
        let worktrees_changed = *linked_worktrees != *this.snapshot.linked_worktrees;

        this.snapshot = RepositorySnapshot {
            id,
            work_directory_abs_path,
            branch,
            branch_list: branch_list.clone(),
            branch_list_error,
            head_commit,
            remote_origin_url,
            remote_upstream_url,
            linked_worktrees,
            scan_id: prev_snapshot.scan_id + 1,
            ..prev_snapshot
        };

        if head_changed {
            cx.emit(RepositoryEvent::HeadChanged);
        }

        if branch_list_changed || branch_list_error_changed {
            cx.emit(RepositoryEvent::BranchListChanged);
        }

        if worktrees_changed {
            cx.emit(RepositoryEvent::GitWorktreeListChanged);
        }

        this.snapshot.clone()
    });

    let statuses_future = {
        let backend = backend.clone();
        async move {
            backend
                .status(&[RepoPath::from_rel_path(
                    &RelPath::new(".".as_ref(), PathStyle::local()).unwrap(),
                )])
                .await
                .log_err()
                .unwrap_or_default()
        }
    };
    let diff_stat_future = {
        let snapshot = snapshot.clone();
        let backend = backend.clone();
        async move {
            if snapshot.head_commit.is_some() {
                backend.diff_stat(&[]).await.log_err().unwrap_or_default()
            } else {
                Default::default()
            }
        }
    };
    let stash_entries_future = {
        let backend = backend.clone();
        async move { backend.stash_entries().await.log_err().unwrap_or_default() }
    };

    let (statuses, diff_stats, stash_entries) =
        futures::future::join3(statuses_future, diff_stat_future, stash_entries_future).await;
    log::debug!("fetched statuses, diff stats, stash entries");

    let diff_stat_map: HashMap<&RepoPath, DiffStat> =
        diff_stats.entries.iter().map(|(p, s)| (p, *s)).collect();
    let mut conflicted_paths = Vec::new();
    let statuses_by_path = SumTree::from_iter(
        statuses.entries.iter().map(|(repo_path, status)| {
            if status.is_conflicted() {
                conflicted_paths.push(repo_path.clone());
            }
            StatusEntry {
                repo_path: repo_path.clone(),
                status: *status,
                diff_stat: diff_stat_map.get(repo_path).copied(),
            }
        }),
        (),
    );

    let (merge_details, conflicts_changed) = cx
        .background_spawn({
            let backend = backend.clone();
            let mut merge_details = snapshot.merge.clone();
            async move {
                let conflicts_changed = merge_details.update(&backend, conflicted_paths).await;
                (merge_details, conflicts_changed)
            }
        })
        .await;
    log::debug!("new merge details: {merge_details:?}");

    this.update(cx, |this, cx| {
        if conflicts_changed || statuses_by_path != this.snapshot.statuses_by_path {
            cx.emit(RepositoryEvent::StatusesChanged);
        }
        if stash_entries != this.snapshot.stash_entries {
            cx.emit(RepositoryEvent::StashEntriesChanged);
        }

        this.snapshot.scan_id += 1;
        this.snapshot.merge = merge_details;
        this.snapshot.statuses_by_path = statuses_by_path;
        this.snapshot.stash_entries = stash_entries;

        this.snapshot.clone()
    })
}
