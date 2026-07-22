use super::*;

impl BackgroundScannerState {
    pub(super) async fn enqueue_scan_dir(
        &self,
        abs_path: Arc<Path>,
        entry: &Entry,
        scan_job_tx: &Sender<ScanJob>,
        fs: &dyn Fs,
    ) {
        let path = entry.path.clone();
        let ignore_stack = self
            .snapshot
            .ignore_stack_for_abs_path(&abs_path, true, fs)
            .await;
        let mut ancestor_inodes = self.snapshot.ancestor_inodes_for_path(&path);

        if !ancestor_inodes.contains(&entry.inode) {
            ancestor_inodes.insert(entry.inode);
            scan_job_tx
                .try_send(ScanJob {
                    abs_path,
                    path,
                    ignore_stack,
                    scan_queue: scan_job_tx.clone(),
                    ancestor_inodes,
                    is_external: entry.is_external,
                })
                .unwrap();
        }
    }

    pub(super) fn reuse_entry_id(&mut self, entry: &mut Entry) {
        if let Some(mtime) = entry.mtime {
            // If an entry with the same inode was removed from the worktree during this scan,
            // then it *might* represent the same file or directory. But the OS might also have
            // re-used the inode for a completely different file or directory.
            //
            // Conditionally reuse the old entry's id:
            // * if the mtime is the same, the file was probably been renamed.
            // * if the path is the same, the file may just have been updated
            if let Some(removed_entry) = self.removed_entries.remove(&entry.inode) {
                if removed_entry.mtime == Some(mtime) || removed_entry.path == entry.path {
                    entry.id = removed_entry.id;
                }
            } else if let Some(existing_entry) = self.snapshot.entry_for_path(&entry.path) {
                entry.id = existing_entry.id;
            }
        }
    }

    pub(super) fn entry_id_for(
        &mut self,
        next_entry_id: &AtomicUsize,
        path: &RelPath,
        metadata: &fs::Metadata,
    ) -> ProjectEntryId {
        // If an entry with the same inode was removed from the worktree during this scan,
        // then it *might* represent the same file or directory. But the OS might also have
        // re-used the inode for a completely different file or directory.
        //
        // Conditionally reuse the old entry's id:
        // * if the mtime is the same, the file was probably been renamed.
        // * if the path is the same, the file may just have been updated
        if let Some(removed_entry) = self.removed_entries.remove(&metadata.inode) {
            if removed_entry.mtime == Some(metadata.mtime) || *removed_entry.path == *path {
                return removed_entry.id;
            }
        } else if let Some(existing_entry) = self.snapshot.entry_for_path(path) {
            return existing_entry.id;
        }
        ProjectEntryId::new(next_entry_id)
    }

    pub(super) async fn insert_entry(
        &mut self,
        entry: Entry,
        fs: &dyn Fs,
        watcher: &dyn Watcher,
    ) -> Entry {
        let entry = self.snapshot.insert_entry(entry, fs).await;
        if entry.path.file_name() == Some(&DOT_GIT) {
            self.insert_git_repository(entry.path.clone(), fs, watcher)
                .await;
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);

        entry
    }

    pub(super) fn populate_dir(
        &mut self,
        parent_path: Arc<RelPath>,
        entries: impl IntoIterator<Item = Entry>,
        ignore: Option<Arc<Gitignore>>,
    ) {
        let mut parent_entry = if let Some(parent_entry) = self
            .snapshot
            .entries_by_path
            .get(&PathKey(parent_path.clone()), ())
        {
            parent_entry.clone()
        } else {
            log::warn!(
                "populating a directory {:?} that has been removed",
                parent_path
            );
            return;
        };

        match parent_entry.kind {
            EntryKind::PendingDir | EntryKind::UnloadedDir => parent_entry.kind = EntryKind::Dir,
            EntryKind::Dir => {}
            _ => return,
        }

        if let Some(ignore) = ignore {
            let abs_parent_path = self
                .snapshot
                .abs_path
                .as_path()
                .join(parent_path.as_std_path())
                .into();
            self.snapshot
                .ignores_by_parent_abs_path
                .insert(abs_parent_path, (ignore, false));
        }

        let parent_entry_id = parent_entry.id;
        self.scanned_dirs.insert(parent_entry_id);
        let mut entries_by_path_edits = vec![Edit::Insert(parent_entry)];
        let mut entries_by_id_edits = Vec::new();

        for entry in entries {
            entries_by_id_edits.push(Edit::Insert(PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id: self.snapshot.scan_id,
            }));
            entries_by_path_edits.push(Edit::Insert(entry));
        }

