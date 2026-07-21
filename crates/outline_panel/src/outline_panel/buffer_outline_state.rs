use super::*;

impl OutlinePanel {
    pub(super) fn fetch_outdated_outlines(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let buffers_to_fetch = self.buffers_to_fetch();
        if buffers_to_fetch.is_empty() {
            return;
        }

        let first_update = Arc::new(AtomicBool::new(true));
        for buffer_id in buffers_to_fetch {
            let outline_task = self.active_editor().map(|editor| {
                editor.update(cx, |editor, cx| editor.buffer_outline_items(buffer_id, cx))
            });

            let first_update = first_update.clone();

            self.outline_fetch_tasks.insert(
                buffer_id,
                cx.spawn_in(window, async move |outline_panel, cx| {
                    let Some(outline_task) = outline_task else {
                        return;
                    };
                    let fetched_outlines = outline_task.await;
                    let outlines_with_children = fetched_outlines
                        .windows(2)
                        .filter_map(|window| {
                            let current = &window[0];
                            let next = &window[1];
                            if next.depth > current.depth {
                                Some((current.range.clone(), current.depth))
                            } else {
                                None
                            }
                        })
                        .collect::<HashSet<_>>();

                    outline_panel
                        .update_in(cx, |outline_panel, window, cx| {
                            let pending_default_depth =
                                outline_panel.pending_default_expansion_depth.take();

                            let debounce =
                                if first_update.fetch_and(false, atomic::Ordering::AcqRel) {
                                    None
                                } else {
                                    Some(UPDATE_DEBOUNCE)
                                };

                            if let Some(buffer) = outline_panel.buffers.get_mut(&buffer_id) {
                                buffer.outlines = OutlineState::Outlines(fetched_outlines.clone());

                                if let Some(default_depth) = pending_default_depth
                                    && let OutlineState::Outlines(outlines) = &buffer.outlines
                                {
                                    outlines
                                        .iter()
                                        .filter(|outline| {
                                            (default_depth == 0 || outline.depth >= default_depth)
                                                && outlines_with_children.contains(&(
                                                    outline.range.clone(),
                                                    outline.depth,
                                                ))
                                        })
                                        .for_each(|outline| {
                                            outline_panel.collapsed_entries.insert(
                                                CollapsedEntry::Outline(outline.range.clone()),
                                            );
                                        });
                                }
                            }

                            outline_panel.update_cached_entries(debounce, window, cx);
                        })
                        .ok();
                }),
            );
        }
    }

    pub(super) fn is_singleton_active(&self, cx: &App) -> bool {
        self.active_editor()
            .is_some_and(|active_editor| active_editor.read(cx).buffer().read(cx).is_singleton())
    }

    pub(super) fn invalidate_outlines(&mut self, ids: &[BufferId]) {
        self.outline_fetch_tasks.clear();
        let mut ids = ids.iter().collect::<HashSet<_>>();
        for (buffer_id, buffer) in self.buffers.iter_mut() {
            if ids.remove(&buffer_id) {
                buffer.invalidate_outlines();
            }
            if ids.is_empty() {
                break;
            }
        }
    }

    pub(super) fn buffers_to_fetch(&self) -> HashSet<BufferId> {
        self.fs_entries
            .iter()
            .fold(HashSet::default(), |mut buffers_to_fetch, fs_entry| {
                match fs_entry {
                    FsEntry::File(FsEntryFile { buffer_id, .. })
                    | FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }) => {
                        if let Some(buffer) = self.buffers.get(buffer_id)
                            && buffer.should_fetch_outlines()
                        {
                            buffers_to_fetch.insert(*buffer_id);
                        }
                    }
                    FsEntry::Directory(..) => {}
                }
                buffers_to_fetch
            })
    }

    pub(super) fn buffer_snapshot_for_id(
        &self,
        buffer_id: BufferId,
        cx: &App,
    ) -> Option<BufferSnapshot> {
        let editor = self.active_editor()?;
        Some(
            editor
                .read(cx)
                .buffer()
                .read(cx)
                .buffer(buffer_id)?
                .read(cx)
                .snapshot(),
        )
    }

    pub(super) fn abs_path(&self, entry: &PanelEntry, cx: &App) -> Option<PathBuf> {
        match entry {
            PanelEntry::Fs(
                FsEntry::File(FsEntryFile { buffer_id, .. })
                | FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }),
            ) => self
                .buffer_snapshot_for_id(*buffer_id, cx)
                .and_then(|buffer_snapshot| {
                    let file = File::from_dyn(buffer_snapshot.file())?;
                    Some(file.worktree.read(cx).absolutize(&file.path))
                }),
            PanelEntry::Fs(FsEntry::Directory(FsEntryDirectory {
                worktree_id, entry, ..
            })) => Some(
                self.project
                    .read(cx)
                    .worktree_for_id(*worktree_id, cx)?
                    .read(cx)
                    .absolutize(&entry.path),
            ),
            PanelEntry::FoldedDirs(FoldedDirsEntry {
                worktree_id,
                entries: dirs,
                ..
            }) => dirs.last().and_then(|entry| {
                self.project
                    .read(cx)
                    .worktree_for_id(*worktree_id, cx)
                    .map(|worktree| worktree.read(cx).absolutize(&entry.path))
            }),
            PanelEntry::Search(_) | PanelEntry::Outline(..) => None,
        }
    }

    pub(super) fn relative_path(&self, entry: &FsEntry, cx: &App) -> Option<Arc<RelPath>> {
        match entry {
            FsEntry::ExternalFile(FsEntryExternalFile { buffer_id, .. }) => {
                let buffer_snapshot = self.buffer_snapshot_for_id(*buffer_id, cx)?;
                Some(buffer_snapshot.file()?.path().clone())
            }
            FsEntry::Directory(FsEntryDirectory { entry, .. }) => Some(entry.path.clone()),
            FsEntry::File(FsEntryFile { entry, .. }) => Some(entry.path.clone()),
        }
    }
}
