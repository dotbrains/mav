use super::*;

impl BackgroundScanner {
    pub(super) async fn update_ignore_statuses_for_paths(
        &self,
        scan_job_tx: Sender<ScanJob>,
        prev_snapshot: LocalSnapshot,
        ignores_to_update: Vec<(Arc<Path>, IgnoreStack)>,
    ) {
        let (ignore_queue_tx, ignore_queue_rx) = async_channel::unbounded();
        {
            for (parent_abs_path, ignore_stack) in ignores_to_update {
                ignore_queue_tx
                    .send_blocking(UpdateIgnoreStatusJob {
                        abs_path: parent_abs_path,
                        ignore_stack,
                        ignore_queue: ignore_queue_tx.clone(),
                        scan_queue: scan_job_tx.clone(),
                    })
                    .unwrap();
            }
        }
        drop(ignore_queue_tx);

        self.executor
            .scoped(|scope| {
                for _ in 0..self.executor.num_cpus() {
                    scope.spawn(async {
                        loop {
                            select_biased! {
                                // Process any path refresh requests before moving on to process
                                // the queue of ignore statuses.
                                request = self.next_scan_request().fuse() => {
                                    let Ok(request) = request else { break };
                                    if !self.process_scan_request(request, true).await {
                                        return;
                                    }
                                }

                                // Recursively process directories whose ignores have changed.
                                job = ignore_queue_rx.recv().fuse() => {
                                    let Ok(job) = job else { break };
                                    self.update_ignore_status(job, &prev_snapshot).await;
                                }
                            }
                        }
                    });
                }
            })
            .await;
    }

    pub(super) async fn ignores_needing_update(&self) -> Vec<Arc<Path>> {
        let mut ignores_to_update = Vec::new();
        let mut excludes_to_load: Vec<(Arc<Path>, PathBuf)> = Vec::new();

        // First pass: collect updates and drop stale entries without awaiting.
        {
            let snapshot = &mut self.state.lock().await.snapshot;
            let abs_path = snapshot.abs_path.clone();
            let mut repo_exclude_keys_to_remove: Vec<Arc<Path>> = Vec::new();

            for (work_dir_abs_path, (_, needs_update)) in
                snapshot.repo_exclude_by_work_dir_abs_path.iter_mut()
            {
                let repository = snapshot
                    .git_repositories
                    .iter()
                    .find(|(_, repo)| &repo.work_directory_abs_path == work_dir_abs_path);

                if *needs_update {
                    *needs_update = false;
                    if work_dir_abs_path.starts_with(abs_path.as_path()) {
                        ignores_to_update.push(work_dir_abs_path.clone());
                    } else {
                        ignores_to_update.push(abs_path.as_path().into());
                    }

                    if let Some((_, repository)) = repository {
                        let exclude_abs_path = repository.common_dir_abs_path.join(REPO_EXCLUDE);
                        excludes_to_load.push((work_dir_abs_path.clone(), exclude_abs_path));
                    }
                }

                if repository.is_none() {
                    repo_exclude_keys_to_remove.push(work_dir_abs_path.clone());
                }
            }

            for key in repo_exclude_keys_to_remove {
                snapshot.repo_exclude_by_work_dir_abs_path.remove(&key);
            }

            snapshot
                .ignores_by_parent_abs_path
                .retain(|parent_abs_path, (_, needs_update)| {
                    if let Ok(parent_path) = parent_abs_path.strip_prefix(abs_path.as_path())
                        && let Some(parent_path) =
                            RelPath::new(&parent_path, PathStyle::local()).log_err()
                    {
                        if *needs_update {
                            *needs_update = false;
                            if snapshot.snapshot.entry_for_path(&parent_path).is_some() {
                                ignores_to_update.push(parent_abs_path.clone());
                            }
                        }

                        let ignore_path = parent_path.join(RelPath::unix(GITIGNORE).unwrap());
                        if snapshot.snapshot.entry_for_path(&ignore_path).is_none() {
                            return false;
                        }
                    }
                    true
                });
        }

        // Load gitignores asynchronously (outside the lock)
        let mut loaded_excludes: Vec<(Arc<Path>, Arc<Gitignore>)> = Vec::new();
        for (work_dir_abs_path, exclude_abs_path) in excludes_to_load {
            if let Ok(current_exclude) =
                build_gitignore_with_root(&exclude_abs_path, &work_dir_abs_path, self.fs.as_ref())
                    .await
            {
                loaded_excludes.push((work_dir_abs_path, Arc::new(current_exclude)));
            }
        }

        // Second pass: apply updates.
        if !loaded_excludes.is_empty() {
            let snapshot = &mut self.state.lock().await.snapshot;

            for (work_dir_abs_path, exclude) in loaded_excludes {
                if let Some((existing_exclude, _)) = snapshot
                    .repo_exclude_by_work_dir_abs_path
                    .get_mut(&work_dir_abs_path)
                {
                    *existing_exclude = exclude;
                }
            }
        }

        ignores_to_update
    }

