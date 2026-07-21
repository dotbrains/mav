use super::*;

pub(super) struct BackgroundScanner {
    pub(super) state: async_lock::Mutex<BackgroundScannerState>,
    pub(super) fs: Arc<dyn Fs>,
    pub(super) fs_case_sensitive: bool,
    pub(super) status_updates_tx: UnboundedSender<ScanState>,
    pub(super) executor: BackgroundExecutor,
    pub(super) scan_requests_rx: async_channel::Receiver<ScanRequest>,
    pub(super) path_prefixes_to_scan_rx: async_channel::Receiver<PathPrefixScanRequest>,
    pub(super) next_entry_id: Arc<AtomicUsize>,
    pub(super) phase: BackgroundScannerPhase,
    pub(super) watcher: Arc<dyn Watcher>,
    pub(super) settings: WorktreeSettings,
    pub(super) share_private_files: bool,
    pub(super) track_git_repositories: bool,
    /// Whether this is a single-file worktree (root is a file, not a directory).
    /// Used to determine if we should give up after repeated canonicalization failures.
    pub(super) is_single_file: bool,
    pub(super) defer_watch: bool,
}

#[derive(Copy, Clone, PartialEq)]
pub(super) enum BackgroundScannerPhase {
    InitialScan,
    EventsReceivedDuringInitialScan,
    Events,
}

