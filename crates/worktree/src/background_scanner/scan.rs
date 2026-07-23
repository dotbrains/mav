use super::*;

impl BackgroundScanner {
    pub(super) async fn forcibly_load_paths(&self, paths: &[Arc<RelPath>]) -> bool {
        let (scan_job_tx, scan_job_rx) = async_channel::unbounded();
        {
            let mut state = self.state.lock().await;
            let root_path = state.snapshot.abs_path.clone();
            for path in paths {
                for ancestor in path.ancestors() {
                    if let Some(entry) = state.snapshot.entry_for_path(ancestor)
                        && entry.kind == EntryKind::UnloadedDir
                    {
                        let abs_path = if entry.is_external {
                            entry
                                .canonical_path
                                .as_ref()
                                .map(|path| path.as_ref().to_path_buf())
                                .unwrap_or_else(|| root_path.join(ancestor.as_std_path()))
                        } else {
                            root_path.join(ancestor.as_std_path())
                        };
                        state
                            .enqueue_scan_dir(
                                abs_path.into(),
                                entry,
                                &scan_job_tx,
                                self.fs.as_ref(),
                            )
                            .await;
                        state.paths_to_scan.insert(path.clone());
                        break;
                    }
                }
            }
            drop(scan_job_tx);
        }
        while let Ok(job) = scan_job_rx.recv().await {
            self.scan_dir(&job).await.log_err();
        }

        !mem::take(&mut self.state.lock().await.paths_to_scan).is_empty()
    }

    pub(super) async fn scan_dirs(
        &self,
        enable_progress_updates: bool,
        scan_jobs_rx: async_channel::Receiver<ScanJob>,
    ) {
        if self
            .status_updates_tx
            .unbounded_send(ScanState::Started)
            .is_err()
        {
            return;
        }

        let progress_update_count = AtomicUsize::new(0);
        self.executor
            .scoped_priority(Priority::Low, |scope| {
                for _ in 0..self.executor.num_cpus() {
                    scope.spawn(async {
                        let mut last_progress_update_count = 0;
                        let progress_update_timer = self.progress_timer(enable_progress_updates).fuse();
                        futures::pin_mut!(progress_update_timer);

                        loop {
                            select_biased! {
                                // Process any path refresh requests before moving on to process
                                // the scan queue, so that user operations are prioritized.
                                request = self.next_scan_request().fuse() => {
                                    let Ok(request) = request else { break };
                                    if !self.process_scan_request(request, true).await {
                                        return;
                                    }
                                }

                                // Send periodic progress updates to the worktree. Use an atomic counter
                                // to ensure that only one of the workers sends a progress update after
                                // the update interval elapses.
                                _ = progress_update_timer => {
                                    match progress_update_count.compare_exchange(
                                        last_progress_update_count,
                                        last_progress_update_count + 1,
                                        SeqCst,
                                        SeqCst
                                    ) {
                                        Ok(_) => {
                                            last_progress_update_count += 1;
                                            self.send_status_update(true, SmallVec::new(), &[])
                                                .await;
                                        }
                                        Err(count) => {
                                            last_progress_update_count = count;
                                        }
                                    }
                                    progress_update_timer.set(self.progress_timer(enable_progress_updates).fuse());
                                }

                                // Recursively load directories from the file system.
                                job = scan_jobs_rx.recv().fuse() => {
                                    let Ok(job) = job else { break };
                                    if let Err(err) = self.scan_dir(&job).await
                                        && job.path.is_empty() {
                                            log::error!("error scanning directory {:?}: {}", job.abs_path, err);
                                        }
                                }
                            }
                        }
                    });
                }
            })
            .await;
    }

    pub(super) async fn send_status_update(
        &self,
        scanning: bool,
        barrier: SmallVec<[barrier::Sender; 1]>,
        event_roots: &[EventRoot],
    ) -> bool {
        let mut state = self.state.lock().await;
        if state.changed_paths.is_empty() && event_roots.is_empty() && scanning {
            return true;
        }

        let merged_event_roots = merge_event_roots(&state.changed_paths, event_roots);

        let new_snapshot = state.snapshot.clone();
        let old_snapshot = mem::replace(&mut state.prev_snapshot, new_snapshot.snapshot.clone());
        let changes = build_diff(
            self.phase,
            &old_snapshot,
            &new_snapshot,
            &merged_event_roots,
        );
        state.changed_paths.clear();

        self.status_updates_tx
            .unbounded_send(ScanState::Updated {
                snapshot: new_snapshot,
                changes,
                scanning,
                barrier,
            })
            .is_ok()
    }