    pub(super) async fn order_ignores(
        &self,
        mut ignores: Vec<Arc<Path>>,
    ) -> Vec<(Arc<Path>, IgnoreStack)> {
        let fs = self.fs.clone();
        let snapshot = self.state.lock().await.snapshot.clone();
        ignores.sort_unstable();
        let mut ignores_to_update = ignores.into_iter().peekable();

        let mut result = vec![];
        while let Some(parent_abs_path) = ignores_to_update.next() {
            while ignores_to_update
                .peek()
                .map_or(false, |p| p.starts_with(&parent_abs_path))
            {
                ignores_to_update.next().unwrap();
            }
            let ignore_stack = snapshot
                .ignore_stack_for_abs_path(&parent_abs_path, true, fs.as_ref())
                .await;
            result.push((parent_abs_path, ignore_stack));
        }

        result
    }

    pub(super) async fn update_ignore_status(
        &self,
        job: UpdateIgnoreStatusJob,
        snapshot: &LocalSnapshot,
    ) {
        log::trace!("update ignore status {:?}", job.abs_path);

        let mut ignore_stack = job.ignore_stack;
        if let Some((ignore, _)) = snapshot.ignores_by_parent_abs_path.get(&job.abs_path) {
            ignore_stack =
                ignore_stack.append(IgnoreKind::Gitignore(job.abs_path.clone()), ignore.clone());
        }

        let mut entries_by_id_edits = Vec::new();
        let mut entries_by_path_edits = Vec::new();
        let Some(path) = job
            .abs_path
            .strip_prefix(snapshot.abs_path.as_path())
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to strip prefix '{}' from path '{}'",
                    snapshot.abs_path.as_path().display(),
                    job.abs_path.display()
                )
            })
            .log_err()
        else {
            return;
        };

        let Some(path) = RelPath::new(&path, PathStyle::local()).log_err() else {
            return;
        };

        if let Ok(Some(metadata)) = self.fs.metadata(&job.abs_path.join(DOT_GIT)).await
            && metadata.is_dir
        {
            ignore_stack.repo_root = Some(job.abs_path.clone());
        }

        for mut entry in snapshot.child_entries(&path).cloned() {
            let was_ignored = entry.is_ignored;
            let abs_path: Arc<Path> = snapshot.absolutize(&entry.path).into();
            entry.is_ignored = ignore_stack.is_abs_path_ignored(&abs_path, entry.is_dir());

            if entry.is_dir() {
                let child_ignore_stack = if entry.is_ignored {
                    IgnoreStack::all()
                } else {
                    ignore_stack.clone()
                };

                // Scan any directories that were previously ignored and weren't previously scanned.
                if was_ignored && !entry.is_ignored && entry.kind.is_unloaded() {
                    let state = self.state.lock().await;
                    if self.should_scan_directory(&state, &entry) {
                        state
                            .enqueue_scan_dir(
                                abs_path.clone(),
                                &entry,
                                &job.scan_queue,
                                self.fs.as_ref(),
                            )
                            .await;
                    }
                }

                job.ignore_queue
                    .send(UpdateIgnoreStatusJob {
                        abs_path: abs_path.clone(),
                        ignore_stack: child_ignore_stack,
                        ignore_queue: job.ignore_queue.clone(),
                        scan_queue: job.scan_queue.clone(),
                    })
                    .await
                    .unwrap();
            }

            if entry.is_ignored != was_ignored {
                let mut path_entry = snapshot.entries_by_id.get(&entry.id, ()).unwrap().clone();
                path_entry.scan_id = snapshot.scan_id;
                path_entry.is_ignored = entry.is_ignored;
                entries_by_id_edits.push(Edit::Insert(path_entry));
                entries_by_path_edits.push(Edit::Insert(entry));
            }
        }

        let state = &mut self.state.lock().await;
        for edit in &entries_by_path_edits {
            if let Edit::Insert(entry) = edit
                && let Err(ix) = state.changed_paths.binary_search(&entry.path)
            {
                state.changed_paths.insert(ix, entry.path.clone());
            }
        }

        state
            .snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
        state.snapshot.entries_by_id.edit(entries_by_id_edits, ());
    }
}
