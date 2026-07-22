use super::real::rename_without_replace;
use super::*;

#[cfg_attr(feature = "test-support", allow(dead_code))]
impl RealFs {
    pub(super) async fn create_dir(&self, path: &Path) -> Result<()> {
        Ok(smol::fs::create_dir_all(path).await?)
    }

    pub(super) async fn create_symlink(&self, path: &Path, target: PathBuf) -> Result<()> {
        #[cfg(unix)]
        smol::fs::unix::symlink(target, path).await?;

        #[cfg(windows)]
        if smol::fs::metadata(&target).await?.is_dir() {
            let status = new_command("cmd")
                .args(["/C", "mklink", "/J"])
                .args([path, target.as_path()])
                .status()
                .await?;

            if !status.success() {
                return Err(anyhow::anyhow!(
                    "Failed to create junction from {:?} to {:?}",
                    path,
                    target
                ));
            }
        } else {
            smol::fs::windows::symlink_file(target, path).await?
        }

        Ok(())
    }

    pub(super) async fn create_file(&self, path: &Path, options: CreateOptions) -> Result<()> {
        let mut open_options = smol::fs::OpenOptions::new();
        open_options.write(true).create(true);
        if options.overwrite {
            open_options.truncate(true);
        } else if !options.ignore_if_exists {
            open_options.create_new(true);
        }
        open_options
            .open(path)
            .await
            .with_context(|| format!("Failed to create file at {:?}", path))?;
        Ok(())
    }

    pub(super) async fn create_file_with(
        &self,
        path: &Path,
        content: Pin<&mut (dyn AsyncRead + Send)>,
    ) -> Result<()> {
        let mut file = smol::fs::File::create(&path)
            .await
            .with_context(|| format!("Failed to create file at {:?}", path))?;
        futures::io::copy(content, &mut file).await?;
        Ok(())
    }

    pub(super) async fn extract_tar_file(
        &self,
        path: &Path,
        content: Archive<Pin<&mut (dyn AsyncRead + Send)>>,
    ) -> Result<()> {
        content.unpack(path).await?;
        Ok(())
    }

    pub(super) async fn copy_file(
        &self,
        source: &Path,
        target: &Path,
        options: CopyOptions,
    ) -> Result<()> {
        if !options.overwrite && smol::fs::metadata(target).await.is_ok() {
            if options.ignore_if_exists {
                return Ok(());
            } else {
                anyhow::bail!("{target:?} already exists");
            }
        }

        smol::fs::copy(source, target).await?;
        Ok(())
    }