    pub(super) async fn scan_dir(&self, job: &ScanJob) -> Result<()> {
        let root_abs_path;
        let root_char_bag;
        {
            let snapshot = &self.state.lock().await.snapshot;
            if self.settings.is_path_excluded(&job.path) {
                log::error!("skipping excluded directory {:?}", job.path);
                return Ok(());
            }
            log::trace!("scanning directory {:?}", job.path);
            root_abs_path = snapshot.abs_path().clone();
            root_char_bag = snapshot.root_char_bag;
        }

        let next_entry_id = self.next_entry_id.clone();
        let mut ignore_stack = job.ignore_stack.clone();
        let mut new_ignore = None;
        let mut root_canonical_path = None;
        let mut new_entries: Vec<Entry> = Vec::new();
        let mut new_jobs: Vec<Option<ScanJob>> = Vec::new();
        let mut child_paths = self
            .fs
            .read_dir(&job.abs_path)
            .await?
            .filter_map(|entry| async {
                match entry {
                    Ok(entry) => Some(entry),
                    Err(error) => {
                        log::error!("error processing entry {:?}", error);
                        None
                    }
                }
            })
            .collect::<Vec<_>>()
            .await;

        // Ensure that .git and .gitignore are processed first.
        swap_to_front(&mut child_paths, GITIGNORE);
        swap_to_front(&mut child_paths, DOT_GIT);

        if let Some(path) = child_paths.first()
            && path.ends_with(DOT_GIT)
        {
            ignore_stack.repo_root = Some(job.abs_path.clone());
        }

        for child_abs_path in child_paths {
            let child_abs_path: Arc<Path> = child_abs_path.into();
            let child_name = child_abs_path.file_name().unwrap();
            let Some(child_path) = child_name
                .to_str()
                .and_then(|name| Some(job.path.join(RelPath::unix(name).ok()?)))
            else {
                continue;
            };

            if self.track_git_repositories {
                if child_name == DOT_GIT {
                    let mut state = self.state.lock().await;
                    state
                        .insert_git_repository(
                            child_path.clone(),
                            self.fs.as_ref(),
                            self.watcher.as_ref(),
                        )
                        .await;
                } else if child_name == GITIGNORE {
                    match build_gitignore(&child_abs_path, self.fs.as_ref()).await {
                        Ok(ignore) => {
                            let ignore = Arc::new(ignore);
                            ignore_stack = ignore_stack.append(
                                IgnoreKind::Gitignore(job.abs_path.clone()),
                                ignore.clone(),
                            );
                            new_ignore = Some(ignore);
                        }
                        Err(error) => {
                            log::error!(
                                "error loading .gitignore file {:?} - {:?}",
                                child_name,
                                error
                            );
                        }
                    }
                }
            }

            if self.settings.is_path_excluded(&child_path) {
                log::debug!("skipping excluded child entry {child_path:?}");

                self.state
                    .lock()
                    .await
                    .remove_path_from_snapshot_and_unwatch(
                        &child_path,
                        self.watcher.as_ref(),
                        true,
                    );
                continue;
            }

            let child_metadata = match self.fs.metadata(&child_abs_path).await {
                Ok(Some(metadata)) => metadata,
                Ok(None) => continue,
                Err(err) => {
                    log::error!("error processing {:?}: {err:#}", child_abs_path.display());
                    continue;
                }
            };

            let mut child_entry = Entry::new(
                child_path.clone(),
                &child_metadata,
                ProjectEntryId::new(&next_entry_id),
                root_char_bag,
                None,
            );

            if job.is_external {
                child_entry.is_external = true;
            } else if child_metadata.is_symlink {
                let canonical_path = match self.fs.canonicalize(&child_abs_path).await {
                    Ok(path) => path,
                    Err(err) => {
                        log::error!("error reading target of symlink {child_abs_path:?}: {err:#}",);
                        continue;
                    }
                };

                // lazily canonicalize the root path in order to determine if
                // symlinks point outside of the worktree.
                let root_canonical_path = match &root_canonical_path {
                    Some(path) => path,
                    None => match self.fs.canonicalize(&root_abs_path).await {
                        Ok(path) => root_canonical_path.insert(path),
                        Err(err) => {
                            log::error!("error canonicalizing root {:?}: {:?}", root_abs_path, err);
                            continue;
                        }
                    },
                };

                if !canonical_path.starts_with(root_canonical_path) {
                    child_entry.is_external = true;
                }

                if child_metadata.is_dir {
                    let mut state = self.state.lock().await;
                    let paths = state
                        .symlink_paths_by_target
                        .entry(Arc::from(canonical_path.clone()))
                        .or_default();
                    if !paths.iter().any(|path| path == &child_path) {
                        paths.push(child_path.clone());
                    }
                }

                child_entry.canonical_path = Some(canonical_path.into());
            }

            if child_entry.is_dir() {
                child_entry.is_ignored = ignore_stack.is_abs_path_ignored(&child_abs_path, true);
                child_entry.is_always_included =
                    self.settings.is_path_always_included(&child_path, true);

                // Avoid recursing until crash in the case of a recursive symlink
                if job.ancestor_inodes.contains(&child_entry.inode) {
                    new_jobs.push(None);
                } else {
                    let mut ancestor_inodes = job.ancestor_inodes.clone();
                    ancestor_inodes.insert(child_entry.inode);

                    new_jobs.push(Some(ScanJob {
                        abs_path: child_abs_path.clone(),
                        path: child_path,
                        is_external: child_entry.is_external,
                        ignore_stack: if child_entry.is_ignored {
                            IgnoreStack::all()
                        } else {
                            ignore_stack.clone()
                        },
                        ancestor_inodes,
                        scan_queue: job.scan_queue.clone(),
                    }));
                }
            } else {
                child_entry.is_ignored = ignore_stack.is_abs_path_ignored(&child_abs_path, false);
                child_entry.is_always_included =
                    self.settings.is_path_always_included(&child_path, false);
            }

            {
                let relative_path = job
                    .path
                    .join(RelPath::unix(child_name.to_str().unwrap()).unwrap());
                if self.is_path_private(&relative_path) {
                    log::debug!("detected private file: {relative_path:?}");
                    child_entry.is_private = true;
                }
                if self.settings.is_path_hidden(&relative_path) {
                    log::debug!("detected hidden file: {relative_path:?}");
                    child_entry.is_hidden = true;
                }
            }

            new_entries.push(child_entry);
        }

        let mut state = self.state.lock().await;
        // Identify any subdirectories that should not be scanned.
        let mut job_ix = 0;
        for entry in &mut new_entries {
            state.reuse_entry_id(entry);
            if entry.is_dir() {
                if self.should_scan_directory(&state, entry) {
                    job_ix += 1;
                } else {
                    log::debug!("defer scanning directory {:?}", entry.path);
                    entry.kind = EntryKind::UnloadedDir;
                    new_jobs.remove(job_ix);
                }
            }
            if entry.is_always_included {
                state
                    .snapshot
                    .always_included_entries
                    .push(entry.path.clone());
            }
        }

        state.populate_dir(job.path.clone(), new_entries, new_ignore);
        // For external entries, watch the canonical (resolved) path so OS-level
        // FS events on the real filesystem location are observed. The same
        // canonical path is stored in both `external_canonical_to_relative`
        // (for translating canonical-path FS events back to worktree-relative
        // paths) and `watched_dir_abs_paths_by_entry_id` (used by `remove_path`
        // to know which abs path to unwatch), so both cleanup paths agree on
        // the path the watcher was actually registered on.
        //
        // `canonicalize` is an async filesystem operation that may suspend, so
        // the lock must not be held across the await point below.
        drop(state);
        let watched_abs_path: Option<Arc<Path>> = if job.is_external {
            self.fs
                .canonicalize(job.abs_path.as_ref())
                .await
                .ok()
                .map(|canonical| {
                    let canonical: Arc<Path> = canonical.into();
                    self.watcher.add(&canonical).log_err();
                    canonical
                })
        } else {
            self.watcher.add(job.abs_path.as_ref()).log_err();
            Some(job.abs_path.clone())
        };

        let mut state = self.state.lock().await;
        if let Some(watched_abs_path) = &watched_abs_path {
            if job.is_external {
                state
                    .snapshot
                    .external_canonical_to_relative
                    .insert(watched_abs_path.clone(), job.path.clone());
            }
            if let Some(entry_id) = state
                .snapshot
                .entry_for_path(&job.path)
                .map(|entry| entry.id)
            {
                state
                    .watched_dir_abs_paths_by_entry_id
                    .insert(entry_id, watched_abs_path.clone());
            }
        }

        for new_job in new_jobs.into_iter().flatten() {
            job.scan_queue
                .try_send(new_job)
                .expect("channel is unbounded");
        }

        Ok(())
    }
}
