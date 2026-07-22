use super::*;

#[cfg_attr(feature = "test-support", allow(dead_code))]
impl FakeFs {
    pub(super) async fn watch(
        &self,
        path: &Path,
        _: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        self.simulate_random_delay().await;
        let (tx, rx) = async_channel::unbounded();
        let path = path.to_path_buf();
        self.state.lock().event_txs.push((path.clone(), tx.clone()));
        let executor = self.executor.clone();
        let watcher = Arc::new(FakeWatcher {
            tx,
            fs_state: self.state.clone(),
            prefixes: Mutex::new(vec![path]),
        });
        (
            Box::pin(futures::StreamExt::filter(rx, {
                let watcher = watcher.clone();
                move |events| {
                    let result = events.iter().any(|evt_path| {
                        watcher
                            .prefixes
                            .lock()
                            .iter()
                            .any(|prefix| evt_path.path.starts_with(prefix))
                    });
                    let executor = executor.clone();
                    async move {
                        executor.simulate_random_delay().await;
                        result
                    }
                }
            })),
            watcher,
        )
    }

    pub(super) fn open_repo(
        &self,
        abs_dot_git: &Path,
        _system_git_binary: Option<&Path>,
    ) -> Result<Arc<dyn GitRepository>> {
        self.with_git_state_and_paths(
            abs_dot_git,
            false,
            |_, repository_dir_path, common_dir_path| {
                Arc::new(fake_git_repo::FakeGitRepository {
                    fs: self.this.upgrade().unwrap(),
                    executor: self.executor.clone(),
                    dot_git_path: abs_dot_git.to_path_buf(),
                    repository_dir_path: repository_dir_path.to_owned(),
                    common_dir_path: common_dir_path.to_owned(),
                    checkpoints: Arc::default(),
                    is_trusted: Arc::default(),
                }) as _
            },
        )
    }

    pub(super) async fn git_init(
        &self,
        abs_work_directory_path: &Path,
        _fallback_branch_name: String,
    ) -> Result<()> {
        self.create_dir(&abs_work_directory_path.join(".git")).await
    }

    pub(super) async fn git_clone(
        &self,
        _abs_work_directory: &Path,
        _repo_url: &str,
    ) -> Result<()> {
        anyhow::bail!("Git clone is not supported in fake Fs")
    }

    pub(super) async fn git_config(
        &self,
        _abs_work_directory: &Path,
        _args: Vec<String>,
    ) -> Result<String> {
        anyhow::bail!("Git config is not supported in fake Fs")
    }

    pub(super) fn is_fake(&self) -> bool {
        true
    }

    pub(super) async fn is_case_sensitive(&self) -> bool {
        true
    }

    pub(super) fn subscribe_to_jobs(&self) -> JobEventReceiver {
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        self.state.lock().job_event_subscribers.lock().push(sender);
        receiver
    }

    pub(super) async fn restore(
        &self,
        trashed_entry: TrashedEntry,
    ) -> Result<PathBuf, TrashRestoreError> {
        let mut state = self.state.lock();

        let Some((trashed_entry, fake_entry)) = state
            .trash
            .iter()
            .find(|(entry, _)| *entry == trashed_entry)
            .cloned()
        else {
            return Err(TrashRestoreError::NotFound {
                path: PathBuf::from(trashed_entry.id),
            });
        };

        let path = trashed_entry
            .original_parent
            .join(trashed_entry.name.clone());

        let result = state.write_path(&path, |entry| match entry {
            btree_map::Entry::Vacant(entry) => {
                entry.insert(fake_entry);
                Ok(())
            }
            btree_map::Entry::Occupied(_) => {
                anyhow::bail!("Failed to restore {:?}", path);
            }
        });

        match result {
            Ok(_) => {
                state.trash.retain(|(entry, _)| *entry != trashed_entry);
                state.emit_event([(path.clone(), Some(PathEventKind::Created))]);
                Ok(path)
            }
            Err(_) => {
                // For now we'll just assume that this failed because it was a
                // collision error, which I think that, for the time being, is
                // the only case where this could fail?
                Err(TrashRestoreError::Collision { path })
            }
        }
    }

    #[cfg(feature = "test-support")]
    pub(super) fn as_fake(&self) -> Arc<FakeFs> {
        self.this.upgrade().unwrap()
    }
}
