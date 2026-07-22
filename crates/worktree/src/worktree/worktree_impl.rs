use super::*;

impl Worktree {
    pub async fn local(
        path: impl Into<Arc<Path>>,
        visible: bool,
        fs: Arc<dyn Fs>,
        next_entry_id: Arc<AtomicUsize>,
        scanning_enabled: bool,
        worktree_id: WorktreeId,
        cx: &mut AsyncApp,
    ) -> Result<Entity<Self>> {
        let abs_path = path.into();
        let metadata = fs
            .metadata(&abs_path)
            .await
            .context("failed to stat worktree path")?;

        let fs_case_sensitive = fs.is_case_sensitive().await;

        let root_file_handle = if metadata.as_ref().is_some() {
            fs.open_handle(&abs_path)
                .await
                .with_context(|| {
                    format!(
                        "failed to open local worktree root at {}",
                        abs_path.display()
                    )
                })
                .log_err()
        } else {
            None
        };

        let root_repo_common_dir = if visible {
            discover_root_repo_common_dir(&abs_path, fs.as_ref())
                .await
                .map(SanitizedPath::from_arc)
        } else {
            None
        };
        Ok(cx.new(move |cx: &mut Context<Worktree>| {
            let mut snapshot = LocalSnapshot {
                ignores_by_parent_abs_path: Default::default(),
                global_gitignore: Default::default(),
                repo_exclude_by_work_dir_abs_path: Default::default(),
                git_repositories: Default::default(),
                external_canonical_to_relative: Default::default(),
                snapshot: Snapshot::new(
                    worktree_id,
                    abs_path
                        .file_name()
                        .and_then(|f| f.to_str())
                        .map_or(RelPath::empty_arc(), |f| RelPath::unix(f).unwrap().into()),
                    abs_path.clone(),
                    PathStyle::local(),
                ),
                root_file_handle,
            };
            snapshot.root_repo_common_dir = root_repo_common_dir;

            let worktree_id = snapshot.id();
            let settings_location = Some(SettingsLocation {
                worktree_id,
                path: RelPath::empty(),
            });

            let settings = WorktreeSettings::get(settings_location, cx).clone();
            cx.observe_global::<SettingsStore>(move |this, cx| {
                if let Self::Local(this) = this {
                    let settings = WorktreeSettings::get(settings_location, cx).clone();
                    if this.settings != settings {
                        this.settings = settings;
                        this.restart_background_scanners(cx);
                    }
                }
            })
            .detach();

            let share_private_files = false;
            if let Some(metadata) = metadata {
                let mut entry = Entry::new(
                    RelPath::empty_arc(),
                    &metadata,
                    ProjectEntryId::new(&next_entry_id),
                    snapshot.root_char_bag,
                    None,
                );
                if metadata.is_dir {
                    if !scanning_enabled {
                        entry.kind = EntryKind::UnloadedDir;
                    }
                } else {
                    if let Some(file_name) = abs_path.file_name()
                        && let Some(file_name) = file_name.to_str()
                        && let Ok(path) = RelPath::unix(file_name)
                    {
                        entry.is_private = !share_private_files && settings.is_path_private(path);
                        entry.is_hidden = settings.is_path_hidden(path);
                    }
                }
                cx.foreground_executor()
                    .block_on(snapshot.insert_entry(entry, fs.as_ref()));
            }

            let (scan_requests_tx, scan_requests_rx) = async_channel::unbounded();
            let (path_prefixes_to_scan_tx, path_prefixes_to_scan_rx) = async_channel::unbounded();
            let mut worktree = LocalWorktree {
                share_private_files,
                next_entry_id,
                snapshot,
                is_scanning: watch::channel_with(true),
                snapshot_subscriptions: Default::default(),
                update_observer: None,
                scan_requests_tx,
                path_prefixes_to_scan_tx,
                _background_scanner_tasks: Vec::new(),
                fs,
                fs_case_sensitive,
                visible,
                settings,
                scanning_enabled,
                force_defer_watch: false,
            };
            worktree.start_background_scanner(scan_requests_rx, path_prefixes_to_scan_rx, cx);
            Worktree::Local(worktree)
        }))
    }

