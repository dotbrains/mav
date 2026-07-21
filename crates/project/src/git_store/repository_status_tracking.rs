use super::*;

impl Repository {
    pub(super) fn paths_changed(
        &mut self,
        paths: Vec<RepoPath>,
        updates_tx: Option<mpsc::UnboundedSender<DownstreamUpdate>>,
        cx: &mut Context<Self>,
    ) {
        if !paths.is_empty() {
            self.paths_needing_status_update.push(paths);
        }

        let this = cx.weak_entity();
        let _ = self.send_keyed_job(
            "paths_changed",
            Some(GitJobKey::RefreshStatuses),
            None,
            |state, mut cx| async move {
                let (prev_snapshot, changed_paths) = this.update(&mut cx, |this, _| {
                    (
                        this.snapshot.clone(),
                        mem::take(&mut this.paths_needing_status_update),
                    )
                })?;
                let RepositoryState::Local(LocalRepositoryState { backend, .. }) = state else {
                    bail!("not a local repository")
                };

                if changed_paths.is_empty() {
                    return Ok(());
                }

                let has_head = prev_snapshot.head_commit.is_some();

                let changed_path_statuses = cx
                    .background_spawn(async move {
                        let changed_paths = GitStore::coalesce_repo_paths(
                            changed_paths
                                .into_iter()
                                .flatten()
                                .collect::<BTreeSet<_>>()
                                .into_iter()
                                .collect(),
                        );
                        let changed_paths_vec = changed_paths.iter().cloned().collect::<Vec<_>>();

                        let status_task = backend.status(&changed_paths_vec);
                        let diff_stat_future = if has_head {
                            backend.diff_stat(&changed_paths_vec)
                        } else {
                            future::ready(Ok(status::GitDiffStat {
                                entries: Arc::default(),
                            }))
                            .boxed()
                        };

                        let (statuses, diff_stats) =
                            futures::future::try_join(status_task, diff_stat_future).await?;

                        let diff_stats: HashMap<RepoPath, DiffStat> =
                            HashMap::from_iter(diff_stats.entries.into_iter().cloned());

                        let mut changed_path_statuses = Vec::new();
                        let prev_statuses = prev_snapshot.statuses_by_path.clone();
                        let current_status_paths = statuses
                            .entries
                            .iter()
                            .map(|(repo_path, _)| repo_path.clone())
                            .collect::<BTreeSet<_>>();

                        for path in &changed_paths {
                            let mut cursor = prev_statuses.cursor::<PathProgress>(());
                            cursor.seek_forward(&PathTarget::Path(path), Bias::Left);
                            while let Some(entry) = cursor.item() {
                                if !entry.repo_path.starts_with(path) {
                                    break;
                                }

                                if !current_status_paths.contains(&entry.repo_path) {
                                    changed_path_statuses.push(Edit::Remove(PathKey(
                                        entry.repo_path.as_ref().clone(),
                                    )));
                                }
                                cursor.next();
                            }
                        }

                        let mut cursor = prev_statuses.cursor::<PathProgress>(());

                        for (repo_path, status) in &*statuses.entries {
                            let current_diff_stat = diff_stats.get(repo_path).copied();

                            if cursor.seek_forward(&PathTarget::Path(repo_path), Bias::Left)
                                && cursor.item().is_some_and(|entry| {
                                    entry.status == *status && entry.diff_stat == current_diff_stat
                                })
                            {
                                continue;
                            }

                            changed_path_statuses.push(Edit::Insert(StatusEntry {
                                repo_path: repo_path.clone(),
                                status: *status,
                                diff_stat: current_diff_stat,
                            }));
                        }
                        anyhow::Ok(changed_path_statuses)
                    })
                    .await?;

                this.update(&mut cx, |this, cx| {
                    if !changed_path_statuses.is_empty() {
                        cx.emit(RepositoryEvent::StatusesChanged);
                        this.snapshot
                            .statuses_by_path
                            .edit(changed_path_statuses, ());
                        this.snapshot.scan_id += 1;
                    }

                    if let Some(updates_tx) = updates_tx {
                        updates_tx
                            .unbounded_send(DownstreamUpdate::UpdateRepository(
                                this.snapshot.clone(),
                            ))
                            .ok();
                    }
                })
            },
        );
    }

    /// currently running git command and when it started
    pub fn current_job(&self) -> Option<JobInfo> {
        self.active_jobs.values().next().cloned()
    }

    pub fn job_debug_queue(&self) -> &job_debug_queue::GitJobDebugQueue {
        &self.job_debug_queue
    }

    pub fn barrier(&mut self) -> oneshot::Receiver<()> {
        self.send_job("barrier", None, |_, _| async {})
    }

    pub(super) fn spawn_job_with_tracking<AsyncFn>(
        &mut self,
        paths: Vec<RepoPath>,
        git_status: pending_op::GitStatus,
        cx: &mut Context<Self>,
        f: AsyncFn,
    ) -> Task<Result<()>>
    where
        AsyncFn: AsyncFnOnce(WeakEntity<Repository>, &mut AsyncApp) -> Result<()> + 'static,
    {
        let ids = self.new_pending_ops_for_paths(paths, git_status);

        cx.spawn(async move |this, cx| {
            let (job_status, result) = match f(this.clone(), cx).await {
                Ok(()) => (pending_op::JobStatus::Finished, Ok(())),
                Err(err) if err.is::<Canceled>() => (pending_op::JobStatus::Skipped, Ok(())),
                Err(err) => (pending_op::JobStatus::Error, Err(err)),
            };

            this.update(cx, |this, _| {
                let mut edits = Vec::with_capacity(ids.len());
                for (id, entry) in ids {
                    if let Some(mut ops) = this
                        .pending_ops
                        .get(&PathKey(entry.as_ref().clone()), ())
                        .cloned()
                    {
                        if let Some(op) = ops.op_by_id_mut(id) {
                            op.job_status = job_status;
                        }
                        edits.push(sum_tree::Edit::Insert(ops));
                    }
                }
                this.pending_ops.edit(edits, ());
            })?;

            result
        })
    }

    fn new_pending_ops_for_paths(
        &mut self,
        paths: Vec<RepoPath>,
        git_status: pending_op::GitStatus,
    ) -> Vec<(PendingOpId, RepoPath)> {
        let mut edits = Vec::with_capacity(paths.len());
        let mut ids = Vec::with_capacity(paths.len());
        for path in paths {
            let mut ops = self
                .pending_ops
                .get(&PathKey(path.as_ref().clone()), ())
                .cloned()
                .unwrap_or_else(|| PendingOps::new(&path));
            let id = ops.max_id() + 1;
            ops.ops.push(PendingOp {
                id,
                git_status,
                job_status: pending_op::JobStatus::Running,
            });
            edits.push(sum_tree::Edit::Insert(ops));
            ids.push((id, path));
        }
        self.pending_ops.edit(edits, ());
        ids
    }
}
