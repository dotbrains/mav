use super::*;

#[cfg_attr(feature = "test-support", allow(dead_code))]
impl RealFs {
    pub(super) async fn watch(
        &self,
        path: &Path,
        latency: Duration,
    ) -> (
        Pin<Box<dyn Send + Stream<Item = Vec<PathEvent>>>>,
        Arc<dyn Watcher>,
    ) {
        use util::{ResultExt as _, paths::SanitizedPath};
        let executor = self.executor.clone();

        let (tx, rx) = async_channel::unbounded();
        let pending_paths: Arc<Mutex<Vec<PathEvent>>> = Default::default();

        let watcher: Arc<dyn Watcher> = Arc::new(fs_watcher::FsWatcher::new(
            executor.clone(),
            tx.clone(),
            pending_paths.clone(),
        ));

        if let Err(e) = watcher.add(path) {
            log::warn!("Failed to watch {}:\n{e}", path.display());
        }

        // Check if path is a symlink and follow the target parent
        if let Some(mut target) = self.read_link(path).await.ok() {
            log::trace!("watch symlink {path:?} -> {target:?}");
            // Check if symlink target is relative path, if so make it absolute
            if target.is_relative()
                && let Some(parent) = path.parent()
            {
                target = parent.join(target);
                if let Ok(canonical) = self.canonicalize(&target).await {
                    target = SanitizedPath::new(&canonical).as_path().to_path_buf();
                }
            }
            watcher.add(&target).ok();
            if let Some(parent) = target.parent() {
                watcher.add(parent).log_err();
            }
        }

        (
            Box::pin(rx.filter_map({
                let watcher = watcher.clone();
                let executor = executor.clone();
                move |_| {
                    let _ = watcher.clone();
                    let pending_paths = pending_paths.clone();
                    let executor = executor.clone();
                    async move {
                        executor.timer(latency).await;
                        let paths = std::mem::take(&mut *pending_paths.lock());
                        log::debug!("pending path events: {:?}", paths);
                        (!paths.is_empty()).then_some(paths)
                    }
                }
            })),
            watcher,
        )
    }

    pub(super) fn open_repo(
        &self,
        dotgit_path: &Path,
        system_git_binary_path: Option<&Path>,
    ) -> Result<Arc<dyn GitRepository>> {
        Ok(Arc::new(RealGitRepository::new(
            dotgit_path,
            self.bundled_git_binary_path.clone(),
            system_git_binary_path.map(|path| path.to_path_buf()),
            self.executor.clone(),
        )?))
    }

    pub(super) async fn git_init(
        &self,
        abs_work_directory_path: &Path,
        fallback_branch_name: String,
    ) -> Result<()> {
        let result = new_command("git")
            .current_dir(abs_work_directory_path)
            .args(&["config", "--global", "--get", "init.defaultBranch"])
            .output()
            .await;

        // In case the `git config` command fails, which would be the case if
        // the user doesn't have an `init.defaultBranch` value set, we'll just
        // default to the provided `fallback_branch_name`.
        let branch_name = match result {
            Ok(output) if !output.stdout.is_empty() => String::from_utf8(output.stdout)?,
            _ => fallback_branch_name,
        };

        new_command("git")
            .current_dir(abs_work_directory_path)
            .args(&["init", "-b"])
            .arg(branch_name.trim())
            .output()
            .await?;

        Ok(())
    }

    pub(super) async fn git_clone(&self, abs_work_directory: &Path, repo_url: &str) -> Result<()> {
        let job_id = self.next_job_id.fetch_add(1, Ordering::SeqCst);
        let job_info = JobInfo {
            id: job_id,
            start: Instant::now(),
            message: SharedString::from(format!("Cloning {}", repo_url)),
        };

        let _job_tracker = JobTracker::new(job_info, self.job_event_subscribers.clone());

        let output = new_command("git")
            .current_dir(abs_work_directory)
            .args(&["clone", repo_url])
            .output()
            .await?;

        if !output.status.success() {
            anyhow::bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }

    /// Runs `git config` with the given arguments.
    /// Will return `Ok` if the commands exit status is `0`, with the stdout
    /// contents. Otherwise returns `Err` with the stderr contents.
    pub(super) async fn git_config(
        &self,
        abs_work_directory: &Path,
        args: Vec<String>,
    ) -> Result<String> {
        let output = new_command("git")
            .current_dir(abs_work_directory)
            .args([String::from("config")].into_iter().chain(args))
            .output()
            .await?;

        if !output.status.success() {
            let err = String::from_utf8(output.stderr)?;
            anyhow::bail!(err);
        }

        String::from_utf8(output.stdout).map_err(Into::into)
    }

    pub(super) fn is_fake(&self) -> bool {
        false
    }

    pub(super) fn subscribe_to_jobs(&self) -> JobEventReceiver {
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        self.job_event_subscribers.lock().push(sender);
        receiver
    }

    /// Checks whether the file system is case sensitive by attempting to create two files
    /// that have the same name except for the casing.
    ///
    /// It creates both files in a temporary directory it removes at the end.
    pub(super) async fn is_case_sensitive(&self) -> bool {
        const UNINITIALIMAV: u8 = 0;
        const CASE_SENSITIVE: u8 = 1;
        const NOT_CASE_SENSITIVE: u8 = 2;

        // Note we could CAS here, but really, if we race we do this work twice at worst which isn't a big deal.
        let load = self.is_case_sensitive.load(Ordering::Acquire);
        if load != UNINITIALIMAV {
            return load == CASE_SENSITIVE;
        }
        let temp_dir = self.executor.spawn(async { TempDir::new() });
        let res = maybe!(async {
            let temp_dir = temp_dir.await?;
            let test_file_1 = temp_dir.path().join("case_sensitivity_test.tmp");
            let test_file_2 = temp_dir.path().join("CASE_SENSITIVITY_TEST.TMP");

            let create_opts = CreateOptions {
                overwrite: false,
                ignore_if_exists: false,
            };

            // Create file1
            self.create_file(&test_file_1, create_opts).await?;

            // Now check whether it's possible to create file2
            let case_sensitive = match self.create_file(&test_file_2, create_opts).await {
                Ok(_) => Ok(true),
                Err(e) => {
                    if let Some(io_error) = e.downcast_ref::<io::Error>() {
                        if io_error.kind() == io::ErrorKind::AlreadyExists {
                            Ok(false)
                        } else {
                            Err(e)
                        }
                    } else {
                        Err(e)
                    }
                }
            };

            temp_dir.close()?;
            case_sensitive
        }).await.unwrap_or_else(|e| {
            log::error!(
                "Failed to determine whether filesystem is case sensitive (falling back to true) due to error: {e:#}"
            );
            true
        });
        self.is_case_sensitive.store(
            if res {
                CASE_SENSITIVE
            } else {
                NOT_CASE_SENSITIVE
            },
            Ordering::Release,
        );
        res
    }

    pub(super) async fn restore(
        &self,
        trashed_entry: TrashedEntry,
    ) -> std::result::Result<PathBuf, TrashRestoreError> {
        let restored_item_path = trashed_entry.original_parent.join(&trashed_entry.name);

        let (tx, rx) = futures::channel::oneshot::channel();
        std::thread::Builder::new()
            .name("restore trashed item".to_string())
            .spawn(move || {
                let res = trash::restore_all([trashed_entry.into_trash_item()]);
                tx.send(res)
            })
            .expect("The OS can spawn a threads");
        rx.await.expect("Restore all never panics")?;
        Ok(restored_item_path)
    }
}
