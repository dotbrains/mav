use super::*;

#[cfg_attr(feature = "test-support", allow(dead_code))]
impl FakeFs {
    pub(super) async fn create_dir(&self, path: &Path) -> Result<()> {
        self.simulate_random_delay().await;

        let mut created_dirs = Vec::new();
        let mut cur_path = PathBuf::new();
        for component in path.components() {
            let should_skip = matches!(component, Component::Prefix(..) | Component::RootDir);
            cur_path.push(component);
            if should_skip {
                continue;
            }
            let mut state = self.state.lock();

            let inode = state.get_and_increment_inode();
            let mtime = state.get_and_increment_mtime();
            state.write_path(&cur_path, |entry| {
                entry.or_insert_with(|| {
                    created_dirs.push((cur_path.clone(), Some(PathEventKind::Created)));
                    FakeFsEntry::Dir {
                        inode,
                        mtime,
                        len: 0,
                        entries: Default::default(),
                        git_repo_state: None,
                    }
                });
                Ok(())
            })?
        }

        self.state.lock().emit_event(created_dirs);
        Ok(())
    }

    pub(super) async fn create_file(&self, path: &Path, options: CreateOptions) -> Result<()> {
        self.simulate_random_delay().await;
        let mut state = self.state.lock();
        let inode = state.get_and_increment_inode();
        let mtime = state.get_and_increment_mtime();
        let file = FakeFsEntry::File {
            inode,
            mtime,
            len: 0,
            content: Vec::new(),
            git_dir_path: None,
        };
        let mut kind = Some(PathEventKind::Created);
        state.write_path(path, |entry| {
            match entry {
                btree_map::Entry::Occupied(mut e) => {
                    if options.overwrite {
                        kind = Some(PathEventKind::Changed);
                        *e.get_mut() = file;
                    } else if !options.ignore_if_exists {
                        anyhow::bail!("path already exists: {path:?}");
                    }
                }
                btree_map::Entry::Vacant(e) => {
                    e.insert(file);
                }
            }
            Ok(())
        })?;
        state.emit_event([(path, kind)]);
        Ok(())
    }

    pub(super) async fn create_symlink(&self, path: &Path, target: PathBuf) -> Result<()> {
        let mut state = self.state.lock();
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

        Ok(())
    }

    pub(super) async fn create_file_with(
        &self,
        path: &Path,
        mut content: Pin<&mut (dyn AsyncRead + Send)>,
    ) -> Result<()> {
        let mut bytes = Vec::new();
        content.read_to_end(&mut bytes).await?;
        self.write_file_internal(path, bytes, true)?;
        Ok(())
    }

