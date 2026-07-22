use super::*;

pub struct FakeFs {
    pub(super) this: std::sync::Weak<Self>,
    // Use an unfair lock to ensure tests are deterministic.
    pub(super) state: Arc<Mutex<FakeFsState>>,
    pub(super) executor: gpui::BackgroundExecutor,
}

#[cfg(feature = "test-support")]
pub(super) struct FakeFsState {
    pub(super) root: FakeFsEntry,
    pub(super) next_inode: u64,
    pub(super) next_mtime: SystemTime,
    pub(super) git_event_tx: async_channel::Sender<PathBuf>,
    pub(super) event_txs: Vec<(PathBuf, async_channel::Sender<Vec<PathEvent>>)>,
    pub(super) events_paused: bool,
    pub(super) buffered_events: Vec<PathEvent>,
    pub(super) metadata_call_count: usize,
    pub(super) read_dir_call_count: usize,
    pub(super) path_write_counts: std::collections::HashMap<PathBuf, usize>,
    pub(super) moves: std::collections::HashMap<u64, PathBuf>,
    pub(super) job_event_subscribers: Arc<Mutex<Vec<JobEventSender>>>,
    pub(super) trash: Vec<(TrashedEntry, FakeFsEntry)>,
    pub(super) file_to_create_before_watch_add: Option<(PathBuf, PathBuf)>,
    pub(super) remove_dir_errors: std::collections::HashMap<PathBuf, String>,
}

#[cfg(feature = "test-support")]
impl FakeFsState {
    pub(super) fn create_file_before_watch_add(&mut self, watch_path: &Path) -> Result<()> {
        let Some((pending_watch_path, file_path)) = self.file_to_create_before_watch_add.take()
        else {
            return Ok(());
        };
        if pending_watch_path != watch_path {
            self.file_to_create_before_watch_add = Some((pending_watch_path, file_path));
            return Ok(());
        }

        let inode = self.get_and_increment_inode();
        let mtime = self.get_and_increment_mtime();
        self.write_path(&file_path, |entry| {
            let btree_map::Entry::Vacant(entry) = entry else {
                anyhow::bail!("file already exists: {}", file_path.display());
            };
            entry.insert(FakeFsEntry::File {
                inode,
                mtime,
                len: 0,
                content: Vec::new(),
                git_dir_path: None,
            });
            Ok(())
        })?;
        self.emit_event([(file_path, Some(PathEventKind::Created))]);
        Ok(())
    }
}

#[cfg(feature = "test-support")]
#[derive(Clone, Debug)]
pub(super) enum FakeFsEntry {
    File {
        inode: u64,
        mtime: MTime,
        len: u64,
        content: Vec<u8>,
        // The path to the repository state directory, if this is a gitfile.
        git_dir_path: Option<PathBuf>,
    },
    Dir {
        inode: u64,
        mtime: MTime,
        len: u64,
        entries: BTreeMap<String, FakeFsEntry>,
        git_repo_state: Option<Arc<Mutex<FakeGitRepositoryState>>>,
    },
    Symlink {
        target: PathBuf,
    },
}

#[cfg(feature = "test-support")]
impl PartialEq for FakeFsEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::File {
                    inode: l_inode,
                    mtime: l_mtime,
                    len: l_len,
                    content: l_content,
                    git_dir_path: l_git_dir_path,
                },
                Self::File {
                    inode: r_inode,
                    mtime: r_mtime,
                    len: r_len,
                    content: r_content,
                    git_dir_path: r_git_dir_path,
                },
            ) => {
                l_inode == r_inode
                    && l_mtime == r_mtime
                    && l_len == r_len
                    && l_content == r_content
                    && l_git_dir_path == r_git_dir_path
            }
            (
                Self::Dir {
                    inode: l_inode,
                    mtime: l_mtime,
                    len: l_len,
                    entries: l_entries,
                    git_repo_state: l_git_repo_state,
                },
                Self::Dir {
                    inode: r_inode,
                    mtime: r_mtime,
                    len: r_len,
                    entries: r_entries,
                    git_repo_state: r_git_repo_state,
                },
            ) => {
                let same_repo_state = match (l_git_repo_state.as_ref(), r_git_repo_state.as_ref()) {
                    (Some(l), Some(r)) => Arc::ptr_eq(l, r),
                    (None, None) => true,
                    _ => false,
                };
                l_inode == r_inode
                    && l_mtime == r_mtime
                    && l_len == r_len
                    && l_entries == r_entries
                    && same_repo_state
            }
            (Self::Symlink { target: l_target }, Self::Symlink { target: r_target }) => {
                l_target == r_target
            }
            _ => false,
        }
    }
}

#[cfg(feature = "test-support")]
impl FakeFsState {
    pub(super) fn get_and_increment_mtime(&mut self) -> MTime {
        let mtime = self.next_mtime;
        self.next_mtime += FakeFs::SYSTEMTIME_INTERVAL;
        MTime(mtime)
    }