impl BackgroundScanner {
    pub(super) async fn run(
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

    async fn process_scan_request(&self, mut request: ScanRequest, scanning: bool) -> bool {
        log::debug!("rescanning paths {:?}", request.relative_paths);

        request.relative_paths.sort_unstable();
        self.forcibly_load_paths(&request.relative_paths).await;

        let root_path = self.state.lock().await.snapshot.abs_path.clone();
        let root_canonical_path = self.fs.canonicalize(root_path.as_path()).await;
        let root_canonical_path = match &root_canonical_path {
            Ok(path) => SanitizedPath::new(path),
            Err(err) => {
                log::error!("failed to canonicalize root path {root_path:?}: {err:#}");
                return true;
            }
        };
        let abs_paths = request
            .relative_paths
            .iter()
            .map(|path| {
                if path.file_name().is_some() {
                    root_canonical_path.as_path().join(path.as_std_path())
                } else {
                    root_canonical_path.as_path().to_path_buf()
                }
            })
            .collect::<Vec<_>>();

        {
            let mut state = self.state.lock().await;
            let is_idle = state.snapshot.completed_scan_id == state.snapshot.scan_id;
            state.snapshot.scan_id += 1;
            if is_idle {
                state.snapshot.completed_scan_id = state.snapshot.scan_id;
            }
        }

        self.reload_entries_for_paths(
            &root_path,
            &root_canonical_path,
            &request.relative_paths,
            abs_paths,
            None,
        )
        .await;

        self.send_status_update(scanning, request.done, &[]).await
    }

    fn normalized_events_for_worktree(
        state: &BackgroundScannerState,
        root_canonical_path: &SanitizedPath,
        mut events: Vec<PathEvent>,
    ) -> Vec<PathEvent> {
        if state.symlink_paths_by_target.is_empty() {
            return events;
        }
        let mut mapped_events = Vec::new();

        events.retain(|event| {
            let abs_path = SanitizedPath::new(&event.path);

            let mut best_match: Option<(&Arc<Path>, &SmallVec<[Arc<RelPath>; 1]>)> = None;
            let mut best_depth = 0;
            for (target_root, symlink_paths) in &state.symlink_paths_by_target {
                if abs_path.as_path().starts_with(target_root.as_ref()) {
                    let depth = target_root.as_ref().components().count();
                    if depth > best_depth {
                        best_depth = depth;
                        best_match = Some((target_root, symlink_paths));
                    }
                }
            }

            let Some((target_root, symlink_paths)) = best_match else {
                return true;
            };

            let Ok(suffix) = abs_path.as_path().strip_prefix(target_root.as_ref()) else {
                return true;
            };

            // If the symlink's real target is outside this worktree, the original path
            // isn't visible to the worktree. Keep only the remapped symlink events.
            let keep_original = target_root.starts_with(root_canonical_path.as_path());

            for symlink_path in symlink_paths {
                let mapped_path = if suffix.as_os_str().is_empty() {
                    root_canonical_path
                        .as_path()
                        .join(symlink_path.as_std_path())
                } else {
                    root_canonical_path
                        .as_path()
                        .join(symlink_path.as_std_path())
                        .join(suffix)
                };
                if mapped_path != event.path {
                    mapped_events.push(PathEvent {
                        path: mapped_path,
                        kind: event.kind,
                    });
                }
            }
            keep_original
        });
        events.extend(mapped_events);
        events
    }

    async fn process_events(&self, mut events: Vec<PathEvent>) {
        let root_path = self.state.lock().await.snapshot.abs_path.clone();
        let root_canonical_path = self.fs.canonicalize(root_path.as_path()).await;
        let root_canonical_path = match &root_canonical_path {
            Ok(path) => SanitizedPath::new(path),
            Err(err) => {
                let new_path = self
                    .state
                    .lock()
                    .await
                    .snapshot
                    .root_file_handle
                    .clone()
                    .and_then(|handle| match handle.current_path(&self.fs) {
                        Ok(new_path) => Some(new_path),
                        Err(e) => {
                            log::error!("Failed to refresh worktree root path: {e:#}");
                            None
                        }
                    })
                    .map(|path| SanitizedPath::new_arc(&path))
                    .filter(|new_path| *new_path != root_path);

                if let Some(new_path) = new_path {
                    log::info!(
                        "root renamed from {:?} to {:?}",
                        root_path.as_path(),
                        new_path.as_path(),
                    );
                    self.status_updates_tx
                        .unbounded_send(ScanState::RootUpdated { new_path })
                        .ok();
                } else {
                    log::error!("root path could not be canonicalized: {err:#}");

                    // For single-file worktrees, if we can't canonicalize and the file handle
                    // fallback also failed, the file is gone - close the worktree
                    if self.is_single_file {
                        log::info!(
                            "single-file worktree root {:?} no longer exists, marking as deleted",
                            root_path.as_path()
                        );
                        self.status_updates_tx
                            .unbounded_send(ScanState::RootDeleted)
                            .ok();
                    }
                }
                return;
            }
        };

        {
            let state = self.state.lock().await;
            events = Self::normalized_events_for_worktree(&state, &root_canonical_path, events);
        }

        log::debug!("raw events for process_events: {events:?}");

        fn skip_ix(ranges: &mut SmallVec<[Range<usize>; 4]>, ix: usize) {
            if let Some(last_range) = ranges.last_mut()
                && last_range.end == ix
            {
                last_range.end += 1;
            } else {
                ranges.push(ix..ix + 1);
            }
        }

        // Check for events inside .git directories, so that we know which repositories need their git state reloaded.
        //
        // Certain directories may have FS changes, but do not lead to git data changes that Mav cares about.
        // Ignore these, to avoid Mav unnecessarily rescanning git metadata.
        let skipped_file_names_in_dot_git =
            [COMMIT_MESSAGE, FETCH_HEAD, ORIG_HEAD, BISECT_LOG, GC_PID];
        let skipped_dirs_in_dot_git = [
            FSMONITOR_DAEMON,
            LFS_DIR,
            OBJECTS_DIR,
            HOOKS_DIR,
            REBASE_MERGE_DIR,
            REBASE_APPLY_DIR,
            SEQUENCER_DIR,
        ];

        let mut dot_git_abs_paths = Vec::new();
        let mut work_dirs_needing_exclude_update = Vec::new();

        {
            let snapshot = &self.state.lock().await.snapshot;

            let mut ranges_to_drop = SmallVec::<[Range<usize>; 4]>::new();

            for (ix, event) in events.iter().enumerate() {
                let abs_path = SanitizedPath::new(&event.path);

                let mut dot_git_paths = None;

                if self.track_git_repositories {
                    for ancestor in abs_path.as_path().ancestors() {
                        if is_dot_git(ancestor, self.fs.as_ref()).await {
                            let path_in_git_dir = abs_path
                                .as_path()
                                .strip_prefix(ancestor)
                                .expect("stripping off the ancestor");
                            dot_git_paths = Some((ancestor.to_owned(), path_in_git_dir.to_owned()));
                            break;
                        }
                    }
                }

                if let Some((dot_git_abs_path, path_in_git_dir)) = dot_git_paths {
                    let is_ignored = skipped_file_names_in_dot_git.iter().any(|skipped| {
                        path_in_git_dir
                            .file_name()
                            .is_some_and(|file_name| file_name == OsStr::new(skipped))
                    }) || (path_in_git_dir.starts_with(LOGS_DIR)
                        && path_in_git_dir != Path::new(LOGS_REF_STASH))
                        || (path_in_git_dir.starts_with(INFO_DIR)
                            && path_in_git_dir != Path::new(REPO_EXCLUDE))
                        || skipped_dirs_in_dot_git.iter().any(|skipped_git_subdir| {
                            path_in_git_dir.starts_with(skipped_git_subdir)
                        })
                        || path_in_git_dir.extension().is_some_and(|ext| ext == "lock")
                        || (path_in_git_dir.components().count() == 1
                            && path_in_git_dir
                                .extension()
                                .is_some_and(|ext| ext == "new" || ext == "tmp"));
                    let is_dot_git = path_in_git_dir == Path::new("")
                        && matches!(event.kind, Some(PathEventKind::Changed))
                        && self.fs.is_dir(&dot_git_abs_path).await;
                    if is_ignored {
                        log::debug!(
                            "ignoring event {abs_path:?} as it's in the .git directory among skipped files or directories"
                        );
                        skip_ix(&mut ranges_to_drop, ix);
                        continue;
                    }
                    if is_dot_git {
                        log::debug!(
                            "ignoring event {abs_path:?} for .git directory itself (kind: {:?})",
                            event.kind
                        );
                        skip_ix(&mut ranges_to_drop, ix);
                        continue;
                    }

                    if !dot_git_abs_paths.contains(&dot_git_abs_path) {
                        log::debug!(
                            "detected update within git repo at {dot_git_abs_path:?}: {abs_path:?}"
                        );
                        dot_git_abs_paths.push(dot_git_abs_path);
                    }
                }

                if self.track_git_repositories
                    && abs_path
                        .as_path()
                        .ends_with(Path::new(DOT_GIT).join(REPO_EXCLUDE))
                {
                    if let Some(repository) = snapshot.git_repositories.values().find(|repo| {
                        repo.common_dir_abs_path.join(REPO_EXCLUDE) == abs_path.as_path()
                    }) {
                        work_dirs_needing_exclude_update
                            .push(repository.work_directory_abs_path.clone());
                    }
                }
            }

            for range_to_drop in ranges_to_drop.into_iter().rev() {
                events.drain(range_to_drop);
            }
        }

        events.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        events.dedup_by(|left, right| {
            if left.path == right.path {
                if matches!(left.kind, Some(fs::PathEventKind::Rescan)) {
                    right.kind = left.kind;
                }
                true
            } else if left.path.starts_with(&right.path) {
                if matches!(left.kind, Some(fs::PathEventKind::Rescan)) {
                    right.kind = left.kind;
                }
                true
            } else {
                false
            }
        });

        let mut relative_paths = Vec::with_capacity(events.len());

        {
            let snapshot = &self.state.lock().await.snapshot;

            let mut ranges_to_drop = SmallVec::<[Range<usize>; 4]>::new();

            for (ix, event) in events.iter().enumerate() {
                let abs_path = SanitizedPath::new(&event.path);
                // TODO: this strips the root case-sensitively, so on a case-insensitive
                // volume an event whose casing differs from the canonical root is
                // dropped. Once `fs` exposes per-volume case-sensitivity (e.g. on the
                // `Fs` trait, with a per-volume cache + `FakeFs` support), fold this
                // comparison on case-insensitive volumes.
                let relative_path = if let Ok(path) = abs_path.strip_prefix(&root_canonical_path)
                    && let Ok(path) = RelPath::new(path, PathStyle::local())
                {
                    path
                } else if let Ok(path) = abs_path.strip_prefix(&root_path)
                    && let Ok(path) = RelPath::new(path, PathStyle::local())
                {
                    path
                } else if let Some(path) = snapshot.external_canonical_to_relative.iter().find_map(
                    |(canonical, relative)| {
                        abs_path
                            .as_path()
                            .strip_prefix(canonical.as_ref())
                            .ok()
                            .and_then(|suffix| {
                                RelPath::new(suffix, PathStyle::local())
                                    .ok()
                                    .map(|suffix_rel| {
                                        std::borrow::Cow::Owned(
                                            relative.join(&suffix_rel).to_rel_path_buf(),
                                        )
                                    })
                            })
                    },
                ) {
                    path
                } else {
                    skip_ix(&mut ranges_to_drop, ix);
                    continue;
                };

                if self.track_git_repositories
                    && abs_path.file_name() == Some(OsStr::new(GITIGNORE))
                {
                    for (_, repo) in snapshot
                        .git_repositories
                        .iter()
                        .filter(|(_, repo)| repo.directory_contains(&relative_path))
                    {
                        if !dot_git_abs_paths.iter().any(|dot_git_abs_path| {
                            dot_git_abs_path == repo.common_dir_abs_path.as_ref()
                        }) {
                            dot_git_abs_paths.push(repo.common_dir_abs_path.to_path_buf());
                        }
                    }
                }

                let parent_dir_is_loaded = relative_path.parent().is_none_or(|parent| {
                    snapshot
                        .entry_for_path(parent)
                        .is_some_and(|entry| entry.kind == EntryKind::Dir)
                });
                if !parent_dir_is_loaded {
                    log::debug!("filtering event {relative_path:?} within unloaded directory");
                    skip_ix(&mut ranges_to_drop, ix);
                    continue;
                }

                if self.settings.is_path_excluded(&relative_path) {
                    skip_ix(&mut ranges_to_drop, ix);
                    continue;
                }

                relative_paths.push(EventRoot {
                    path: relative_path.into_arc(),
                    was_rescanned: matches!(event.kind, Some(fs::PathEventKind::Rescan)),
                });
            }

            for range_to_drop in ranges_to_drop.into_iter().rev() {
                events.drain(range_to_drop);
            }
        }

        if relative_paths.is_empty() && dot_git_abs_paths.is_empty() {
            return;
        }

        if !work_dirs_needing_exclude_update.is_empty() {
            let mut state = self.state.lock().await;
            for work_dir_abs_path in work_dirs_needing_exclude_update {
                if let Some((_, needs_update)) = state
                    .snapshot
                    .repo_exclude_by_work_dir_abs_path
                    .get_mut(&work_dir_abs_path)
                {
                    *needs_update = true;
                }
            }
        }

        self.state.lock().await.snapshot.scan_id += 1;

        let (scan_job_tx, scan_job_rx) = async_channel::unbounded();
        if !relative_paths.is_empty() {
            log::debug!(
                "will update project paths {:?}",
                relative_paths
                    .iter()
                    .map(|event_root| &event_root.path)
                    .collect::<Vec<_>>()
            );
        }
        self.reload_entries_for_paths(
            &root_path,
            &root_canonical_path,
            &relative_paths
                .iter()
                .map(|event_root| event_root.path.clone())
                .collect::<Vec<_>>(),
            events
                .into_iter()
                .map(|event| event.path)
                .collect::<Vec<_>>(),
            Some(scan_job_tx.clone()),
        )
        .await;

        let affected_repo_roots = if !dot_git_abs_paths.is_empty() {
            self.update_git_repositories(dot_git_abs_paths).await
        } else {
            Vec::new()
        };

        {
            let mut ignores_to_update = self.ignores_needing_update().await;
            ignores_to_update.extend(affected_repo_roots);
            let ignores_to_update = self.order_ignores(ignores_to_update).await;
            let snapshot = self.state.lock().await.snapshot.clone();
            self.update_ignore_statuses_for_paths(scan_job_tx, snapshot, ignores_to_update)
                .await;
            self.scan_dirs(false, scan_job_rx).await;
        }

        {
            let mut state = self.state.lock().await;
            state.snapshot.completed_scan_id = state.snapshot.scan_id;
            for (_, entry) in mem::take(&mut state.removed_entries) {
                state.scanned_dirs.remove(&entry.id);
            }
        }
        self.send_status_update(false, SmallVec::new(), &relative_paths)
            .await;
    }

    async fn update_global_gitignore(&self, abs_path: &Path) {
        let ignore = build_gitignore(abs_path, self.fs.as_ref())
            .await
            .log_err()
            .map(Arc::new);
        let (prev_snapshot, ignore_stack, abs_path) = {
            let mut state = self.state.lock().await;
            state.snapshot.global_gitignore = ignore;
            let abs_path = state.snapshot.abs_path().clone();
            let ignore_stack = state
                .snapshot
                .ignore_stack_for_abs_path(&abs_path, true, self.fs.as_ref())
                .await;
            (state.snapshot.clone(), ignore_stack, abs_path)
        };
        let (scan_job_tx, scan_job_rx) = async_channel::unbounded();
        self.update_ignore_statuses_for_paths(
            scan_job_tx,
            prev_snapshot,
            vec![(abs_path, ignore_stack)],
        )
        .await;
        self.scan_dirs(false, scan_job_rx).await;
        self.send_status_update(false, SmallVec::new(), &[]).await;
    }

    async fn forcibly_load_paths(&self, paths: &[Arc<RelPath>]) -> bool {
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

    async fn scan_dirs(
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

    async fn send_status_update(
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

    async fn scan_dir(&self, job: &ScanJob) -> Result<()> {
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

    /// All list arguments should be sorted before calling this function
    async fn reload_entries_for_paths(
        &self,
        root_abs_path: &SanitizedPath,
        root_canonical_path: &SanitizedPath,
        relative_paths: &[Arc<RelPath>],
        abs_paths: Vec<PathBuf>,
        scan_queue_tx: Option<Sender<ScanJob>>,
    ) {
        // grab metadata for all requested paths
        let metadata = futures::future::join_all(
            abs_paths
                .iter()
                .map(|abs_path| async move {
                    let metadata = self.fs.metadata(abs_path).await?;
                    if let Some(metadata) = metadata {
                        let canonical_path = self.fs.canonicalize(abs_path).await?;

                        // If we're on a case-insensitive filesystem (default on macOS), we want
                        // to only ignore metadata for non-symlink files if their absolute-path matches
                        // the canonical-path.
                        // Because if not, this might be a case-only-renaming (`mv test.txt TEST.TXT`)
                        // and we want to ignore the metadata for the old path (`test.txt`) so it's
                        // treated as removed.
                        if !self.fs_case_sensitive && !metadata.is_symlink {
                            let canonical_file_name = canonical_path.file_name();
                            let file_name = abs_path.file_name();
                            if canonical_file_name != file_name {
                                return Ok(None);
                            }
                        }

                        anyhow::Ok(Some((metadata, SanitizedPath::new_arc(&canonical_path))))
                    } else {
                        Ok(None)
                    }
                })
                .collect::<Vec<_>>(),
        )
        .await;

        let mut new_ancestor_repo =
            if self.track_git_repositories && relative_paths.iter().any(|path| path.is_empty()) {
                Some(discover_ancestor_git_repo(self.fs.clone(), &root_abs_path).await)
            } else {
                None
            };

        let mut state = self.state.lock().await;
        let doing_recursive_update = scan_queue_tx.is_some();

        // Remove any entries for paths that no longer exist or are being recursively
        // refreshed. Do this before adding any new entries, so that renames can be
        // detected regardless of the order of the paths.
        let mut paths_to_process = Vec::with_capacity(relative_paths.len());
        for (path, metadata) in relative_paths.iter().zip(metadata.iter()) {
            let path_was_removed = matches!(metadata, Ok(None));
            let removed_descendant_paths = if path_was_removed || doing_recursive_update {
                state.remove_path_from_snapshot(path, path_was_removed)
            } else {
                Vec::new()
            };
            paths_to_process.push((path, metadata, removed_descendant_paths));
        }

        for (path, metadata, removed_descendant_abs_paths) in paths_to_process {
            let abs_path: Arc<Path> = root_abs_path.join(path.as_std_path()).into();
            match metadata {
                Ok(Some((metadata, canonical_path))) => {
                    let ignore_stack = state
                        .snapshot
                        .ignore_stack_for_abs_path(&abs_path, metadata.is_dir, self.fs.as_ref())
                        .await;
                    let is_external = !canonical_path.starts_with(&root_canonical_path);
                    let entry_id = state.entry_id_for(self.next_entry_id.as_ref(), path, &metadata);
                    let mut fs_entry = Entry::new(
                        path.clone(),
                        &metadata,
                        entry_id,
                        state.snapshot.root_char_bag,
                        if metadata.is_symlink {
                            Some(canonical_path.as_path().to_path_buf().into())
                        } else {
                            None
                        },
                    );

                    let is_dir = fs_entry.is_dir();
                    fs_entry.is_ignored = ignore_stack.is_abs_path_ignored(&abs_path, is_dir);
                    fs_entry.is_external = is_external;
                    fs_entry.is_private = self.is_path_private(path);
                    fs_entry.is_always_included =
                        self.settings.is_path_always_included(path, is_dir);
                    fs_entry.is_hidden = self.settings.is_path_hidden(path);

                    if let (Some(scan_queue_tx), true) = (&scan_queue_tx, is_dir) {
                        if self.should_scan_directory(&state, &fs_entry)
                            || (self.track_git_repositories
                                && fs_entry.path.is_empty()
                                && abs_path.file_name() == Some(OsStr::new(DOT_GIT)))
                        {
                            state
                                .enqueue_scan_dir(
                                    abs_path,
                                    &fs_entry,
                                    scan_queue_tx,
                                    self.fs.as_ref(),
                                )
                                .await;
                        } else {
                            fs_entry.kind = EntryKind::UnloadedDir;
                        }
                    }

                    state
                        .insert_entry(fs_entry.clone(), self.fs.as_ref(), self.watcher.as_ref())
                        .await;

                    if path.is_empty()
                        && let Some((ignores, exclude, repo)) = new_ancestor_repo.take()
                    {
                        log::trace!("updating ancestor git repository");
                        state.snapshot.ignores_by_parent_abs_path.extend(ignores);
                        if let Some((ancestor_dot_git, work_directory)) = repo {
                            if let Some(exclude) = exclude {
                                let work_directory_abs_path =
                                    state.snapshot.work_directory_abs_path(&work_directory);

                                state
                                    .snapshot
                                    .repo_exclude_by_work_dir_abs_path
                                    .insert(work_directory_abs_path.into(), (exclude, false));
                            }
                            state
                                .insert_git_repository_for_path(
                                    work_directory,
                                    ancestor_dot_git.into(),
                                    self.fs.as_ref(),
                                    self.watcher.as_ref(),
                                )
                                .await
                                .log_err();
                        }
                    }
                }
                Ok(None) => {
                    self.remove_repo_path(path.clone(), &mut state.snapshot);
                    state.unwatch_path(
                        self.watcher.as_ref(),
                        path,
                        removed_descendant_abs_paths,
                        false,
                    );
                }
                Err(err) => {
                    log::error!("error reading file {abs_path:?} on event: {err:#}");
                    state.unwatch_path(
                        self.watcher.as_ref(),
                        path,
                        removed_descendant_abs_paths,
                        false,
                    );
                }
            }
        }

        util::extend_sorted(
            &mut state.changed_paths,
            relative_paths.iter().cloned(),
            usize::MAX,
            Ord::cmp,
        );
    }

    fn remove_repo_path(&self, path: Arc<RelPath>, snapshot: &mut LocalSnapshot) -> Option<()> {
        if !path.components().any(|component| component == DOT_GIT)
            && let Some(local_repo) = snapshot.local_repo_for_work_directory_path(&path)
        {
            let id = local_repo.work_directory_id;
            log::debug!("remove repo path: {:?}", path);
            snapshot.git_repositories.remove(&id);
            return Some(());
        }

        Some(())
    }

    async fn update_ignore_statuses_for_paths(
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

    async fn ignores_needing_update(&self) -> Vec<Arc<Path>> {
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

    async fn order_ignores(&self, mut ignores: Vec<Arc<Path>>) -> Vec<(Arc<Path>, IgnoreStack)> {
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

    async fn update_ignore_status(&self, job: UpdateIgnoreStatusJob, snapshot: &LocalSnapshot) {
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

    async fn update_git_repositories(&self, dot_git_paths: Vec<PathBuf>) -> Vec<Arc<Path>> {
        log::trace!("reloading repositories: {dot_git_paths:?}");
        let mut state = self.state.lock().await;
        let scan_id = state.snapshot.scan_id;
        let mut affected_repo_roots = Vec::new();
        for dot_git_dir in dot_git_paths {
            let existing_repository_entry =
                state
                    .snapshot
                    .git_repositories
                    .iter()
                    .find_map(|(_, repo)| {
                        let dot_git_dir = SanitizedPath::new(&dot_git_dir);
                        if SanitizedPath::new(repo.common_dir_abs_path.as_ref()) == dot_git_dir
                            || SanitizedPath::new(repo.repository_dir_abs_path.as_ref())
                                == dot_git_dir
                            || SanitizedPath::new(repo.dot_git_abs_path.as_ref()) == dot_git_dir
                        {
                            Some(repo.clone())
                        } else {
                            None
                        }
                    });

            match existing_repository_entry {
                None => {
                    let Ok(relative) = dot_git_dir.strip_prefix(state.snapshot.abs_path()) else {
                        // A `.git` path outside the worktree root is not
                        // ours to register. This happens legitimately when
                        // `.git` is a gitfile pointing outside the worktree
                        // (linked worktrees and submodules), and also when
                        // a rescan of a linked worktree's commondir arrives
                        // after the worktree's repository has already been
                        // unregistered.
                        continue;
                    };
                    affected_repo_roots.push(dot_git_dir.parent().unwrap().into());
                    state
                        .insert_git_repository(
                            RelPath::new(relative, PathStyle::local())
                                .unwrap()
                                .into_arc(),
                            self.fs.as_ref(),
                            self.watcher.as_ref(),
                        )
                        .await;
                }
                Some(local_repository) => {
                    state.snapshot.git_repositories.update(
                        &local_repository.work_directory_id,
                        |entry| {
                            entry.git_dir_scan_id = scan_id;
                        },
                    );
                }
            };
        }

        // Remove any git repositories whose .git entry no longer exists.
        let snapshot = &mut state.snapshot;
        let mut ids_to_preserve = HashSet::default();
        for (&work_directory_id, entry) in snapshot.git_repositories.iter() {
            let exists_in_snapshot =
                snapshot
                    .entry_for_id(work_directory_id)
                    .is_some_and(|entry| {
                        snapshot
                            .entry_for_path(&entry.path.join(RelPath::unix(DOT_GIT).unwrap()))
                            .is_some()
                    });

            // Only drop a repository when we can positively confirm that its git
            // directory is gone. `metadata` returns `Ok(None)` for a confirmed
            // absence, but `Err(_)` for a transient failure (which can happen
            // under heavy filesystem churn). Treating an error as a deletion
            // makes the repository flap out of and back into the snapshot,
            // causing the GitStore to repeatedly tear it down and re-create it
            // with a fresh `RepositoryId`. So preserve the repository unless the
            // `.git` entry is confirmed absent.
            let dot_git_present =
                !matches!(self.fs.metadata(&entry.dot_git_abs_path).await, Ok(None));

            if exists_in_snapshot || dot_git_present {
                ids_to_preserve.insert(work_directory_id);
            }
        }

        snapshot
            .git_repositories
            .retain(|work_directory_id, entry| {
                let preserve = ids_to_preserve.contains(work_directory_id);
                if !preserve {
                    affected_repo_roots.push(entry.dot_git_abs_path.parent().unwrap().into());
                    snapshot
                        .repo_exclude_by_work_dir_abs_path
                        .remove(&entry.work_directory_abs_path);
                }
                preserve
            });

        affected_repo_roots
    }

    async fn progress_timer(&self, running: bool) {
        if !running {
            return futures::future::pending().await;
        }

        #[cfg(feature = "test-support")]
        if self.fs.is_fake() {
            return self.executor.simulate_random_delay().await;
        }

        self.executor.timer(FS_WATCH_LATENCY).await
    }

    fn is_path_private(&self, path: &RelPath) -> bool {
        !self.share_private_files && self.settings.is_path_private(path)
    }

    fn should_scan_directory(&self, state: &BackgroundScannerState, entry: &Entry) -> bool {
        let scannable = state.scanning_enabled
            && (!entry.is_external
                || self.settings.scan_symlinks == settings::ScanSymlinksSetting::Always)
            && (!entry.is_ignored || entry.is_always_included);

        scannable
            || entry.path.file_name() == Some(DOT_GIT)
            || entry.path.file_name() == Some(local_settings_folder_name())
            || entry.path.file_name() == Some(local_vscode_folder_name())
            || state.scanned_dirs.contains(&entry.id) // If we've ever scanned it, keep scanning
            || state
                .paths_to_scan
                .iter()
                .any(|p| p.starts_with(&entry.path))
            || state
                .path_prefixes_to_scan
                .iter()
                .any(|p| entry.path.starts_with(p))
    }

    async fn next_scan_request(&self) -> Result<ScanRequest> {
        let mut request = self.scan_requests_rx.recv().await?;
        while let Ok(next_request) = self.scan_requests_rx.try_recv() {
            request.relative_paths.extend(next_request.relative_paths);
            request.done.extend(next_request.done);
        }
        Ok(request)
    }
}
