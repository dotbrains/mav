use super::*;

impl FakeFs {
    /// We need to use something large enough for Windows and Unix to consider this a new file.
    /// https://doc.rust-lang.org/nightly/std/time/struct.SystemTime.html#platform-specific-behavior
    pub(super) const SYSTEMTIME_INTERVAL: Duration = Duration::from_nanos(100);

    pub fn new(executor: gpui::BackgroundExecutor) -> Arc<Self> {
        let (tx, rx) = async_channel::bounded::<PathBuf>(10);

        let this = Arc::new_cyclic(|this| Self {
            this: this.clone(),
            executor: executor.clone(),
            state: Arc::new(Mutex::new(FakeFsState {
                root: FakeFsEntry::Dir {
                    inode: 0,
                    mtime: MTime(UNIX_EPOCH),
                    len: 0,
                    entries: Default::default(),
                    git_repo_state: None,
                },
                git_event_tx: tx,
                next_mtime: UNIX_EPOCH + Self::SYSTEMTIME_INTERVAL,
                next_inode: 1,
                event_txs: Default::default(),
                buffered_events: Vec::new(),
                events_paused: false,
                read_dir_call_count: 0,
                metadata_call_count: 0,
                path_write_counts: Default::default(),
                moves: Default::default(),
                job_event_subscribers: Arc::new(Mutex::new(Vec::new())),
                trash: Vec::new(),
                file_to_create_before_watch_add: None,
                remove_dir_errors: Default::default(),
            })),
        });

        executor.spawn({
            let this = this.clone();
            async move {
                while let Ok(git_event) = rx.recv().await {
                    if let Some(mut state) = this.state.try_lock() {
                        state.emit_event([(git_event, Some(PathEventKind::Changed))]);
                    } else {
                        panic!("Failed to lock file system state, this execution would have caused a test hang");
                    }
                }
            }
        }).detach();

        this
    }

    pub fn set_next_mtime(&self, next_mtime: SystemTime) {
        let mut state = self.state.lock();
        state.next_mtime = next_mtime;
    }

    pub fn get_and_increment_mtime(&self) -> MTime {
        let mut state = self.state.lock();
        state.get_and_increment_mtime()
    }

    pub async fn touch_path(&self, path: impl AsRef<Path>) {
        let mut state = self.state.lock();
        let path = path.as_ref();
        let new_mtime = state.get_and_increment_mtime();
        let new_inode = state.get_and_increment_inode();
        state
            .write_path(path, move |entry| {
                match entry {
                    btree_map::Entry::Vacant(e) => {
                        e.insert(FakeFsEntry::File {
                            inode: new_inode,
                            mtime: new_mtime,
                            content: Vec::new(),
                            len: 0,
                            git_dir_path: None,
                        });
                    }
                    btree_map::Entry::Occupied(mut e) => match &mut *e.get_mut() {
                        FakeFsEntry::File { mtime, .. } => *mtime = new_mtime,
                        FakeFsEntry::Dir { mtime, .. } => *mtime = new_mtime,
                        FakeFsEntry::Symlink { .. } => {}
                    },
                }
                Ok(())
            })
            .unwrap();
        state.emit_event([(path.to_path_buf(), Some(PathEventKind::Changed))]);
    }

    pub async fn insert_file(&self, path: impl AsRef<Path>, content: Vec<u8>) {
        self.write_file_internal(path, content, true).unwrap()
    }

    pub async fn insert_symlink(&self, path: impl AsRef<Path>, target: PathBuf) {
        let mut state = self.state.lock();
        let path = path.as_ref();
        let file = FakeFsEntry::Symlink { target };
        state
            .write_path(path.as_ref(), move |e| match e {
                btree_map::Entry::Vacant(e) => {
                    e.insert(file);
                    Ok(())
                }
                btree_map::Entry::Occupied(mut e) => {
                    *e.get_mut() = file;
                    Ok(())
                }
            })
            .unwrap();
        state.emit_event([(path, Some(PathEventKind::Created))]);
    }

    pub(super) fn write_file_internal(
        &self,
        path: impl AsRef<Path>,
        new_content: Vec<u8>,
        recreate_inode: bool,
    ) -> Result<()> {
        fn inner(
            this: &FakeFs,
            path: &Path,
            new_content: Vec<u8>,
            recreate_inode: bool,
        ) -> Result<()> {
            let mut state = this.state.lock();
            let path_buf = path.to_path_buf();
            *state.path_write_counts.entry(path_buf).or_insert(0) += 1;
            let new_inode = state.get_and_increment_inode();
            let new_mtime = state.get_and_increment_mtime();
            let new_len = new_content.len() as u64;
            let mut kind = None;
            state.write_path(path, |entry| {
                match entry {
                    btree_map::Entry::Vacant(e) => {
                        kind = Some(PathEventKind::Created);
                        e.insert(FakeFsEntry::File {
                            inode: new_inode,
                            mtime: new_mtime,
                            len: new_len,
                            content: new_content,
                            git_dir_path: None,
                        });
                    }
                    btree_map::Entry::Occupied(mut e) => {
                        kind = Some(PathEventKind::Changed);
                        if let FakeFsEntry::File {
                            inode,
                            mtime,
                            len,
                            content,
                            ..
                        } = e.get_mut()
                        {
                            *mtime = new_mtime;
                            *content = new_content;
                            *len = new_len;
                            if recreate_inode {
                                *inode = new_inode;
                            }
                        } else {
                            anyhow::bail!("not a file")
                        }
                    }
                }
                Ok(())
            })?;
            state.emit_event([(path, kind)]);
            Ok(())
        }
        inner(self, path.as_ref(), new_content, recreate_inode)
    }

    pub fn read_file_sync(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        let path = normalize_path(path);
        let mut state = self.state.lock();
        let entry = state.entry(&path)?;
        entry.file_content(&path).cloned()
    }

    pub(super) async fn load_internal(&self, path: impl AsRef<Path>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        let path = normalize_path(path);
        self.simulate_random_delay().await;
        let mut state = self.state.lock();
        let entry = state.entry(&path)?;
        entry.file_content(&path).cloned()
    }

    pub fn pause_events(&self) {
        self.state.lock().events_paused = true;
    }

    pub fn unpause_events_and_flush(&self) {
        self.state.lock().events_paused = false;
        self.flush_events(usize::MAX);
    }

    pub fn buffered_event_count(&self) -> usize {
        self.state.lock().buffered_events.len()
    }

    pub fn clear_buffered_events(&self) {
        self.state.lock().buffered_events.clear();
    }

    pub fn create_file_before_next_watch_add(
        &self,
        watch_path: impl AsRef<Path>,
        path: impl AsRef<Path>,
    ) {
        self.state.lock().file_to_create_before_watch_add = Some((
            normalize_path(watch_path.as_ref()),
            normalize_path(path.as_ref()),
        ));
    }

    pub fn flush_events(&self, count: usize) {
        self.state.lock().flush_events(count);
    }

    pub(crate) fn entry(&self, target: &Path) -> Result<FakeFsEntry> {
        self.state.lock().entry(target).cloned()
    }

    pub(crate) fn insert_entry(&self, target: &Path, new_entry: FakeFsEntry) -> Result<()> {
        let mut state = self.state.lock();
        state.write_path(target, |entry| {
            match entry {
                btree_map::Entry::Vacant(vacant_entry) => {
                    vacant_entry.insert(new_entry);
                }
                btree_map::Entry::Occupied(mut occupied_entry) => {
                    occupied_entry.insert(new_entry);
                }
            }
            Ok(())
        })
    }
}
