use super::*;

#[cfg(feature = "test-support")]
impl FakeFs {
    #[must_use]
    pub fn insert_tree<'a>(
        &'a self,
        path: impl 'a + AsRef<Path> + Send,
        tree: serde_json::Value,
    ) -> futures::future::BoxFuture<'a, ()> {
        use futures::FutureExt as _;
        use serde_json::Value::*;

        fn inner<'a>(
            this: &'a FakeFs,
            path: Arc<Path>,
            tree: serde_json::Value,
        ) -> futures::future::BoxFuture<'a, ()> {
            async move {
                match tree {
                    Object(map) => {
                        this.create_dir(&path).await.unwrap();
                        for (name, contents) in map {
                            let mut path = PathBuf::from(path.as_ref());
                            path.push(name);
                            this.insert_tree(&path, contents).await;
                        }
                    }
                    Null => {
                        this.create_dir(&path).await.unwrap();
                    }
                    String(contents) => {
                        this.insert_file(&path, contents.into_bytes()).await;
                    }
                    _ => {
                        panic!("JSON object must contain only objects, strings, or null");
                    }
                }
            }
            .boxed()
        }
        inner(self, Arc::from(path.as_ref()), tree)
    }

    pub fn insert_tree_from_real_fs<'a>(
        &'a self,
        path: impl 'a + AsRef<Path> + Send,
        src_path: impl 'a + AsRef<Path> + Send,
    ) -> futures::future::BoxFuture<'a, ()> {
        use futures::FutureExt as _;

        async move {
            let path = path.as_ref();
            if std::fs::metadata(&src_path).unwrap().is_file() {
                let contents = std::fs::read(src_path).unwrap();
                self.insert_file(path, contents).await;
            } else {
                self.create_dir(path).await.unwrap();
                for entry in std::fs::read_dir(&src_path).unwrap() {
                    let entry = entry.unwrap();
                    self.insert_tree_from_real_fs(path.join(entry.file_name()), entry.path())
                        .await;
                }
            }
        }
        .boxed()
    }

    pub fn with_git_state_and_paths<T, F>(
        &self,
        dot_git: &Path,
        emit_git_event: bool,
        f: F,
    ) -> Result<T>
    where
        F: FnOnce(&mut FakeGitRepositoryState, &Path, &Path) -> T,
    {
        let mut state = self.state.lock();
        let git_event_tx = state.git_event_tx.clone();
        let entry = state.entry(dot_git).context("open .git")?;

        if let FakeFsEntry::Dir { git_repo_state, .. } = entry {
            let repo_state = git_repo_state.get_or_insert_with(|| {
                log::debug!("insert git state for {dot_git:?}");
                Arc::new(Mutex::new(FakeGitRepositoryState::new(git_event_tx)))
            });
            let mut repo_state = repo_state.lock();

            let result = f(&mut repo_state, dot_git, dot_git);

            drop(repo_state);
            if emit_git_event {
                state.emit_event([(
                    dot_git.join("fake_git_repo_event"),
                    Some(PathEventKind::Changed),
                )]);
            }

            Ok(result)
        } else if let FakeFsEntry::File {
            content,
            git_dir_path,
            ..
        } = &mut *entry
        {
            let path = match git_dir_path {
                Some(path) => path,
                None => {
                    let path = std::str::from_utf8(content)
                        .ok()
                        .and_then(|content| content.strip_prefix("gitdir:"))
                        .context("not a valid gitfile")?
                        .trim();
                    git_dir_path.insert(normalize_path(&dot_git.parent().unwrap().join(path)))
                }
            }
            .clone();
            let Some((git_dir_entry, canonical_path)) = state.try_entry(&path, true) else {
                anyhow::bail!("pointed-to git dir {path:?} not found")
            };
            let FakeFsEntry::Dir {
                git_repo_state,
                entries,
                ..
            } = git_dir_entry
            else {
                anyhow::bail!("gitfile points to a non-directory")
            };
            let common_dir = if let Some(child) = entries.get("commondir") {
                let raw = std::str::from_utf8(child.file_content("commondir".as_ref())?)
                    .context("commondir content")?
                    .trim();
                let raw_path = Path::new(raw);
                if raw_path.is_relative() {
                    normalize_path(&canonical_path.join(raw_path))
                } else {
                    raw_path.to_owned()
                }
            } else {
                canonical_path.clone()
            };
            let repo_state = git_repo_state.get_or_insert_with(|| {
                Arc::new(Mutex::new(FakeGitRepositoryState::new(git_event_tx)))
            });
            let mut repo_state = repo_state.lock();

            let result = f(&mut repo_state, &canonical_path, &common_dir);

            if emit_git_event {
                drop(repo_state);
                state.emit_event([(
                    canonical_path.join("fake_git_repo_event"),
                    Some(PathEventKind::Changed),
                )]);
            }

            Ok(result)
        } else {
            anyhow::bail!("not a valid git repository");
        }
    }
}
