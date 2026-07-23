use super::*;

impl BackgroundScanner {
    pub(super) fn normalized_events_for_worktree(
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

    pub(super) async fn process_events(&self, mut events: Vec<PathEvent>) {
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
}