        self.snapshot
            .entries_by_path
            .edit(entries_by_path_edits, ());
        self.snapshot.entries_by_id.edit(entries_by_id_edits, ());

        if let Err(ix) = self.changed_paths.binary_search(&parent_path) {
            self.changed_paths.insert(ix, parent_path.clone());
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);
    }

    pub(super) fn remove_path_from_snapshot_and_unwatch(
        &mut self,
        path: &RelPath,
        watcher: &dyn Watcher,
        preserve_repository_watches: bool,
    ) {
        // When the caller preserves repository watches, it intends to re-scan
        // this subtree and keep its git repositories; pruning them here would
        // transiently drop and then re-create them with fresh `RepositoryId`s.
        let prune_repositories = !preserve_repository_watches;
        let removed_descendant_abs_paths = self.remove_path_from_snapshot(path, prune_repositories);
        self.unwatch_path(
            watcher,
            path,
            removed_descendant_abs_paths,
            preserve_repository_watches,
        );
    }

    pub(super) fn unwatch_path(
        &mut self,
        watcher: &dyn Watcher,
        path: &RelPath,
        removed_descendant_abs_paths: Vec<PathBuf>,
        preserve_repository_watches: bool,
    ) {
        let mut repository_watches_to_preserve = HashSet::<Arc<Path>>::default();
        if preserve_repository_watches {
            for repository in self.snapshot.git_repositories.values() {
                repository_watches_to_preserve.insert(repository.common_dir_abs_path.clone());
                repository_watches_to_preserve.insert(repository.repository_dir_abs_path.clone());
            }
        }

        for removed_dir_abs_path in removed_descendant_abs_paths {
            if repository_watches_to_preserve.contains(removed_dir_abs_path.as_path()) {
                continue;
            }
            watcher.remove(&removed_dir_abs_path).log_err();
        }

        self.snapshot
            .external_canonical_to_relative
            .retain(|canonical, relative| {
                if relative.starts_with(path) {
                    if !repository_watches_to_preserve.contains(canonical.as_ref()) {
                        watcher.remove(canonical.as_ref()).log_err();
                    }
                    false
                } else {
                    true
                }
            });
    }

    pub(super) fn remove_path_from_snapshot(
        &mut self,
        path: &RelPath,
        prune_repositories: bool,
    ) -> Vec<PathBuf> {
        log::trace!("background scanner removing path {path:?}");
        let mut new_entries;
        let removed_entries;
        {
            let mut cursor = self
                .snapshot
                .entries_by_path
                .cursor::<TraversalProgress>(());
            new_entries = cursor.slice(&TraversalTarget::path(path), Bias::Left);
            removed_entries = cursor.slice(&TraversalTarget::successor(path), Bias::Left);
            new_entries.append(cursor.suffix(), ());
        }
        self.snapshot.entries_by_path = new_entries;

        let mut removed_ids = Vec::with_capacity(removed_entries.summary().count);
        let mut removed_dir_abs_paths = Vec::new();
        for entry in removed_entries.cursor::<()>(()) {
            if entry.is_dir() {
                let watch_path = self
                    .watched_dir_abs_paths_by_entry_id
                    .remove(&entry.id)
                    .map(|path| path.as_ref().to_path_buf())
                    .unwrap_or_else(|| self.snapshot.absolutize(&entry.path));
                removed_dir_abs_paths.push(watch_path);
            }

            match self.removed_entries.entry(entry.inode) {
                hash_map::Entry::Occupied(mut e) => {
                    let prev_removed_entry = e.get_mut();
                    if entry.id > prev_removed_entry.id {
                        *prev_removed_entry = entry.clone();
                    }
                }
                hash_map::Entry::Vacant(e) => {
                    e.insert(entry.clone());
                }
            }

            if entry.path.file_name() == Some(GITIGNORE) {
                let abs_parent_path = self.snapshot.absolutize(&entry.path.parent().unwrap());
                if let Some((_, needs_update)) = self
                    .snapshot
                    .ignores_by_parent_abs_path
                    .get_mut(abs_parent_path.as_path())
                {
                    *needs_update = true;
                }
            }

            if let Err(ix) = removed_ids.binary_search(&entry.id) {
                removed_ids.insert(ix, entry.id);
            }
        }

        self.snapshot
            .entries_by_id
            .edit(removed_ids.iter().map(|&id| Edit::Remove(id)).collect(), ());

        // Only prune git repositories when the entries are being genuinely
        // removed. During a recursive refresh (e.g. a watcher-forced rescan),
        // the subtree is removed and immediately re-scanned; dropping the
        // repositories here would make them flap, causing the GitStore to
        // tear them down and re-create them with fresh `RepositoryId`s. Stale
        // repositories are instead reaped authoritatively (against the actual
        // filesystem) in `update_git_repositories`.
        if prune_repositories {
            self.snapshot
                .git_repositories
                .retain(|id, _| removed_ids.binary_search(id).is_err());
        }

        #[cfg(feature = "test-support")]
        self.snapshot.check_invariants(false);

        removed_dir_abs_paths
    }

    pub(super) async fn insert_git_repository(
        &mut self,
        dot_git_path: Arc<RelPath>,
        fs: &dyn Fs,
        watcher: &dyn Watcher,
    ) {
        let work_dir_path: Arc<RelPath> = match dot_git_path.parent() {
            Some(parent_dir) => {
                // Guard against repositories inside the repository metadata
                if parent_dir
                    .components()
                    .any(|component| component == DOT_GIT)
                {
                    log::debug!(
                        "not building git repository for nested `.git` directory, `.git` path in the worktree: {dot_git_path:?}"
                    );
                    return;
                };

                parent_dir.into()
            }
            None => {
                // `dot_git_path.parent().is_none()` means `.git` directory is the opened worktree itself,
                // no files inside that directory are tracked by git, so no need to build the repo around it
                log::debug!(
                    "not building git repository for the worktree itself, `.git` path in the worktree: {dot_git_path:?}"
                );
                return;
            }
        };

        let dot_git_abs_path = Arc::from(self.snapshot.absolutize(&dot_git_path).as_ref());

        self.insert_git_repository_for_path(
            WorkDirectory::InProject {
                relative_path: work_dir_path,
            },
            dot_git_abs_path,
            fs,
            watcher,
        )
        .await
        .log_err();
    }

    pub(super) async fn insert_git_repository_for_path(
        &mut self,
        work_directory: WorkDirectory,
        dot_git_abs_path: Arc<Path>,
        fs: &dyn Fs,
        watcher: &dyn Watcher,
    ) -> Result<LocalRepositoryEntry> {
        let work_dir_entry = self
            .snapshot
            .entry_for_path(&work_directory.path_key().0)
            .with_context(|| {
                format!(
                    "working directory `{}` not indexed",
                    work_directory
                        .path_key()
                        .0
                        .display(self.snapshot.path_style)
                )
            })?;
        let work_directory_abs_path = self.snapshot.work_directory_abs_path(&work_directory);

        let (repository_dir_abs_path, common_dir_abs_path) =
            discover_git_paths(&dot_git_abs_path, fs).await;
        watcher
            .add(&common_dir_abs_path)
            .context("failed to add common directory to watcher")
            .log_err();
        watcher
            .add(&repository_dir_abs_path)
            .context("failed to add repository directory to watcher")
            .log_err();

        // On Linux and FreeBSD, the native watcher is non-recursive, so subdirectories inside `.git` need explicit watching.
        // For repos using the reftable backend, watch the `.git/reftable` directory so that ref changes are detected.
        let reftable_path = common_dir_abs_path.join("reftable");
        if fs.is_dir(&reftable_path).await {
            watcher
                .add(&reftable_path)
                .context("failed to add reftable directory to watcher")
                .log_err();
        }

        let work_directory_id = work_dir_entry.id;

        let local_repository = LocalRepositoryEntry {
            work_directory_id,
            work_directory,
            work_directory_abs_path: work_directory_abs_path.as_path().into(),
            git_dir_scan_id: 0,
            dot_git_abs_path,
            common_dir_abs_path,
            repository_dir_abs_path,
        };

        self.snapshot
            .git_repositories
            .insert(work_directory_id, local_repository.clone());

        log::trace!("inserting new local git repository");
        Ok(local_repository)
    }
}
