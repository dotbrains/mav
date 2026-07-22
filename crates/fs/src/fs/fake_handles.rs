use super::*;

#[cfg(feature = "test-support")]
impl FakeFsEntry {
    pub(super) fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }

    pub(super) fn is_symlink(&self) -> bool {
        matches!(self, Self::Symlink { .. })
    }

    pub(super) fn file_content(&self, path: &Path) -> Result<&Vec<u8>> {
        if let Self::File { content, .. } = self {
            Ok(content)
        } else {
            anyhow::bail!("not a file: {path:?}");
        }
    }

    pub(super) fn dir_entries(
        &mut self,
        path: &Path,
    ) -> Result<&mut BTreeMap<String, FakeFsEntry>> {
        if let Self::Dir { entries, .. } = self {
            Ok(entries)
        } else {
            anyhow::bail!("not a directory: {path:?}");
        }
    }
}

#[cfg(feature = "test-support")]
pub(super) struct FakeWatcher {
    tx: async_channel::Sender<Vec<PathEvent>>,
    fs_state: Arc<Mutex<FakeFsState>>,
    prefixes: Mutex<Vec<PathBuf>>,
}

#[cfg(feature = "test-support")]
impl Watcher for FakeWatcher {
    pub(super) fn add(&self, path: &Path) -> Result<()> {
        let path = normalize_path(path);
        self.fs_state
            .try_lock()
            .unwrap()
            .create_file_before_watch_add(&path)?;

        let mut prefixes = self.prefixes.lock();
        if prefixes.iter().any(|prefix| path.starts_with(prefix)) {
            return Ok(());
        }

        self.fs_state
            .try_lock()
            .unwrap()
            .event_txs
            .push((path.clone(), self.tx.clone()));
        prefixes.push(path);
        Ok(())
    }

    pub(super) fn remove(&self, path: &Path) -> Result<()> {
        let path = normalize_path(path);
        self.prefixes.lock().retain(|prefix| prefix != &path);
        self.fs_state
            .try_lock()
            .unwrap()
            .event_txs
            .retain(|(watched_path, _)| watched_path != &path);
        Ok(())
    }
}

#[cfg(feature = "test-support")]
#[derive(Debug)]
pub(super) struct FakeHandle {
    inode: u64,
}

#[cfg(feature = "test-support")]
impl FileHandle for FakeHandle {
    pub(super) fn current_path(&self, fs: &Arc<dyn Fs>) -> Result<PathBuf> {
        let fs = fs.as_fake();
        let mut state = fs.state.lock();
        let Some(target) = state.moves.get(&self.inode).cloned() else {
            anyhow::bail!("fake fd not moved")
        };

        if state.try_entry(&target, false).is_some() {
            return Ok(target);
        }
        anyhow::bail!("fake fd target not found")
    }
}