    pub fn remote(
        project_id: u64,
        replica_id: ReplicaId,
        worktree: proto::WorktreeMetadata,
        client: AnyProtoClient,
        path_style: PathStyle,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let mut snapshot = Snapshot::new(
                WorktreeId::from_proto(worktree.id),
                RelPath::from_proto(&worktree.root_name).unwrap_or_else(|_| RelPath::empty_arc()),
                Path::new(&worktree.abs_path).into(),
                path_style,
            );

            snapshot.root_repo_common_dir = worktree
                .root_repo_common_dir
                .map(|p| SanitizedPath::new_arc(Path::new(&p)));

            let background_snapshot = Arc::new(Mutex::new((
                snapshot.clone(),
                Vec::<proto::UpdateWorktree>::new(),
            )));
            let (background_updates_tx, mut background_updates_rx) =
                mpsc::unbounded::<proto::UpdateWorktree>();
            let (mut snapshot_updated_tx, mut snapshot_updated_rx) = watch::channel();

            let worktree_id = snapshot.id();
            let settings_location = Some(SettingsLocation {
                worktree_id,
                path: RelPath::empty(),
            });

            let settings = WorktreeSettings::get(settings_location, cx).clone();
            let worktree = RemoteWorktree {
                client,
                project_id,
                replica_id,
                snapshot,
                file_scan_inclusions: settings.parent_dir_scan_inclusions.clone(),
                background_snapshot: background_snapshot.clone(),
                updates_tx: Some(background_updates_tx),
                update_observer: None,
                snapshot_subscriptions: Default::default(),
                visible: worktree.visible,
                disconnected: false,
                received_initial_update: false,
            };

            // Apply updates to a separate snapshot in a background task, then
            // send them to a foreground task which updates the model.
            cx.background_spawn(async move {
                while let Some(update) = background_updates_rx.next().await {
                    {
                        let mut lock = background_snapshot.lock();
                        lock.0.apply_remote_update(
                            update.clone(),
                            &settings.parent_dir_scan_inclusions,
                        );
                        lock.1.push(update);
                    }
                    snapshot_updated_tx.send(()).await.ok();
                }
            })
            .detach();

            // On the foreground task, update to the latest snapshot and notify
            // any update observer of all updates that led to that snapshot.
            cx.spawn(async move |this, cx| {
                while (snapshot_updated_rx.recv().await).is_some() {
                    this.update(cx, |this, cx| {
                        let this = this.as_remote_mut().unwrap();

                        // The watch channel delivers an initial signal before
                        // any real updates arrive. Skip these spurious wakeups.
                        if this.background_snapshot.lock().1.is_empty() {
                            return;
                        }

                        let old_root_repo_common_dir = this.snapshot.root_repo_common_dir.clone();
                        let mut changed_entries: Vec<(Arc<RelPath>, ProjectEntryId, PathChange)> =
                            Vec::new();
                        {
                            let mut lock = this.background_snapshot.lock();
                            // Replace the snapshot, keeping the previous one around so we can
                            // resolve the paths of removed entries (the new snapshot no longer
                            // contains them, and the wire format only carries their ids).
                            let old_snapshot = mem::replace(&mut this.snapshot, lock.0.clone());
                            for update in lock.1.drain(..) {
                                for entry_id in &update.removed_entries {
                                    let entry_id = ProjectEntryId::from_proto(*entry_id);
                                    if let Some(entry) = old_snapshot.entry_for_id(entry_id) {
                                        changed_entries.push((
                                            entry.path.clone(),
                                            entry_id,
                                            PathChange::Removed,
                                        ));
                                    }
                                }
                                for entry in &update.updated_entries {
                                    // Remote updates don't distinguish creation from
                                    // modification, so report `AddedOrUpdated`.
                                    if let Some(path) = RelPath::from_proto(&entry.path).log_err() {
                                        changed_entries.push((
                                            path,
                                            ProjectEntryId::from_proto(entry.id),
                                            PathChange::AddedOrUpdated,
                                        ));
                                    }
                                }
                                if let Some(tx) = &this.update_observer {
                                    tx.unbounded_send(update).ok();
                                }
                            }
                        };

                        if !changed_entries.is_empty() {
                            cx.emit(Event::UpdatedEntries(changed_entries.into()));
                        }
                        let is_first_update = !this.received_initial_update;
                        this.received_initial_update = true;
                        if this.snapshot.root_repo_common_dir != old_root_repo_common_dir
                            || (is_first_update && this.snapshot.root_repo_common_dir.is_none())
                        {
                            cx.emit(Event::UpdatedRootRepoCommonDir {
                                old: old_root_repo_common_dir,
                            });
                        }
                        cx.notify();
                        while let Some((scan_id, _)) = this.snapshot_subscriptions.front() {
                            if this.observed_snapshot(*scan_id) {
                                let (_, tx) = this.snapshot_subscriptions.pop_front().unwrap();
                                let _ = tx.send(());
                            } else {
                                break;
                            }
                        }
                    })?;
                }
                anyhow::Ok(())
            })
            .detach();

            Worktree::Remote(worktree)
        })
    }

    pub fn is_single_file(&self) -> bool {
        self.root_dir().is_none()
    }

    /// For visible worktrees, returns the path with the worktree name as the first component.
    /// Otherwise, returns an absolute path.
    pub fn full_path(&self, worktree_relative_path: &RelPath) -> PathBuf {
        if self.is_visible() {
            self.root_name()
                .join(worktree_relative_path)
                .display(self.path_style)
                .to_string()
                .into()
        } else {
            let full_path = self.abs_path();
            let mut full_path_string = if self.is_local()
                && let Ok(stripped) = full_path.strip_prefix(home_dir())
            {
                self.path_style
                    .join("~", &*stripped.to_string_lossy())
                    .unwrap()
            } else {
                full_path.to_string_lossy().into_owned()
            };

            if worktree_relative_path.components().next().is_some() {
                full_path_string.push_str(self.path_style.primary_separator());
                full_path_string.push_str(&worktree_relative_path.display(self.path_style));
            }

            full_path_string.into()
        }
    }
}
