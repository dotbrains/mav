use super::*;

impl OutlinePanel {
    pub(super) fn update_fs_entries(
        &mut self,
        active_editor: Entity<Editor>,
        debounce: Option<Duration>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.active {
            return;
        }

        if debounce.is_some() && self.fs_entries_update_pending {
            return;
        }
        self.fs_entries_update_pending = true;

        self.fs_entries_update_task = cx.spawn_in(window, async move |outline_panel, cx| {
            if let Some(debounce) = debounce {
                cx.background_executor().timer(debounce).await;
            }

            let mut new_collapsed_entries = HashSet::default();
            let mut new_unfolded_dirs = HashMap::default();
            let mut root_entries = HashSet::default();
            let mut new_buffers = HashMap::<BufferId, BufferOutlines>::default();
            let Ok((buffer_excerpts, auto_fold_dirs, repo_snapshots)) =
                outline_panel.update(cx, |outline_panel, cx| {
                    outline_panel.fs_entries_update_pending = false;
                    let auto_fold_dirs = OutlinePanelSettings::get_global(cx).auto_fold_dirs;
                    let active_multi_buffer = active_editor.read(cx).buffer().clone();
                    let new_entries = outline_panel.new_entries_for_fs_update.clone();
                    let repo_snapshots = outline_panel.project.update(cx, |project, cx| {
                        project.git_store().read(cx).repo_snapshots(cx)
                    });
                    let git_store = outline_panel.project.read(cx).git_store().clone();
                    new_collapsed_entries = outline_panel.collapsed_entries.clone();
                    new_unfolded_dirs = outline_panel.unfolded_dirs.clone();
                    let multi_buffer_snapshot = active_multi_buffer.read(cx).snapshot(cx);

                    let buffer_excerpts = multi_buffer_snapshot.excerpts().fold(
                        HashMap::default(),
                        |mut buffer_excerpts, excerpt_range| {
                            let Some(buffer_snapshot) = multi_buffer_snapshot
                                .buffer_for_id(excerpt_range.context.start.buffer_id)
                            else {
                                return buffer_excerpts;
                            };
                            let buffer_id = buffer_snapshot.remote_id();
                            let file = File::from_dyn(buffer_snapshot.file());
                            let entry_id = file.and_then(|file| file.project_entry_id());
                            let worktree = file.map(|file| file.worktree.read(cx).snapshot());
                            let is_new = new_entries.contains(&buffer_id)
                                || !outline_panel.buffers.contains_key(&buffer_id);
                            let is_folded = active_editor.read(cx).is_buffer_folded(buffer_id, cx);
                            let status = git_store
                                .read(cx)
                                .repository_and_path_for_buffer_id(buffer_id, cx)
                                .and_then(|(repo, path)| {
                                    Some(repo.read(cx).status_for_path(&path)?.status)
                                });
                            buffer_excerpts
                                .entry(buffer_id)
                                .or_insert_with(|| {
                                    (is_new, is_folded, Vec::new(), entry_id, worktree, status)
                                })
                                .2
                                .push(excerpt_range.clone());

                            new_buffers
                                .entry(buffer_id)
                                .or_insert_with(|| {
                                    let outlines = match outline_panel.buffers.get(&buffer_id) {
                                        Some(old_buffer) => match &old_buffer.outlines {
                                            OutlineState::Outlines(outlines) => {
                                                OutlineState::Outlines(outlines.clone())
                                            }
                                            OutlineState::Invalidated(_) => {
                                                OutlineState::NotFetched
                                            }
                                            OutlineState::NotFetched => OutlineState::NotFetched,
                                        },
                                        None => OutlineState::NotFetched,
                                    };
                                    BufferOutlines {
                                        outlines,
                                        excerpts: Vec::new(),
                                    }
                                })
                                .excerpts
                                .push(excerpt_range);
                            buffer_excerpts
                        },
                    );
                    (buffer_excerpts, auto_fold_dirs, repo_snapshots)
                })
            else {
                return;
            };

            let Some((
                new_collapsed_entries,
                new_unfolded_dirs,
                new_fs_entries,
                new_depth_map,
                new_children_count,
            )) = cx
                .background_spawn(async move {
                    let mut processed_external_buffers = HashSet::default();
                    let mut new_worktree_entries =
                        BTreeMap::<WorktreeId, HashMap<ProjectEntryId, GitEntry>>::default();
                    let mut worktree_excerpts = HashMap::<
                        WorktreeId,
                        HashMap<ProjectEntryId, (BufferId, Vec<ExcerptRange<Anchor>>)>,
                    >::default();
                    let mut external_excerpts = HashMap::default();

                    for (buffer_id, (is_new, is_folded, excerpts, entry_id, worktree, status)) in
                        buffer_excerpts
                    {
                        if is_folded {
                            match &worktree {
                                Some(worktree) => {
                                    new_collapsed_entries
                                        .insert(CollapsedEntry::File(worktree.id(), buffer_id));
                                }
                                None => {
                                    new_collapsed_entries
                                        .insert(CollapsedEntry::ExternalFile(buffer_id));
                                }
                            }
                        } else if is_new {
                            match &worktree {
                                Some(worktree) => {
                                    new_collapsed_entries
                                        .remove(&CollapsedEntry::File(worktree.id(), buffer_id));
                                }
                                None => {
                                    new_collapsed_entries
                                        .remove(&CollapsedEntry::ExternalFile(buffer_id));
                                }
                            }
                        }

                        if let Some(worktree) = worktree {
                            let worktree_id = worktree.id();
                            let unfolded_dirs = new_unfolded_dirs.entry(worktree_id).or_default();

                            match entry_id.and_then(|id| worktree.entry_for_id(id)).cloned() {
                                Some(entry) => {
                                    let entry = GitEntry {
                                        git_summary: status
                                            .map(|status| status.summary())
                                            .unwrap_or_default(),
                                        entry,
                                    };
                                    let mut traversal = GitTraversal::new(
                                        &repo_snapshots,
                                        worktree.traverse_from_path(
                                            true,
                                            true,
                                            true,
                                            entry.path.as_ref(),
                                        ),
                                    );

                                    let mut entries_to_add = HashMap::default();
                                    worktree_excerpts
                                        .entry(worktree_id)
                                        .or_default()
                                        .insert(entry.id, (buffer_id, excerpts));
                                    let mut current_entry = entry;
                                    loop {
                                        if current_entry.is_dir() {
                                            let is_root =
                                                worktree.root_entry().map(|entry| entry.id)
                                                    == Some(current_entry.id);
                                            if is_root {
                                                root_entries.insert(current_entry.id);
                                                if auto_fold_dirs {
                                                    unfolded_dirs.insert(current_entry.id);
                                                }
                                            }
                                            if is_new {
                                                new_collapsed_entries.remove(&CollapsedEntry::Dir(
                                                    worktree_id,
                                                    current_entry.id,
                                                ));
                                            }
                                        }

                                        let new_entry_added = entries_to_add
                                            .insert(current_entry.id, current_entry)
                                            .is_none();
                                        if new_entry_added
                                            && traversal.back_to_parent()
                                            && let Some(parent_entry) = traversal.entry()
                                        {
                                            current_entry = parent_entry.to_owned();
                                            continue;
                                        }
                                        break;
                                    }
                                    new_worktree_entries
                                        .entry(worktree_id)
                                        .or_insert_with(HashMap::default)
                                        .extend(entries_to_add);
                                }
                                None => {
                                    if processed_external_buffers.insert(buffer_id) {
                                        external_excerpts
                                            .entry(buffer_id)
                                            .or_insert_with(Vec::new)
                                            .extend(excerpts);
                                    }
                                }
                            }
                        } else if processed_external_buffers.insert(buffer_id) {
                            external_excerpts
                                .entry(buffer_id)
                                .or_insert_with(Vec::new)
                                .extend(excerpts);
                        }
                    }

                    let mut new_children_count =
                        HashMap::<WorktreeId, HashMap<Arc<RelPath>, FsChildren>>::default();

                    let worktree_entries = new_worktree_entries
                        .into_iter()
                        .map(|(worktree_id, entries)| {
                            let mut entries = entries.into_values().collect::<Vec<_>>();
                            entries.sort_by(|a, b| a.path.as_ref().cmp(b.path.as_ref()));
                            (worktree_id, entries)
                        })
                        .flat_map(|(worktree_id, entries)| {
                            {
                                entries
                                    .into_iter()
                                    .filter_map(|entry| {
                                        if auto_fold_dirs && let Some(parent) = entry.path.parent()
                                        {
                                            let children = new_children_count
                                                .entry(worktree_id)
                                                .or_default()
                                                .entry(Arc::from(parent))
                                                .or_default();
                                            if entry.is_dir() {
                                                children.dirs += 1;
                                            } else {
                                                children.files += 1;
                                            }
                                        }

                                        if entry.is_dir() {
                                            Some(FsEntry::Directory(FsEntryDirectory {
                                                worktree_id,
                                                entry,
                                            }))
                                        } else {
                                            let (buffer_id, excerpts) = worktree_excerpts
                                                .get_mut(&worktree_id)
                                                .and_then(|worktree_excerpts| {
                                                    worktree_excerpts.remove(&entry.id)
                                                })?;
                                            Some(FsEntry::File(FsEntryFile {
                                                worktree_id,
                                                buffer_id,
                                                entry,
                                                excerpts,
                                            }))
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            }
                        })
                        .collect::<Vec<_>>();

                    let mut visited_dirs = Vec::new();
                    let mut new_depth_map = HashMap::default();
                    let new_visible_entries = external_excerpts
                        .into_iter()
                        .sorted_by_key(|(id, _)| *id)
                        .map(|(buffer_id, excerpts)| {
                            FsEntry::ExternalFile(FsEntryExternalFile {
                                buffer_id,
                                excerpts,
                            })
                        })
                        .chain(worktree_entries)
                        .filter(|visible_item| {
                            match visible_item {
                                FsEntry::Directory(directory) => {
                                    let parent_id = back_to_common_visited_parent(
                                        &mut visited_dirs,
                                        &directory.worktree_id,
                                        &directory.entry,
                                    );

                                    let mut depth = 0;
                                    if !root_entries.contains(&directory.entry.id) {
                                        if auto_fold_dirs {
                                            let children = new_children_count
                                                .get(&directory.worktree_id)
                                                .and_then(|children_count| {
                                                    children_count.get(&directory.entry.path)
                                                })
                                                .copied()
                                                .unwrap_or_default();

                                            if !children.may_be_fold_part()
                                                || (children.dirs == 0
                                                    && visited_dirs
                                                        .last()
                                                        .map(|(parent_dir_id, _)| {
                                                            new_unfolded_dirs
                                                                .get(&directory.worktree_id)
                                                                .is_none_or(|unfolded_dirs| {
                                                                    unfolded_dirs
                                                                        .contains(parent_dir_id)
                                                                })
                                                        })
                                                        .unwrap_or(true))
                                            {
                                                new_unfolded_dirs
                                                    .entry(directory.worktree_id)
                                                    .or_default()
                                                    .insert(directory.entry.id);
                                            }
                                        }

                                        depth = parent_id
                                            .and_then(|(worktree_id, id)| {
                                                new_depth_map.get(&(worktree_id, id)).copied()
                                            })
                                            .unwrap_or(0)
                                            + 1;
                                    };
                                    visited_dirs
                                        .push((directory.entry.id, directory.entry.path.clone()));
                                    new_depth_map
                                        .insert((directory.worktree_id, directory.entry.id), depth);
                                }
                                FsEntry::File(FsEntryFile {
                                    worktree_id,
                                    entry: file_entry,
                                    ..
                                }) => {
                                    let parent_id = back_to_common_visited_parent(
                                        &mut visited_dirs,
                                        worktree_id,
                                        file_entry,
                                    );
                                    let depth = if root_entries.contains(&file_entry.id) {
                                        0
                                    } else {
                                        parent_id
                                            .and_then(|(worktree_id, id)| {
                                                new_depth_map.get(&(worktree_id, id)).copied()
                                            })
                                            .unwrap_or(0)
                                            + 1
                                    };
                                    new_depth_map.insert((*worktree_id, file_entry.id), depth);
                                }
                                FsEntry::ExternalFile(..) => {
                                    visited_dirs.clear();
                                }
                            }

                            true
                        })
                        .collect::<Vec<_>>();

                    anyhow::Ok((
                        new_collapsed_entries,
                        new_unfolded_dirs,
                        new_visible_entries,
                        new_depth_map,
                        new_children_count,
                    ))
                })
                .await
                .log_err()
            else {
                return;
            };

            outline_panel
                .update_in(cx, |outline_panel, window, cx| {
                    outline_panel.new_entries_for_fs_update.clear();
                    outline_panel.buffers = new_buffers;
                    outline_panel.collapsed_entries = new_collapsed_entries;
                    outline_panel.unfolded_dirs = new_unfolded_dirs;
                    outline_panel.fs_entries = new_fs_entries;
                    outline_panel.fs_entries_depth = new_depth_map;
                    outline_panel.fs_children_count = new_children_count;
                    outline_panel.update_non_fs_items(window, cx);

                    // Only update cached entries if we don't have outlines to fetch
                    // If we do have outlines to fetch, let fetch_outdated_outlines handle the update
                    if outline_panel.buffers_to_fetch().is_empty() {
                        outline_panel.update_cached_entries(debounce, window, cx);
                    }

                    cx.notify();
                })
                .ok();
        });
    }
}
