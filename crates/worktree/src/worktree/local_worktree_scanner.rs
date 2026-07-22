use super::*;

impl LocalWorktree {
    pub(super) fn restart_background_scanners(&mut self, cx: &Context<Worktree>) {
        let (scan_requests_tx, scan_requests_rx) = async_channel::unbounded();
        let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = async_channel::unbounded();
        self.scan_requests_tx = scan_requests_tx;
        self.path_prefixes_to_scan_tx = path_prefixes_to_scan_tx;

        self.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
        let always_included_entries = mem::take(&mut self.snapshot.always_included_entries);
        log::debug!(
            "refreshing entries for the following always included paths: {:?}",
            always_included_entries
        );

        // Cleans up old always included entries to ensure they get updated properly. Otherwise,
        // nested always included entries may not get updated and will result in out-of-date info.
        self.refresh_entries_for_paths(always_included_entries);
    }

    pub(super) fn start_background_scanner(
        &mut self,
        scan_requests_rx: async_channel::Receiver<ScanRequest>,
        path_prefixes_to_scan_rx: async_channel::Receiver<PathPrefixScanRequest>,
        cx: &Context<Worktree>,
    ) {
        let snapshot = self.snapshot();
        let share_private_files = self.share_private_files;
        let next_entry_id = self.next_entry_id.clone();
        let fs = self.fs.clone();
        let scanning_enabled = self.scanning_enabled;
        let force_defer_watch = self.force_defer_watch;
        let track_git_repositories = self.visible;
        let settings = self.settings.clone();
        let (scan_states_tx, mut scan_states_rx) = mpsc::unbounded();
        let background_scanner = cx.background_spawn({
            let abs_path = snapshot.abs_path.as_path().to_path_buf();
            let background = cx.background_executor().clone();
            async move {
                let defer_watch =
                    force_defer_watch || (scanning_enabled && fs::requires_poll_watcher(&abs_path));

                let (events, watcher) = if scanning_enabled && !defer_watch {
                    fs.watch(&abs_path, FS_WATCH_LATENCY).await
                } else {
                    (Box::pin(stream::pending()) as _, Arc::new(NullWatcher) as _)
                };
                let fs_case_sensitive = fs.is_case_sensitive().await;

                let is_single_file = snapshot.snapshot.root_dir().is_none();
                let mut scanner = BackgroundScanner {
                    fs,
                    fs_case_sensitive,
                    status_updates_tx: scan_states_tx,
                    executor: background,
                    scan_requests_rx,
                    path_prefixes_to_scan_rx,
                    next_entry_id,
                    state: async_lock::Mutex::new(BackgroundScannerState {
                        prev_snapshot: snapshot.snapshot.clone(),
                        snapshot,
                        symlink_paths_by_target: Default::default(),
                        scanned_dirs: Default::default(),
                        watched_dir_abs_paths_by_entry_id: Default::default(),
                        scanning_enabled,
                        path_prefixes_to_scan: Default::default(),
                        paths_to_scan: Default::default(),
                        removed_entries: Default::default(),
                        changed_paths: Default::default(),
                    }),
                    phase: BackgroundScannerPhase::InitialScan,
                    share_private_files,
                    settings,
                    watcher,
                    track_git_repositories,
                    is_single_file,
                    defer_watch,
                };

                scanner.run(events).await;
            }
        });
        let scan_state_updater = cx.spawn(async move |this, cx| {
            while let Some((state, this)) = scan_states_rx.next().await.zip(this.upgrade()) {
                this.update(cx, |this, cx| {
                    let this = this.as_local_mut().unwrap();
                    match state {
                        ScanState::Started => {
                            *this.is_scanning.0.borrow_mut() = true;
                        }
                        ScanState::Updated {
                            snapshot,
                            changes,
                            barrier,
                            scanning,
                        } => {
                            *this.is_scanning.0.borrow_mut() = scanning;
                            this.set_snapshot(snapshot, changes, cx);
                            drop(barrier);
                        }
                        ScanState::RootUpdated { new_path } => {
                            this.update_abs_path_and_refresh(new_path, cx);
                        }
                        ScanState::RootDeleted => {
                            log::info!(
                                "worktree root {} no longer exists, closing worktree",
                                this.abs_path().display()
                            );
                            cx.emit(Event::Deleted);
                        }
                    }
                });
            }
        });
        self._background_scanner_tasks = vec![background_scanner, scan_state_updater];
        *self.is_scanning.0.borrow_mut() = true;
    }

