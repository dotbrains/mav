use super::*;

impl BackgroundScanner {
    pub(crate) async fn run(
        &mut self,
        mut fs_events_rx: Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
    ) {
        let root_abs_path;
        let scanning_enabled;
        {
            let state = self.state.lock().await;
            root_abs_path = state.snapshot.abs_path.clone();
            scanning_enabled = state.scanning_enabled;
        }

        // If the worktree root does not contain a git repository, then find
        // the git repository in an ancestor directory. Find any gitignore files
        // in ancestor directories.
        let repo = if scanning_enabled && self.track_git_repositories {
            let (ignores, exclude, repo) =
                discover_ancestor_git_repo(self.fs.clone(), &root_abs_path).await;
            let mut state = self.state.lock().await;
            state.snapshot.ignores_by_parent_abs_path.extend(ignores);
            if let Some(exclude) = exclude {
                let work_directory_abs_path: Arc<Path> = repo
                    .as_ref()
                    .map(|(_, work_directory)| {
                        state
                            .snapshot
                            .work_directory_abs_path(work_directory)
                            .into()
                    })
                    .unwrap_or_else(|| root_abs_path.as_path().into());
                state
                    .snapshot
                    .repo_exclude_by_work_dir_abs_path
                    .insert(work_directory_abs_path, (exclude, false));
            }

            repo
        } else {
            None
        };

        let containing_git_repository = if let Some((ancestor_dot_git, work_directory)) = repo
            && scanning_enabled
            && self.track_git_repositories
        {
            maybe!(async {
                self.state
                    .lock()
                    .await
                    .insert_git_repository_for_path(
                        work_directory,
                        ancestor_dot_git.clone().into(),
                        self.fs.as_ref(),
                        self.watcher.as_ref(),
                    )
                    .await
                    .log_err()?;
                Some(ancestor_dot_git)
            })
            .await
        } else {
            None
        };

        log::trace!("containing git repository: {containing_git_repository:?}");

        let global_gitignore_file = paths::global_gitignore_path();
        let mut global_gitignore_events = if let Some(global_gitignore_path) =
            &global_gitignore_file
            && scanning_enabled
            && self.track_git_repositories
        {
            let is_file = self.fs.is_file(&global_gitignore_path).await;
            self.state.lock().await.snapshot.global_gitignore = if is_file {
                build_gitignore(global_gitignore_path, self.fs.as_ref())
                    .await
                    .ok()
                    .map(Arc::new)
            } else {
                None
            };
            if is_file {
                self.fs
                    .watch(global_gitignore_path, FS_WATCH_LATENCY)
                    .await
                    .0
            } else {
                Box::pin(futures::stream::pending())
            }
        } else {
            self.state.lock().await.snapshot.global_gitignore = None;
            Box::pin(futures::stream::pending())
        };

        let (scan_job_tx, scan_job_rx) = async_channel::unbounded();
        {
            let mut state = self.state.lock().await;
            state.snapshot.scan_id += 1;
            if let Some(mut root_entry) = state.snapshot.root_entry().cloned() {
                let ignore_stack = state
                    .snapshot
                    .ignore_stack_for_abs_path(root_abs_path.as_path(), true, self.fs.as_ref())
                    .await;
                if ignore_stack.is_abs_path_ignored(root_abs_path.as_path(), true) {
                    root_entry.is_ignored = true;
                    let mut root_entry = root_entry.clone();
                    state.reuse_entry_id(&mut root_entry);
                    state
                        .insert_entry(root_entry, self.fs.as_ref(), self.watcher.as_ref())
                        .await;
                }
                if root_entry.is_dir() && state.scanning_enabled {
                    state
                        .enqueue_scan_dir(
                            root_abs_path.as_path().into(),
                            &root_entry,
                            &scan_job_tx,
                            self.fs.as_ref(),
                        )
                        .await;
                }
            }
        };

        // Perform an initial scan of the directory.
        drop(scan_job_tx);
        self.scan_dirs(true, scan_job_rx).await;
        {
            let mut state = self.state.lock().await;
            state.snapshot.completed_scan_id = state.snapshot.scan_id;
        }

        self.send_status_update(false, SmallVec::new(), &[]).await;

        if self.defer_watch {
            let (events, watcher) = self
                .fs
                .watch(root_abs_path.as_path(), FS_WATCH_LATENCY)
                .await;
            self.watcher = watcher;
            fs_events_rx = Box::pin(events.map(|events| events.into_iter().collect()));

            let state = self.state.lock().await;
            for target in state.symlink_paths_by_target.keys() {
                if !target.starts_with(root_abs_path.as_path()) {
                    self.watcher.add(target).log_err();
                }
            }
            for repo in state.snapshot.git_repositories.values() {
                if !repo
                    .common_dir_abs_path
                    .starts_with(root_abs_path.as_path())
                {
                    self.watcher.add(&repo.common_dir_abs_path).log_err();
                }
                if !repo
                    .repository_dir_abs_path
                    .starts_with(root_abs_path.as_path())
                {
                    self.watcher.add(&repo.repository_dir_abs_path).log_err();
                }
            }
            drop(state);
        }

        // Process any any FS events that occurred while performing the initial scan.
        // For these events, update events cannot be as precise, because we didn't
        // have the previous state loaded yet.
        self.phase = BackgroundScannerPhase::EventsReceivedDuringInitialScan;
        if let Poll::Ready(Some(mut paths)) = futures::poll!(fs_events_rx.next()) {
            while let Poll::Ready(Some(more_paths)) = futures::poll!(fs_events_rx.next()) {
                paths.extend(more_paths);
            }
            self.process_events(
                paths
                    .into_iter()
                    .filter(|event| event.kind.is_some())
                    .collect(),
            )
            .await;
        }
        if let Some(abs_path) = containing_git_repository {
            self.process_events(vec![PathEvent {
                path: abs_path,
                kind: Some(fs::PathEventKind::Changed),
            }])
            .await;
        }

        // Continue processing events until the worktree is dropped.
        self.phase = BackgroundScannerPhase::Events;

        loop {
            select_biased! {
                // Process any path refresh requests from the worktree. Prioritize
                // these before handling changes reported by the filesystem.
                request = self.next_scan_request().fuse() => {
                    let Ok(request) = request else { break };
                    if !self.process_scan_request(request, false).await {
                        return;
                    }
                }

                path_prefix_request = self.path_prefixes_to_scan_rx.recv().fuse() => {
                    let Ok(request) = path_prefix_request else { break };

                    if self.state.lock().await.path_prefixes_to_scan.contains(&request.path) {
                        self.send_status_update(false, request.done, &[]).await;
                        continue;
                    }

                    log::trace!("adding path prefix {:?}", request.path);

                    let did_scan = self.forcibly_load_paths(std::slice::from_ref(&request.path)).await;
                    if did_scan {
                        let abs_path =
                        {
                            let mut state = self.state.lock().await;
                            state.path_prefixes_to_scan.insert(request.path.clone());
                            state.snapshot.absolutize(&request.path)
                        };

                        if let Some(abs_path) = self.fs.canonicalize(&abs_path).await.log_err() {
                            self.process_events(vec![PathEvent {
                                path: abs_path,
                                kind: Some(fs::PathEventKind::Changed),
                            }])
                            .await;
                        }
                    }
                    self.send_status_update(false, request.done, &[]).await;
                }

                paths = fs_events_rx.next().fuse() => {
                    let Some(mut paths) = paths else { break };
                    while let Poll::Ready(Some(more_paths)) = futures::poll!(fs_events_rx.next()) {
                        paths.extend(more_paths);
                    }
                    self.process_events(paths.into_iter().filter(|event| event.kind.is_some()).collect()).await;
                }

                _ = global_gitignore_events.next().fuse() => {
                    if let Some(path) = &global_gitignore_file {
                        self.update_global_gitignore(&path).await;
                    }
                }
            }
        }
    }
}
