use super::*;

#[cfg(feature = "test-support")]
impl FakeFs {
    pub fn paths(&self, include_dot_git: bool) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let mut queue = collections::VecDeque::new();
        let state = &*self.state.lock();
        queue.push_back((PathBuf::from(util::path!("/")), &state.root));
        while let Some((path, entry)) = queue.pop_front() {
            if let FakeFsEntry::Dir { entries, .. } = entry {
                for (name, entry) in entries {
                    queue.push_back((path.join(name), entry));
                }
            }
            if include_dot_git
                || !path
                    .components()
                    .any(|component| component.as_os_str() == *FS_DOT_GIT)
            {
                result.push(path);
            }
        }
        result
    }

    pub fn directories(&self, include_dot_git: bool) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let mut queue = collections::VecDeque::new();
        let state = &*self.state.lock();
        queue.push_back((PathBuf::from(util::path!("/")), &state.root));
        while let Some((path, entry)) = queue.pop_front() {
            if let FakeFsEntry::Dir { entries, .. } = entry {
                for (name, entry) in entries {
                    queue.push_back((path.join(name), entry));
                }
                if include_dot_git
                    || !path
                        .components()
                        .any(|component| component.as_os_str() == *FS_DOT_GIT)
                {
                    result.push(path);
                }
            }
        }
        result
    }

    pub fn files(&self) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let mut queue = collections::VecDeque::new();
        let state = &*self.state.lock();
        queue.push_back((PathBuf::from(util::path!("/")), &state.root));
        while let Some((path, entry)) = queue.pop_front() {
            match entry {
                FakeFsEntry::File { .. } => result.push(path),
                FakeFsEntry::Dir { entries, .. } => {
                    for (name, entry) in entries {
                        queue.push_back((path.join(name), entry));
                    }
                }
                FakeFsEntry::Symlink { .. } => {}
            }
        }
        result
    }

    pub fn files_with_contents(&self, prefix: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut result = Vec::new();
        let mut queue = collections::VecDeque::new();
        let state = &*self.state.lock();
        queue.push_back((PathBuf::from(util::path!("/")), &state.root));
        while let Some((path, entry)) = queue.pop_front() {
            match entry {
                FakeFsEntry::File { content, .. } => {
                    if path.starts_with(prefix) {
                        result.push((path, content.clone()));
                    }
                }
                FakeFsEntry::Dir { entries, .. } => {
                    for (name, entry) in entries {
                        queue.push_back((path.join(name), entry));
                    }
                }
                FakeFsEntry::Symlink { .. } => {}
            }
        }
        result
    }

    /// How many `read_dir` calls have been issued.
    pub fn read_dir_call_count(&self) -> usize {
        self.state.lock().read_dir_call_count
    }

    pub fn watched_paths(&self) -> Vec<PathBuf> {
        let state = self.state.lock();
        state
            .event_txs
            .iter()
            .filter_map(|(path, tx)| Some(path.clone()).filter(|_| !tx.is_closed()))
            .collect()
    }

    /// How many `metadata` calls have been issued.
    pub fn metadata_call_count(&self) -> usize {
        self.state.lock().metadata_call_count
    }

    /// How many write operations have been issued for a specific path.
    pub fn write_count_for_path(&self, path: impl AsRef<Path>) -> usize {
        let path = path.as_ref().to_path_buf();
        self.state
            .lock()
            .path_write_counts
            .get(&path)
            .copied()
            .unwrap_or(0)
    }

    pub fn emit_fs_event(&self, path: impl Into<PathBuf>, event: Option<PathEventKind>) {
        self.state.lock().emit_event(std::iter::once((path, event)));
    }

    pub(super) fn simulate_random_delay(&self) -> impl futures::Future<Output = ()> {
        self.executor.simulate_random_delay()
    }

    /// Returns list of all tracked trash entries.
    pub fn trash_entries(&self) -> Vec<TrashedEntry> {
        self.state
            .lock()
            .trash
            .iter()
            .map(|(entry, _)| entry.clone())
            .collect()
    }

    pub(super) async fn remove_dir_inner(
        &self,
        path: &Path,
        options: RemoveOptions,
    ) -> Result<Option<FakeFsEntry>> {
        self.simulate_random_delay().await;

        let path = normalize_path(path);
        if let Some(message) = self.state.lock().remove_dir_errors.get(&path) {
            anyhow::bail!("{message}");
        }
        let parent_path = path.parent().context("cannot remove the root")?;
        let base_name = path.file_name().context("cannot remove the root")?;

        let mut state = self.state.lock();
        let parent_entry = state.entry(parent_path)?;
        let entry = parent_entry
            .dir_entries(parent_path)?
            .entry(base_name.to_str().unwrap().into());

        let removed = match entry {
            btree_map::Entry::Vacant(_) => {
                if !options.ignore_if_not_exists {
                    anyhow::bail!("{path:?} does not exist");
                }

                None
            }
            btree_map::Entry::Occupied(mut entry) => {
                {
                    let children = entry.get_mut().dir_entries(&path)?;
                    if !options.recursive && !children.is_empty() {
                        anyhow::bail!("{path:?} is not empty");
                    }
                }

                Some(entry.remove())
            }
        };

        state.emit_event([(path, Some(PathEventKind::Removed))]);
        Ok(removed)
    }

    pub(super) async fn remove_file_inner(
        &self,
        path: &Path,
        options: RemoveOptions,
    ) -> Result<Option<FakeFsEntry>> {
        self.simulate_random_delay().await;

        let path = normalize_path(path);
        let parent_path = path.parent().context("cannot remove the root")?;
        let base_name = path.file_name().unwrap();
        let mut state = self.state.lock();
        let parent_entry = state.entry(parent_path)?;
        let entry = parent_entry
            .dir_entries(parent_path)?
            .entry(base_name.to_str().unwrap().into());
        let removed = match entry {
            btree_map::Entry::Vacant(_) => {
                if !options.ignore_if_not_exists {
                    anyhow::bail!("{path:?} does not exist");
                }

                None
            }
            btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().file_content(&path)?;
                Some(entry.remove())
            }
        };

        state.emit_event([(path, Some(PathEventKind::Removed))]);
        Ok(removed)
    }
}
