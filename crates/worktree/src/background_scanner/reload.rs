use super::*;

impl BackgroundScanner {
    /// All list arguments should be sorted before calling this function
    pub(super) async fn reload_entries_for_paths(
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

    pub(super) fn remove_repo_path(
        &self,
        path: Arc<RelPath>,
        snapshot: &mut LocalSnapshot,
    ) -> Option<()> {
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
}
