use super::*;

impl LocalWorktree {
    pub(super) fn expand_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        let path = self.entry_for_id(entry_id)?.path.clone();
        let mut refresh = self.refresh_entries_for_paths(vec![path]);
        Some(cx.background_spawn(async move {
            refresh.next().await;
            Ok(())
        }))
    }

    pub(super) fn expand_all_for_entry(
        &self,
        entry_id: ProjectEntryId,
        cx: &Context<Worktree>,
    ) -> Option<Task<Result<()>>> {
        let path = self.entry_for_id(entry_id).unwrap().path.clone();
        let mut rx = self.add_path_prefix_to_scan(path);
        Some(cx.background_spawn(async move {
            rx.next().await;
            Ok(())
        }))
    }

    pub fn refresh_entries_for_paths(&self, paths: Vec<Arc<RelPath>>) -> barrier::Receiver {
        let (tx, rx) = barrier::channel();
        self.scan_requests_tx
            .try_send(ScanRequest {
                relative_paths: paths,
                done: smallvec![tx],
            })
            .ok();
        rx
    }

    #[cfg(feature = "test-support")]
    pub fn manually_refresh_entries_for_paths(
        &self,
        paths: Vec<Arc<RelPath>>,
    ) -> barrier::Receiver {
        self.refresh_entries_for_paths(paths)
    }

    pub fn add_path_prefix_to_scan(&self, path_prefix: Arc<RelPath>) -> barrier::Receiver {
        let (tx, rx) = barrier::channel();
        self.path_prefixes_to_scan_tx
            .try_send(PathPrefixScanRequest {
                path: path_prefix,
                done: smallvec![tx],
            })
            .ok();
        rx
    }

    pub fn refresh_entry(
        &self,
        path: Arc<RelPath>,
        old_path: Option<Arc<RelPath>>,
        cx: &Context<Worktree>,
    ) -> Task<Result<Option<Entry>>> {
        if self.settings.is_path_excluded(&path) {
            return Task::ready(Ok(None));
        }
        let paths = if let Some(old_path) = old_path.as_ref() {
            vec![old_path.clone(), path.clone()]
        } else {
            vec![path.clone()]
        };
        let t0 = Instant::now();
        let mut refresh = self.refresh_entries_for_paths(paths);
        // todo(lw): Hot foreground spawn
        cx.spawn(async move |this, cx| {
            refresh.recv().await;
            log::trace!("refreshed entry {path:?} in {:?}", t0.elapsed());
            let new_entry = this.read_with(cx, |this, _| {
                this.entry_for_path(&path).cloned().with_context(|| {
                    format!("Could not find entry in worktree for {path:?} after refresh")
                })
            })??;
            Ok(Some(new_entry))
        })
    }

    pub fn observe_updates<F, Fut>(&mut self, project_id: u64, cx: &Context<Worktree>, callback: F)
    where
        F: 'static + Send + Fn(proto::UpdateWorktree) -> Fut,
        Fut: 'static + Send + Future<Output = bool>,
    {
        if let Some(observer) = self.update_observer.as_mut() {
            *observer.resume_updates.borrow_mut() = ();
            return;
        }

        let (resume_updates_tx, mut resume_updates_rx) = watch::channel::<()>();
        let (snapshots_tx, mut snapshots_rx) =
            mpsc::unbounded::<(LocalSnapshot, UpdatedEntriesSet)>();
        snapshots_tx
            .unbounded_send((self.snapshot(), Arc::default()))
            .ok();

        let worktree_id = self.id.to_proto();
        let _maintain_remote_snapshot = cx.background_spawn(async move {
            let mut is_first = true;
            while let Some((snapshot, entry_changes)) = snapshots_rx.next().await {
                let update = if is_first {
                    is_first = false;
                    snapshot.build_initial_update(project_id, worktree_id)
                } else {
                    snapshot.build_update(project_id, worktree_id, entry_changes)
                };

                for update in proto::split_worktree_update(update) {
                    let _ = resume_updates_rx.try_recv();
                    loop {
                        let result = callback(update.clone());
                        if result.await {
                            break;
                        } else {
                            log::info!("waiting to resume updates");
                            if resume_updates_rx.next().await.is_none() {
                                return Some(());
                            }
                        }
                    }
                }
            }
            Some(())
        });

        self.update_observer = Some(UpdateObservationState {
            snapshots_tx,
            resume_updates: resume_updates_tx,
            _maintain_remote_snapshot,
        });
    }

    pub fn share_private_files(&mut self, cx: &Context<Worktree>) {
        self.share_private_files = true;
        self.restart_background_scanners(cx);
    }

    pub fn update_abs_path_and_refresh(
        &mut self,
        new_path: Arc<SanitizedPath>,
        cx: &Context<Worktree>,
    ) {
        self.snapshot.git_repositories = Default::default();
        self.snapshot.ignores_by_parent_abs_path = Default::default();
        let root_name = new_path
            .as_path()
            .file_name()
            .and_then(|f| f.to_str())
            .map_or(RelPath::empty_arc(), |f| RelPath::unix(f).unwrap().into());
        self.snapshot.update_abs_path(new_path, root_name);
        self.restart_background_scanners(cx);
    }
    #[cfg(feature = "test-support")]
    pub fn set_defer_watch(&mut self, defer: bool, cx: &mut Context<Worktree>) {
        self.force_defer_watch = defer;
        self.restart_background_scanners(cx);
    }

    #[cfg(feature = "test-support")]
    pub fn repositories(&self) -> Vec<Arc<Path>> {
        self.git_repositories
            .values()
            .map(|entry| entry.work_directory_abs_path.clone())
            .collect::<Vec<_>>()
    }
}
