use super::*;

impl LocalWorktree {
    pub(super) fn load_binary_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<Result<LoadedBinaryFile>> {
        let path = Arc::from(path);
        let abs_path = self.absolutize(&path);
        let fs = self.fs.clone();
        let entry = self.refresh_entry(path.clone(), None, cx);
        let is_private = self.is_path_private(&path);

        let worktree = cx.weak_entity();
        cx.background_spawn(async move {
            let content = fs.load_bytes(&abs_path).await?;

            let worktree = worktree.upgrade().context("worktree was dropped")?;
            let file = match entry.await? {
                Some(entry) => File::for_entry(entry, worktree),
                None => {
                    let metadata = fs
                        .metadata(&abs_path)
                        .await
                        .with_context(|| {
                            format!("Loading metadata for excluded file {abs_path:?}")
                        })?
                        .with_context(|| {
                            format!("Excluded file {abs_path:?} got removed during loading")
                        })?;
                    Arc::new(File {
                        entry_id: None,
                        worktree,
                        path,
                        disk_state: DiskState::Present {
                            mtime: metadata.mtime,
                            size: metadata.len,
                        },
                        is_local: true,
                        is_private,
                    })
                }
            };

            Ok(LoadedBinaryFile { file, content })
        })
    }

    #[ztracing::instrument(skip_all)]
    pub(super) fn load_file(
        &self,
        path: &RelPath,
        cx: &Context<Worktree>,
    ) -> Task<Result<LoadedFile>> {
        let path = Arc::from(path);
        let abs_path = self.absolutize(&path);
        let fs = self.fs.clone();
        let entry = self.refresh_entry(path.clone(), None, cx);
        let is_private = self.is_path_private(path.as_ref());

        let this = cx.weak_entity();
        cx.background_spawn(async move {
            // WARN: Temporary workaround for #27283.
            //       We are not efficient with our memory usage per file, and use in excess of 64GB for a 10GB file
            //       Therefore, as a temporary workaround to prevent system freezes, we just bail before opening a file
            //       if it is too large
            //       5GB seems to be more reasonable, peaking at ~16GB, while 6GB jumps up to >24GB which seems like a
            //       reasonable limit
            {
                const FILE_SIZE_MAX: u64 = 6 * 1024 * 1024 * 1024; // 6GB
                if let Ok(Some(metadata)) = fs.metadata(&abs_path).await
                    && metadata.len >= FILE_SIZE_MAX
                {
                    anyhow::bail!("File is too large to load");
                }
            }
            let (text, encoding, has_bom) = decode_file_text(fs.as_ref(), &abs_path).await?;

            let worktree = this.upgrade().context("worktree was dropped")?;
            let file = match entry.await? {
                Some(entry) => File::for_entry(entry, worktree),
                None => {
                    let metadata = fs
                        .metadata(&abs_path)
                        .await
                        .with_context(|| {
                            format!("Loading metadata for excluded file {abs_path:?}")
                        })?
                        .with_context(|| {
                            format!("Excluded file {abs_path:?} got removed during loading")
                        })?;
                    Arc::new(File {
                        entry_id: None,
                        worktree,
                        path,
                        disk_state: DiskState::Present {
                            mtime: metadata.mtime,
                            size: metadata.len,
                        },
                        is_local: true,
                        is_private,
                    })
                }
            };

            Ok(LoadedFile {
                file,
                text,
                encoding,
                has_bom,
            })
        })
    }
}
