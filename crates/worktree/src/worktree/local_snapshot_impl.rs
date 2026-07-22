use super::*;

impl LocalSnapshot {
    pub(super) fn local_repo_for_work_directory_path(
        &self,
        path: &RelPath,
    ) -> Option<&LocalRepositoryEntry> {
        self.git_repositories
            .iter()
            .map(|(_, entry)| entry)
            .find(|entry| entry.work_directory.path_key() == PathKey(path.into()))
    }

    pub(super) fn build_update(
        &self,
        project_id: u64,
        worktree_id: u64,
        entry_changes: UpdatedEntriesSet,
    ) -> proto::UpdateWorktree {
        let mut updated_entries = Vec::new();
        let mut removed_entries = Vec::new();

        for (_, entry_id, path_change) in entry_changes.iter() {
            if let PathChange::Removed = path_change {
                removed_entries.push(entry_id.0 as u64);
            } else if let Some(entry) = self.entry_for_id(*entry_id) {
                updated_entries.push(proto::Entry::from(entry));
            }
        }

        removed_entries.sort_unstable();
        updated_entries.sort_unstable_by_key(|e| e.id);

        // TODO - optimize, knowing that removed_entries are sorted.
        removed_entries.retain(|id| updated_entries.binary_search_by_key(id, |e| e.id).is_err());

        proto::UpdateWorktree {
            project_id,
            worktree_id,
            abs_path: self.abs_path().to_string_lossy().into_owned(),
            root_name: self.root_name().to_proto(),
            root_repo_common_dir: self
                .root_repo_common_dir()
                .map(|p| p.to_string_lossy().into_owned()),
            updated_entries,
            removed_entries,
            scan_id: self.scan_id as u64,
            is_last_update: self.completed_scan_id == self.scan_id,
            // Sent in separate messages.
            updated_repositories: Vec::new(),
            removed_repositories: Vec::new(),
        }
    }

    pub(super) async fn insert_entry(&mut self, mut entry: Entry, fs: &dyn Fs) -> Entry {
        log::trace!("insert entry {:?}", entry.path);
        if entry.is_file() && entry.path.file_name() == Some(&GITIGNORE) {
            let abs_path = self.absolutize(&entry.path);
            match build_gitignore(&abs_path, fs).await {
                Ok(ignore) => {
                    self.ignores_by_parent_abs_path
                        .insert(abs_path.parent().unwrap().into(), (Arc::new(ignore), true));
                }
                Err(error) => {
                    log::error!(
                        "error loading .gitignore file {:?} - {:?}",
                        &entry.path,
                        error
                    );
                }
            }
        }

        if entry.kind == EntryKind::PendingDir
            && let Some(existing_entry) = self.entries_by_path.get(&PathKey(entry.path.clone()), ())
        {
            entry.kind = existing_entry.kind;
        }

        let scan_id = self.scan_id;
        let removed = self.entries_by_path.insert_or_replace(entry.clone(), ());
        if let Some(removed) = removed
            && removed.id != entry.id
        {
            self.entries_by_id.remove(&removed.id, ());
        }
        self.entries_by_id.insert_or_replace(
            PathEntry {
                id: entry.id,
                path: entry.path.clone(),
                is_ignored: entry.is_ignored,
                scan_id,
            },
            (),
        );

        entry
    }

    pub(super) fn ancestor_inodes_for_path(&self, path: &RelPath) -> TreeSet<u64> {
        let mut inodes = TreeSet::default();
        for ancestor in path.ancestors().skip(1) {
            if let Some(entry) = self.entry_for_path(ancestor) {
                inodes.insert(entry.inode);
            }
        }
        inodes
    }