    pub(super) async fn extract_tar_file(
        &self,
        path: &Path,
        content: Archive<Pin<&mut (dyn AsyncRead + Send)>>,
    ) -> Result<()> {
        let mut entries = content.entries()?;
        while let Some(entry) = entries.next().await {
            let mut entry = entry?;
            if entry.header().entry_type().is_file() {
                let path = path.join(entry.path()?.as_ref());
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).await?;
                self.create_dir(path.parent().unwrap()).await?;
                self.write_file_internal(&path, bytes, true)?;
            }
        }
        Ok(())
    }

    pub(super) async fn rename(
        &self,
        old_path: &Path,
        new_path: &Path,
        options: RenameOptions,
    ) -> Result<()> {
        self.simulate_random_delay().await;

        let old_path = normalize_path(old_path);
        let new_path = normalize_path(new_path);

        if options.create_parents {
            if let Some(parent) = new_path.parent() {
                self.create_dir(parent).await?;
            }
        }

        let mut state = self.state.lock();
        let moved_entry = state.write_path(&old_path, |e| {
            if let btree_map::Entry::Occupied(e) = e {
                Ok(e.get().clone())
            } else {
                anyhow::bail!("path does not exist: {old_path:?}")
            }
        })?;

        let inode = match moved_entry {
            FakeFsEntry::File { inode, .. } => inode,
            FakeFsEntry::Dir { inode, .. } => inode,
            _ => 0,
        };

        state.moves.insert(inode, new_path.clone());

        state.write_path(&new_path, |e| {
            match e {
                btree_map::Entry::Occupied(mut e) => {
                    if options.overwrite {
                        *e.get_mut() = moved_entry;
                    } else if !options.ignore_if_exists {
                        anyhow::bail!("path already exists: {new_path:?}");
                    }
                }
                btree_map::Entry::Vacant(e) => {
                    e.insert(moved_entry);
                }
            }
            Ok(())
        })?;

        state
            .write_path(&old_path, |e| {
                if let btree_map::Entry::Occupied(e) = e {
                    Ok(e.remove())
                } else {
                    unreachable!()
                }
            })
            .unwrap();

        state.emit_event([
            (old_path, Some(PathEventKind::Removed)),
            (new_path, Some(PathEventKind::Created)),
        ]);
        Ok(())
    }

    pub(super) async fn copy_file(
        &self,
        source: &Path,
        target: &Path,
        options: CopyOptions,
    ) -> Result<()> {
        self.simulate_random_delay().await;

        let source = normalize_path(source);
        let target = normalize_path(target);
        let mut state = self.state.lock();
        let mtime = state.get_and_increment_mtime();
        let inode = state.get_and_increment_inode();
        let source_entry = state.entry(&source)?;
        let content = source_entry.file_content(&source)?.clone();
        let mut kind = Some(PathEventKind::Created);
        state.write_path(&target, |e| match e {
            btree_map::Entry::Occupied(e) => {
                if options.overwrite {
                    kind = Some(PathEventKind::Changed);
                    Ok(Some(e.get().clone()))
                } else if !options.ignore_if_exists {
                    anyhow::bail!("{target:?} already exists");
                } else {
                    Ok(None)
                }
            }
            btree_map::Entry::Vacant(e) => Ok(Some(
                e.insert(FakeFsEntry::File {
                    inode,
                    mtime,
                    len: content.len() as u64,
                    content,
                    git_dir_path: None,
                })
                .clone(),
            )),
        })?;
        state.emit_event([(target, kind)]);
        Ok(())
    }

    pub(super) async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        self.remove_dir_inner(path, options).await.map(|_| ())
    }

    pub(super) async fn trash(&self, path: &Path, options: RemoveOptions) -> Result<TrashedEntry> {
        let normalized_path = normalize_path(path);
        let parent_path = normalized_path.parent().context("cannot remove the root")?;
        let base_name = normalized_path.file_name().unwrap();
        let result = if self.is_dir(path).await {
            self.remove_dir_inner(path, options).await?
        } else {
            self.remove_file_inner(path, options).await?
        };

        match result {
            Some(fake_entry) => {
                let trashed_entry = TrashedEntry {
                    id: base_name.to_str().unwrap().into(),
                    name: base_name.to_str().unwrap().into(),
                    original_parent: parent_path.to_path_buf(),
                };

                let mut state = self.state.lock();
                state.trash.push((trashed_entry.clone(), fake_entry));
                Ok(trashed_entry)
            }
            None => anyhow::bail!("{normalized_path:?} does not exist"),
        }
    }

    pub(super) async fn remove_file(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        self.remove_file_inner(path, options).await.map(|_| ())
    }

    pub(super) async fn open_sync(&self, path: &Path) -> Result<Box<dyn io::Read + Send + Sync>> {
        let bytes = self.load_internal(path).await?;
        Ok(Box::new(io::Cursor::new(bytes)))
    }

    pub(super) async fn open_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>> {
        self.simulate_random_delay().await;
        let mut state = self.state.lock();
        let inode = match state.entry(path)? {
            FakeFsEntry::File { inode, .. } => *inode,
            FakeFsEntry::Dir { inode, .. } => *inode,
            _ => unreachable!(),
        };
        Ok(Arc::new(FakeHandle { inode }))
    }

    pub(super) async fn load(&self, path: &Path) -> Result<String> {
        let content = self.load_internal(path).await?;
        Ok(String::from_utf8(content)?)
    }

    pub(super) async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        self.load_internal(path).await
    }

    pub(super) async fn atomic_write(&self, path: PathBuf, data: String) -> Result<()> {
        self.simulate_random_delay().await;
        let path = normalize_path(path.as_path());
        if let Some(path) = path.parent() {
            self.create_dir(path).await?;
        }
        self.write_file_internal(path, data.into_bytes(), true)?;
        Ok(())
    }

    pub(super) async fn save(
        &self,
        path: &Path,
        text: &Rope,
        line_ending: LineEnding,
    ) -> Result<()> {
        self.simulate_random_delay().await;
        let path = normalize_path(path);
        let content = text::chunks_with_line_ending(text, line_ending).collect::<String>();
        if let Some(path) = path.parent() {
            self.create_dir(path).await?;
        }
        self.write_file_internal(path, content.into_bytes(), false)?;
        Ok(())
    }

    pub(super) async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        self.simulate_random_delay().await;
        let path = normalize_path(path);
        if let Some(path) = path.parent() {
            self.create_dir(path).await?;
        }
        self.write_file_internal(path, content.to_vec(), false)?;
        Ok(())
    }

    pub(super) async fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        let path = normalize_path(path);
        self.simulate_random_delay().await;
        let state = self.state.lock();
        let canonical_path = state
            .canonicalize(&path, true)
            .with_context(|| format!("path does not exist: {path:?}"))?;
        Ok(canonical_path)
    }

    pub(super) async fn is_file(&self, path: &Path) -> bool {
        let path = normalize_path(path);
        self.simulate_random_delay().await;
        let mut state = self.state.lock();
        if let Some((entry, _)) = state.try_entry(&path, true) {
            entry.is_file()
        } else {
            false
        }
    }

    pub(super) async fn is_dir(&self, path: &Path) -> bool {
        self.metadata(path)
            .await
            .is_ok_and(|metadata| metadata.is_some_and(|metadata| metadata.is_dir))
    }

    pub(super) async fn metadata(&self, path: &Path) -> Result<Option<Metadata>> {
        self.simulate_random_delay().await;
        let path = normalize_path(path);
        let mut state = self.state.lock();
        state.metadata_call_count += 1;
        if let Some((mut entry, _)) = state.try_entry(&path, false) {
            let is_symlink = entry.is_symlink();
            if is_symlink {
                if let Some(e) = state.try_entry(&path, true).map(|e| e.0) {
                    entry = e;
                } else {
                    return Ok(None);
                }
            }

            Ok(Some(match &*entry {
                FakeFsEntry::File {
                    inode, mtime, len, ..
                } => Metadata {
                    inode: *inode,
                    mtime: *mtime,
                    len: *len,
                    is_dir: false,
                    is_symlink,
                    is_fifo: false,
                    is_executable: false,
                },
                FakeFsEntry::Dir {
                    inode, mtime, len, ..
                } => Metadata {
                    inode: *inode,
                    mtime: *mtime,
                    len: *len,
                    is_dir: true,
                    is_symlink,
                    is_fifo: false,
                    is_executable: false,
                },
                FakeFsEntry::Symlink { .. } => unreachable!(),
            }))
        } else {
            Ok(None)
        }
    }

    pub(super) async fn read_link(&self, path: &Path) -> Result<PathBuf> {
        self.simulate_random_delay().await;
        let path = normalize_path(path);
        let mut state = self.state.lock();
        let (entry, _) = state
            .try_entry(&path, false)
            .with_context(|| format!("path does not exist: {path:?}"))?;
        if let FakeFsEntry::Symlink { target } = entry {
            Ok(target.clone())
        } else {
            anyhow::bail!("not a symlink: {path:?}")
        }
    }

    pub(super) async fn read_dir(
        &self,
        path: &Path,
    ) -> Result<Pin<Box<dyn Send + Stream<Item = Result<PathBuf>>>>> {
        self.simulate_random_delay().await;
        let path = normalize_path(path);
        let mut state = self.state.lock();
        state.read_dir_call_count += 1;
        let entry = state.entry(&path)?;
        let children = entry.dir_entries(&path)?;
        let paths = children
            .keys()
            .map(|file_name| Ok(path.join(file_name)))
            .collect::<Vec<_>>();
        Ok(Box::pin(futures::stream::iter(paths)))
    }
}