    pub(super) fn set_snapshot(
        &mut self,
        mut new_snapshot: LocalSnapshot,
        entry_changes: UpdatedEntriesSet,
        cx: &mut Context<Worktree>,
    ) {
        let repo_changes = self.changed_repos(&self.snapshot, &mut new_snapshot);

        new_snapshot.root_repo_common_dir = new_snapshot
            .local_repo_for_work_directory_path(RelPath::empty())
            .map(|repo| SanitizedPath::from_arc(repo.common_dir_abs_path.clone()));

        let old_root_repo_common_dir = (self.snapshot.root_repo_common_dir
            != new_snapshot.root_repo_common_dir)
            .then(|| self.snapshot.root_repo_common_dir.clone());
        self.snapshot = new_snapshot;

        if let Some(share) = self.update_observer.as_mut() {
            share
                .snapshots_tx
                .unbounded_send((self.snapshot.clone(), entry_changes.clone()))
                .ok();
        }

        if !entry_changes.is_empty() {
            cx.emit(Event::UpdatedEntries(entry_changes));
        }
        if !repo_changes.is_empty() {
            cx.emit(Event::UpdatedGitRepositories(repo_changes));
        }
        if let Some(old) = old_root_repo_common_dir {
            cx.emit(Event::UpdatedRootRepoCommonDir { old });
        }

        while let Some((scan_id, _)) = self.snapshot_subscriptions.front() {
            if self.snapshot.completed_scan_id >= *scan_id {
                let (_, tx) = self.snapshot_subscriptions.pop_front().unwrap();
                tx.send(()).ok();
            } else {
                break;
            }
        }
    }

    pub(super) fn changed_repos(
        &self,
        old_snapshot: &LocalSnapshot,
        new_snapshot: &mut LocalSnapshot,
    ) -> UpdatedGitRepositoriesSet {
        let mut changes = Vec::new();
        let mut old_repos = old_snapshot.git_repositories.iter().peekable();
        let new_repos = new_snapshot.git_repositories.clone();
        let mut new_repos = new_repos.iter().peekable();

        loop {
            match (new_repos.peek().map(clone), old_repos.peek().map(clone)) {
                (Some((new_entry_id, new_repo)), Some((old_entry_id, old_repo))) => {
                    match Ord::cmp(&new_entry_id, &old_entry_id) {
                        Ordering::Less => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: new_entry_id,
                                old_work_directory_abs_path: None,
                                new_work_directory_abs_path: Some(
                                    new_repo.work_directory_abs_path.clone(),
                                ),
                                dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                repository_dir_abs_path: Some(
                                    new_repo.repository_dir_abs_path.clone(),
                                ),
                                common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                            });
                            new_repos.next();
                        }
                        Ordering::Equal => {
                            if new_repo.git_dir_scan_id != old_repo.git_dir_scan_id
                                || new_repo.work_directory_abs_path
                                    != old_repo.work_directory_abs_path
                            {
                                changes.push(UpdatedGitRepository {
                                    work_directory_id: new_entry_id,
                                    old_work_directory_abs_path: Some(
                                        old_repo.work_directory_abs_path.clone(),
                                    ),
                                    new_work_directory_abs_path: Some(
                                        new_repo.work_directory_abs_path.clone(),
                                    ),
                                    dot_git_abs_path: Some(new_repo.dot_git_abs_path.clone()),
                                    repository_dir_abs_path: Some(
                                        new_repo.repository_dir_abs_path.clone(),
                                    ),
                                    common_dir_abs_path: Some(new_repo.common_dir_abs_path.clone()),
                                });
                            }
                            new_repos.next();
                            old_repos.next();
                        }
                        Ordering::Greater => {
                            changes.push(UpdatedGitRepository {
                                work_directory_id: old_entry_id,
                                old_work_directory_abs_path: Some(
                                    old_repo.work_directory_abs_path.clone(),
                                ),
                                new_work_directory_abs_path: None,
                                dot_git_abs_path: None,
                                repository_dir_abs_path: None,
                                common_dir_abs_path: None,
                            });
                            old_repos.next();
                        }
                    }
                }
                (Some((entry_id, repo)), None) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: None,
                        new_work_directory_abs_path: Some(repo.work_directory_abs_path.clone()),
                        dot_git_abs_path: Some(repo.dot_git_abs_path.clone()),
                        repository_dir_abs_path: Some(repo.repository_dir_abs_path.clone()),
                        common_dir_abs_path: Some(repo.common_dir_abs_path.clone()),
                    });
                    new_repos.next();
                }
                (None, Some((entry_id, repo))) => {
                    changes.push(UpdatedGitRepository {
                        work_directory_id: entry_id,
                        old_work_directory_abs_path: Some(repo.work_directory_abs_path.clone()),
                        new_work_directory_abs_path: None,
                        dot_git_abs_path: Some(repo.dot_git_abs_path.clone()),
                        repository_dir_abs_path: Some(repo.repository_dir_abs_path.clone()),
                        common_dir_abs_path: Some(repo.common_dir_abs_path.clone()),
                    });
                    old_repos.next();
                }
                (None, None) => break,
            }
        }

        pub(super) fn clone<T: Clone, U: Clone>(value: &(&T, &U)) -> (T, U) {
            (value.0.clone(), value.1.clone())
        }

        changes.into()
    }
}