    pub(super) async fn ignore_stack_for_abs_path(
        &self,
        abs_path: &Path,
        is_dir: bool,
        fs: &dyn Fs,
    ) -> IgnoreStack {
        let mut new_ignores = Vec::new();
        let mut repo_root = None;
        for (index, ancestor) in abs_path.ancestors().enumerate() {
            if index > 0 {
                if let Some((ignore, _)) = self.ignores_by_parent_abs_path.get(ancestor) {
                    new_ignores.push((ancestor, Some(ignore.clone())));
                } else {
                    new_ignores.push((ancestor, None));
                }
            }

            let metadata = fs.metadata(&ancestor.join(DOT_GIT)).await.ok().flatten();
            if metadata.is_some() {
                repo_root = Some(Arc::from(ancestor));
                break;
            }
        }

        let mut ignore_stack = if let Some(global_gitignore) = self.global_gitignore.clone() {
            IgnoreStack::global(global_gitignore)
        } else {
            IgnoreStack::none()
        };

        if let Some((repo_exclude, _)) = repo_root
            .as_ref()
            .and_then(|abs_path| self.repo_exclude_by_work_dir_abs_path.get(abs_path))
        {
            ignore_stack = ignore_stack.append(IgnoreKind::RepoExclude, repo_exclude.clone());
        }
        ignore_stack.repo_root = repo_root;
        for (parent_abs_path, ignore) in new_ignores.into_iter().rev() {
            if ignore_stack.is_abs_path_ignored(parent_abs_path, true) {
                ignore_stack = IgnoreStack::all();
                break;
            } else if let Some(ignore) = ignore {
                ignore_stack =
                    ignore_stack.append(IgnoreKind::Gitignore(parent_abs_path.into()), ignore);
            }
        }

        if ignore_stack.is_abs_path_ignored(abs_path, is_dir) {
            ignore_stack = IgnoreStack::all();
        }

        ignore_stack
    }

    #[cfg(feature = "test-support")]
    pub fn expanded_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries_by_path
            .cursor::<()>(())
            .filter(|entry| entry.kind == EntryKind::Dir && (entry.is_external || entry.is_ignored))
    }

    #[cfg(feature = "test-support")]
    pub fn check_invariants(&self, git_state: bool) {
        use pretty_assertions::assert_eq;

        assert_eq!(
            self.entries_by_path
                .cursor::<()>(())
                .map(|e| (&e.path, e.id))
                .collect::<Vec<_>>(),
            self.entries_by_id
                .cursor::<()>(())
                .map(|e| (&e.path, e.id))
                .collect::<collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
            "entries_by_path and entries_by_id are inconsistent"
        );

        let mut files = self.files(true, 0);
        let mut visible_files = self.files(false, 0);
        for entry in self.entries_by_path.cursor::<()>(()) {
            if entry.is_file() {
                assert_eq!(files.next().unwrap().inode, entry.inode);
                if !entry.is_ignored || entry.is_always_included {
                    assert_eq!(visible_files.next().unwrap().inode, entry.inode);
                }
            }
        }

        assert!(files.next().is_none());
        assert!(visible_files.next().is_none());

        let mut bfs_paths = Vec::new();
        let mut stack = self
            .root_entry()
            .map(|e| e.path.as_ref())
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(path) = stack.pop() {
            bfs_paths.push(path);
            let ix = stack.len();
            for child_entry in self.child_entries(path) {
                stack.insert(ix, &child_entry.path);
            }
        }

        let dfs_paths_via_iter = self
            .entries_by_path
            .cursor::<()>(())
            .map(|e| e.path.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(bfs_paths, dfs_paths_via_iter);

        let dfs_paths_via_traversal = self
            .entries(true, 0)
            .map(|e| e.path.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(dfs_paths_via_traversal, dfs_paths_via_iter);

        if git_state {
            for ignore_parent_abs_path in self.ignores_by_parent_abs_path.keys() {
                let ignore_parent_path = &RelPath::new(
                    ignore_parent_abs_path
                        .strip_prefix(self.abs_path.as_path())
                        .unwrap(),
                    PathStyle::local(),
                )
                .unwrap();
                assert!(self.entry_for_path(ignore_parent_path).is_some());
                assert!(
                    self.entry_for_path(
                        &ignore_parent_path.join(RelPath::unix(GITIGNORE).unwrap())
                    )
                    .is_some()
                );
            }
        }
    }

    #[cfg(feature = "test-support")]
    pub fn entries_without_ids(&self, include_ignored: bool) -> Vec<(&RelPath, u64, bool)> {
        let mut paths = Vec::new();
        for entry in self.entries_by_path.cursor::<()>(()) {
            if include_ignored || !entry.is_ignored {
                paths.push((entry.path.as_ref(), entry.inode, entry.is_ignored));
            }
        }
        paths.sort_by(|a, b| a.0.cmp(b.0));
        paths
    }
}