    pub(super) fn get_and_increment_inode(&mut self) -> u64 {
        let inode = self.next_inode;
        self.next_inode += 1;
        inode
    }

    pub(super) fn canonicalize(&self, target: &Path, follow_symlink: bool) -> Option<PathBuf> {
        let mut canonical_path = PathBuf::new();
        let mut path = target.to_path_buf();
        let mut entry_stack = Vec::new();
        'outer: loop {
            let mut path_components = path.components().peekable();
            let mut prefix = None;
            while let Some(component) = path_components.next() {
                match component {
                    Component::Prefix(prefix_component) => prefix = Some(prefix_component),
                    Component::RootDir => {
                        entry_stack.clear();
                        entry_stack.push(&self.root);
                        canonical_path.clear();
                        match prefix {
                            Some(prefix_component) => {
                                canonical_path = PathBuf::from(prefix_component.as_os_str());
                                // Prefixes like `C:\\` are represented without their trailing slash, so we have to re-add it.
                                canonical_path.push(std::path::MAIN_SEPARATOR_STR);
                            }
                            None => canonical_path = PathBuf::from(std::path::MAIN_SEPARATOR_STR),
                        }
                    }
                    Component::CurDir => {}
                    Component::ParentDir => {
                        entry_stack.pop()?;
                        canonical_path.pop();
                    }
                    Component::Normal(name) => {
                        let current_entry = *entry_stack.last()?;
                        if let FakeFsEntry::Dir { entries, .. } = current_entry {
                            let entry = entries.get(name.to_str().unwrap())?;
                            if (path_components.peek().is_some() || follow_symlink)
                                && let FakeFsEntry::Symlink { target, .. } = entry
                            {
                                let mut target = target.clone();
                                target.extend(path_components);
                                path = target;
                                continue 'outer;
                            }
                            entry_stack.push(entry);
                            canonical_path = canonical_path.join(name);
                        } else {
                            return None;
                        }
                    }
                }
            }
            break;
        }

        if entry_stack.is_empty() {
            None
        } else {
            Some(canonical_path)
        }
    }

    pub(super) fn try_entry(
        &mut self,
        target: &Path,
        follow_symlink: bool,
    ) -> Option<(&mut FakeFsEntry, PathBuf)> {
        let canonical_path = self.canonicalize(target, follow_symlink)?;

        let mut components = canonical_path
            .components()
            .skip_while(|component| matches!(component, Component::Prefix(_)));
        let Some(Component::RootDir) = components.next() else {
            panic!(
                "the path {:?} was not canonicalized properly {:?}",
                target, canonical_path
            )
        };

        let mut entry = &mut self.root;
        for component in components {
            match component {
                Component::Normal(name) => {
                    if let FakeFsEntry::Dir { entries, .. } = entry {
                        entry = entries.get_mut(name.to_str().unwrap())?;
                    } else {
                        return None;
                    }
                }
                _ => {
                    panic!(
                        "the path {:?} was not canonicalized properly {:?}",
                        target, canonical_path
                    )
                }
            }
        }

        Some((entry, canonical_path))
    }

    pub(super) fn entry(&mut self, target: &Path) -> Result<&mut FakeFsEntry> {
        Ok(self
            .try_entry(target, true)
            .ok_or_else(|| {
                anyhow!(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("not found: {target:?}")
                ))
            })?
            .0)
    }

    pub(super) fn write_path<Fn, T>(&mut self, path: &Path, callback: Fn) -> Result<T>
    where
        Fn: FnOnce(btree_map::Entry<String, FakeFsEntry>) -> Result<T>,
    {
        let path = normalize_path(path);
        let filename = path.file_name().context("cannot overwrite the root")?;
        let parent_path = path.parent().unwrap();

        let parent = self.entry(parent_path)?;
        let new_entry = parent
            .dir_entries(parent_path)?
            .entry(filename.to_str().unwrap().into());
        callback(new_entry)
    }

    pub(super) fn emit_event<I, T>(&mut self, paths: I)
    where
        I: IntoIterator<Item = (T, Option<PathEventKind>)>,
        T: Into<PathBuf>,
    {
        self.buffered_events
            .extend(paths.into_iter().map(|(path, kind)| PathEvent {
                path: path.into(),
                kind,
            }));

        if !self.events_paused {
            self.flush_events(self.buffered_events.len());
        }
    }

    pub(super) fn flush_events(&mut self, mut count: usize) {
        count = count.min(self.buffered_events.len());
        let events = self.buffered_events.drain(0..count).collect::<Vec<_>>();
        self.event_txs.retain(|(_, tx)| {
            let _ = tx.try_send(events.clone());
            !tx.is_closed()
        });
    }
}

#[cfg(feature = "test-support")]
pub static FS_DOT_GIT: std::sync::LazyLock<&'static OsStr> =
    std::sync::LazyLock::new(|| OsStr::new(".git"));