    pub(super) async fn rename(
        &self,
        source: &Path,
        target: &Path,
        options: RenameOptions,
    ) -> Result<()> {
        if options.create_parents {
            if let Some(parent) = target.parent() {
                self.create_dir(parent).await?;
            }
        }

        if options.overwrite {
            smol::fs::rename(source, target).await?;
            return Ok(());
        }

        let use_metadata_fallback = {
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            {
                let source = source.to_path_buf();
                let target = target.to_path_buf();
                match self
                    .executor
                    .spawn(async move { rename_without_replace(&source, &target) })
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        if options.ignore_if_exists {
                            return Ok(());
                        }
                        return Err(error.into());
                    }
                    Err(error)
                        if error.raw_os_error().is_some_and(|code| {
                            code == libc::ENOSYS
                                || code == libc::ENOTSUP
                                || code == libc::EOPNOTSUPP
                                || code == libc::EINVAL
                        }) =>
                    {
                        // For case when filesystem or kernel does not support atomic no-overwrite rename.
                        // EINVAL is returned by FUSE-based filesystems (e.g. NTFS via ntfs-3g)
                        // that don't support RENAME_NOREPLACE.
                        true
                    }
                    Err(error) => return Err(error.into()),
                }
            }

            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                // For platforms which do not have an atomic no-overwrite rename yet.
                true
            }
        };

        if use_metadata_fallback && smol::fs::metadata(target).await.is_ok() {
            if options.ignore_if_exists {
                return Ok(());
            } else {
                anyhow::bail!("{target:?} already exists");
            }
        }

        smol::fs::rename(source, target).await?;
        Ok(())
    }

    pub(super) async fn remove_dir(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        let result = if options.recursive {
            smol::fs::remove_dir_all(path).await
        } else {
            smol::fs::remove_dir(path).await
        };
        match result {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound && options.ignore_if_not_exists => {
                Ok(())
            }
            Err(err) => Err(err)?,
        }
    }

    pub(super) async fn remove_file(&self, path: &Path, options: RemoveOptions) -> Result<()> {
        #[cfg(windows)]
        if let Ok(Some(metadata)) = self.metadata(path).await
            && metadata.is_symlink
            && metadata.is_dir
        {
            self.remove_dir(
                path,
                RemoveOptions {
                    recursive: false,
                    ignore_if_not_exists: true,
                },
            )
            .await?;
            return Ok(());
        }

        match smol::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound && options.ignore_if_not_exists => {
                Ok(())
            }
            Err(err) => Err(err)?,
        }
    }

    pub(super) async fn trash(&self, path: &Path, _options: RemoveOptions) -> Result<TrashedEntry> {
        // We must make the path absolute or trash will make a weird abomination
        // of the mav working directory (not usually the worktree) and whatever
        // the path variable holds.
        // We deliberately use `std::path::absolute` instead of `canonicalize`
        // to avoid resolving symlinks. Otherwise trashing a symlink would trash
        // its target and leave the link behind.
        let path = std::path::absolute(path).context("Could not make the path absolute")?;

        let (tx, rx) = futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name("trash file or dir".to_string())
            .spawn(|| tx.send(trash::delete_with_info(path)))
            .expect("The os can spawn threads");

        Ok(rx
            .await
            .context("Tx dropped or fs.restore panicked")?
            .context("Could not trash file or dir")?
            .into())
    }

    pub(super) async fn open_sync(&self, path: &Path) -> Result<Box<dyn io::Read + Send + Sync>> {
        Ok(Box::new(std::fs::File::open(path)?))
    }

    pub(super) async fn open_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS.0);
        }
        Ok(Arc::new(options.open(path)?))
    }

    pub(super) async fn load(&self, path: &Path) -> Result<String> {
        let path = path.to_path_buf();
        self.executor
            .spawn(async move {
                std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read file {}", path.display()))
            })
            .await
    }

    pub(super) async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        let path = path.to_path_buf();
        let bytes = self
            .executor
            .spawn(async move { std::fs::read(path) })
            .await?;
        Ok(bytes)
    }

    #[cfg(not(target_os = "windows"))]
    pub(super) async fn atomic_write(&self, path: PathBuf, data: String) -> Result<()> {
        smol::unblock(move || {
            // Use the directory of the destination as temp dir to avoid
            // invalid cross-device link error, and XDG_CACHE_DIR for fallback.
            // See https://github.com/mav-industries/mav/pull/8437 for more details.
            let mut tmp_file =
                tempfile::NamedTempFile::new_in(path.parent().unwrap_or(paths::temp_dir()))?;
            tmp_file.write_all(data.as_bytes())?;
            tmp_file.persist(path)?;
            anyhow::Ok(())
        })
        .await?;

        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub(super) async fn atomic_write(&self, path: PathBuf, data: String) -> Result<()> {
        smol::unblock(move || {
            // If temp dir is set to a different drive than the destination,
            // we receive error:
            //
            // failed to persist temporary file:
            // The system cannot move the file to a different disk drive. (os error 17)
            //
            // This is because `ReplaceFileW` does not support cross volume moves.
            // See the remark section: "The backup file, replaced file, and replacement file must all reside on the same volume."
            // https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew#remarks
            //
            // So we use the directory of the destination as a temp dir to avoid it.
            // https://github.com/mav-industries/mav/issues/16571
            let temp_dir = TempDir::new_in(path.parent().unwrap_or(paths::temp_dir()))?;
            let temp_file = {
                let temp_file_path = temp_dir.path().join("temp_file");
                let mut file = std::fs::File::create_new(&temp_file_path)?;
                file.write_all(data.as_bytes())?;
                temp_file_path
            };
            atomic_replace(path.as_path(), temp_file.as_path())?;
            anyhow::Ok(())
        })
        .await?;
        Ok(())
    }

    pub(super) async fn save(
        &self,
        path: &Path,
        text: &Rope,
        line_ending: LineEnding,
    ) -> Result<()> {
        let buffer_size = text.summary().len.min(10 * 1024);
        if let Some(path) = path.parent() {
            self.create_dir(path)
                .await
                .with_context(|| format!("Failed to create directory at {:?}", path))?;
        }
        let file = smol::fs::File::create(path)
            .await
            .with_context(|| format!("Failed to create file at {:?}", path))?;
        let mut writer = smol::io::BufWriter::with_capacity(buffer_size, file);
        for chunk in text::chunks_with_line_ending(text, line_ending) {
            writer.write_all(chunk.as_bytes()).await?;
        }
        writer.flush().await?;
        Ok(())
    }

    pub(super) async fn write(&self, path: &Path, content: &[u8]) -> Result<()> {
        if let Some(path) = path.parent() {
            self.create_dir(path)
                .await
                .with_context(|| format!("Failed to create directory at {:?}", path))?;
        }
        let path = path.to_owned();
        let contents = content.to_owned();
        self.executor
            .spawn(async move {
                std::fs::write(path, contents)?;
                Ok(())
            })
            .await
    }

    pub(super) async fn canonicalize(&self, path: &Path) -> Result<PathBuf> {
        let path = path.to_owned();
        self.executor
            .spawn(async move {
                #[cfg(target_os = "windows")]
                let result = Self::canonicalize(&path);

                #[cfg(not(target_os = "windows"))]
                let result = std::fs::canonicalize(&path);

                result.with_context(|| format!("canonicalizing {path:?}"))
            })
            .await
    }

    pub(super) async fn is_file(&self, path: &Path) -> bool {
        let path = path.to_owned();
        self.executor
            .spawn(async move { std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file()) })
            .await
    }

    pub(super) async fn is_dir(&self, path: &Path) -> bool {
        let path = path.to_owned();
        self.executor
            .spawn(async move { std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) })
            .await
    }

    pub(super) async fn metadata(&self, path: &Path) -> Result<Option<Metadata>> {
        let path_buf = path.to_owned();
        let symlink_metadata = match self
            .executor
            .spawn(async move { std::fs::symlink_metadata(&path_buf) })
            .await
        {
            Ok(metadata) => metadata,
            Err(err) => {
                return match err.kind() {
                    io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => Ok(None),
                    _ => Err(anyhow::Error::new(err)),
                };
            }
        };

        let is_symlink = symlink_metadata.file_type().is_symlink();
        let metadata = if is_symlink {
            let path_buf = path.to_path_buf();
            // Read target metadata, if the target exists
            match self
                .executor
                .spawn(async move { std::fs::metadata(path_buf) })
                .await
            {
                Ok(target_metadata) => target_metadata,
                Err(err) => {
                    if err.kind() != io::ErrorKind::NotFound {
                        // TODO: Also FilesystemLoop when that's stable
                        log::warn!(
                            "Failed to read symlink target metadata for path {path:?}: {err}"
                        );
                    }
                    // For a broken or recursive symlink, return the symlink metadata. (Or
                    // as edge cases, a symlink into a directory we can't read, which is hard
                    // to distinguish from just being broken.)
                    symlink_metadata
                }
            }
        } else {
            symlink_metadata
        };

        #[cfg(unix)]
        let inode = metadata.ino();

        #[cfg(windows)]
        let inode = file_id(path).await?;

        #[cfg(windows)]
        let is_fifo = false;

        #[cfg(unix)]
        let is_fifo = metadata.file_type().is_fifo();

        let path_buf = path.to_path_buf();
        let is_executable = self
            .executor
            .spawn(async move { path_buf.is_executable() })
            .await;

        Ok(Some(Metadata {
            inode,
            mtime: MTime(metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)),
            len: metadata.len(),
            is_symlink,
            is_dir: metadata.file_type().is_dir(),
            is_fifo,
            is_executable,
        }))
    }

    pub(super) async fn read_link(&self, path: &Path) -> Result<PathBuf> {
        let path = path.to_owned();
        let path = self
            .executor
            .spawn(async move { std::fs::read_link(&path) })
            .await?;
        Ok(path)
    }

    pub(super) async fn read_dir(
        &self,
        path: &Path,
    ) -> Result<Pin<Box<dyn Send + Stream<Item = Result<PathBuf>>>>> {
        let path = path.to_owned();
        let result = iter(
            self.executor
                .spawn(async move { std::fs::read_dir(path) })
                .await?,
        )
        .map(|entry| match entry {
            Ok(entry) => Ok(entry.path()),
            Err(error) => Err(anyhow!("failed to read dir entry {error:?}")),
        });
        Ok(Box::pin(result))
    }
}
