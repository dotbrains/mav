use super::*;

impl ProjectPanel {
    pub(super) fn update_visible_entries(
        &mut self,
        new_selected_entry: Option<(WorktreeId, ProjectEntryId)>,
        focus_filename_editor: bool,
        autoscroll: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let settings = ProjectPanelSettings::get_global(cx);
        let auto_collapse_dirs = settings.auto_fold_dirs;
        let hide_gitignore = settings.hide_gitignore;
        let sort_mode = settings.sort_mode;
        let sort_order = settings.sort_order;
        let project = self.project.read(cx);
        let repo_snapshots = project.git_store().read(cx).repo_snapshots(cx);

        let old_ancestors = self.state.ancestors.clone();
        let temporary_unfolded_pending_state = self.state.temporarily_unfolded_pending_state.take();
        let mut new_state = State::derive(&self.state);
        new_state.last_worktree_root_id = project
            .visible_worktrees(cx)
            .next_back()
            .and_then(|worktree| worktree.read(cx).root_entry())
            .map(|entry| entry.id);
        let mut max_width_item = None;

        let visible_worktrees: Vec<_> = project
            .visible_worktrees(cx)
            .map(|worktree| worktree.read(cx).snapshot())
            .collect();
        let hide_root = settings.hide_root && visible_worktrees.len() == 1;
        let hide_hidden = settings.hide_hidden;

        let visible_entries_task = cx.spawn_in(window, async move |this, cx| {
            let new_state = cx
                .background_spawn(async move {
                    for worktree_snapshot in visible_worktrees {
                        let worktree_id = worktree_snapshot.id();

                        let mut new_entry_parent_id = None;
                        let mut new_entry_kind = EntryKind::Dir;
                        if let Some(edit_state) = &new_state.edit_state
                            && edit_state.worktree_id == worktree_id
                            && edit_state.is_new_entry()
                        {
                            new_entry_parent_id = Some(edit_state.entry_id);
                            new_entry_kind = if edit_state.is_dir {
                                EntryKind::Dir
                            } else {
                                EntryKind::File
                            };
                        }

                        let mut visible_worktree_entries = Vec::new();
                        let mut entry_iter =
                            GitTraversal::new(&repo_snapshots, worktree_snapshot.entries(true, 0));
                        let mut auto_folded_ancestors = vec![];
                        let worktree_abs_path = worktree_snapshot.abs_path();
                        while let Some(entry) = entry_iter.entry() {
                            if hide_root && Some(entry.entry) == worktree_snapshot.root_entry() {
                                if new_entry_parent_id == Some(entry.id) {
                                    visible_worktree_entries.push(Self::create_new_git_entry(
                                        entry.entry,
                                        entry.git_summary,
                                        new_entry_kind,
                                    ));
                                    new_entry_parent_id = None;
                                }
                                entry_iter.advance();
                                continue;
                            }
                            if auto_collapse_dirs && entry.kind.is_dir() {
                                auto_folded_ancestors.push(entry.id);
                                if !new_state.is_unfolded(&entry.id)
                                    && let Some(root_path) = worktree_snapshot.root_entry()
                                {
                                    let mut child_entries =
                                        worktree_snapshot.child_entries(&entry.path);
                                    if let Some(child) = child_entries.next()
                                        && entry.path != root_path.path
                                        && child_entries.next().is_none()
                                        && child.kind.is_dir()
                                    {
                                        entry_iter.advance();

                                        continue;
                                    }
                                }
                                let depth = temporary_unfolded_pending_state
                                    .as_ref()
                                    .and_then(|state| {
                                        if state.previously_focused_leaf_entry.worktree_id
                                            == worktree_id
                                            && state.previously_focused_leaf_entry.entry_id
                                                == entry.id
                                        {
                                            auto_folded_ancestors.iter().rev().position(|id| {
                                                *id == state.temporarily_unfolded_active_entry_id
                                            })
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap_or_else(|| {
                                        old_ancestors
                                            .get(&entry.id)
                                            .map(|ancestor| ancestor.current_ancestor_depth)
                                            .unwrap_or_default()
                                    })
                                    .min(auto_folded_ancestors.len());
                                if let Some(edit_state) = &mut new_state.edit_state
                                    && edit_state.entry_id == entry.id
                                {
                                    edit_state.depth = depth;
                                }
                                let mut ancestors = std::mem::take(&mut auto_folded_ancestors);
                                if ancestors.len() > 1 {
                                    ancestors.reverse();
                                    new_state.ancestors.insert(
                                        entry.id,
                                        FoldedAncestors {
                                            current_ancestor_depth: depth,
                                            ancestors,
                                        },
                                    );
                                }
                            }
                            auto_folded_ancestors.clear();
                            if (!hide_gitignore || !entry.is_ignored)
                                && (!hide_hidden || !entry.is_hidden)
                            {
                                visible_worktree_entries.push(entry.to_owned());
                            }
                            let precedes_new_entry = if let Some(new_entry_id) = new_entry_parent_id
                            {
                                entry.id == new_entry_id || {
                                    new_state.ancestors.get(&entry.id).is_some_and(|entries| {
                                        entries.ancestors.contains(&new_entry_id)
                                    })
                                }
                            } else {
                                false
                            };
                            if precedes_new_entry
                                && (!hide_gitignore || !entry.is_ignored)
                                && (!hide_hidden || !entry.is_hidden)
                            {
                                visible_worktree_entries.push(Self::create_new_git_entry(
                                    entry.entry,
                                    entry.git_summary,
                                    new_entry_kind,
                                ));
                            }

                            let (depth, chars) = if Some(entry.entry)
                                == worktree_snapshot.root_entry()
                            {
                                let Some(path_name) = worktree_abs_path.file_name() else {
                                    entry_iter.advance();
                                    continue;
                                };
                                let depth = 0;
                                (depth, path_name.to_string_lossy().chars().count())
                            } else if entry.is_file() {
                                let Some(path_name) = entry
                                    .path
                                    .file_name()
                                    .with_context(|| {
                                        format!("Non-root entry has no file name: {entry:?}")
                                    })
                                    .log_err()
                                else {
                                    continue;
                                };
                                let depth = entry.path.ancestors().count() - 1;
                                (depth, path_name.chars().count())
                            } else {
                                let path = new_state
                                    .ancestors
                                    .get(&entry.id)
                                    .and_then(|ancestors| {
                                        let outermost_ancestor = ancestors.ancestors.last()?;
                                        let root_folded_entry = worktree_snapshot
                                            .entry_for_id(*outermost_ancestor)?
                                            .path
                                            .as_ref();
                                        entry.path.strip_prefix(root_folded_entry).ok().and_then(
                                            |suffix| {
                                                Some(
                                                    RelPath::unix(root_folded_entry.file_name()?)
                                                        .unwrap()
                                                        .join(suffix),
                                                )
                                            },
                                        )
                                    })
                                    .or_else(|| {
                                        entry.path.file_name().map(|file_name| {
                                            RelPath::unix(file_name).unwrap().into()
                                        })
                                    })
                                    .unwrap_or_else(|| entry.path.clone());
                                let depth = path.components().count();
                                (depth, path.as_unix_str().chars().count())
                            };
                            let width_estimate =
                                item_width_estimate(depth, chars, entry.canonical_path.is_some());

                            match max_width_item.as_mut() {
                                Some((id, worktree_id, width)) => {
                                    if *width < width_estimate {
                                        *id = entry.id;
                                        *worktree_id = worktree_snapshot.id();
                                        *width = width_estimate;
                                    }
                                }
                                None => {
                                    max_width_item =
                                        Some((entry.id, worktree_snapshot.id(), width_estimate))
                                }
                            }

                            let expanded_dir_ids =
                                match new_state.expanded_dir_ids.entry(worktree_id) {
                                    hash_map::Entry::Occupied(e) => e.into_mut(),
                                    hash_map::Entry::Vacant(e) => {
                                        // The first time a worktree's root entry becomes available,
                                        // mark that root entry as expanded.
                                        if let Some(entry) = worktree_snapshot.root_entry() {
                                            e.insert(vec![entry.id]).as_slice()
                                        } else {
                                            &[]
                                        }
                                    }
                                };

                            if expanded_dir_ids.binary_search(&entry.id).is_err()
                                && entry_iter.advance_to_sibling()
                            {
                                continue;
                            }
                            entry_iter.advance();
                        }

                        par_sort_worktree_entries(
                            &mut visible_worktree_entries,
                            sort_mode,
                            sort_order,
                        );
                        new_state.visible_entries.push(VisibleEntriesForWorktree {
                            worktree_id,
                            entries: visible_worktree_entries,
                            index: OnceCell::new(),
                        })
                    }
                    if let Some((project_entry_id, worktree_id, _)) = max_width_item {
                        let mut visited_worktrees_length = 0;
                        let index = new_state
                            .visible_entries
                            .iter()
                            .find_map(|visible_entries| {
                                if worktree_id == visible_entries.worktree_id {
                                    visible_entries
                                        .entries
                                        .iter()
                                        .position(|entry| entry.id == project_entry_id)
                                } else {
                                    visited_worktrees_length += visible_entries.entries.len();
                                    None
                                }
                            });
                        if let Some(index) = index {
                            new_state.max_width_item_index = Some(visited_worktrees_length + index);
                        }
                    }
                    new_state
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.state = new_state;
                if let Some((worktree_id, entry_id)) = new_selected_entry {
                    this.selection = Some(SelectedEntry {
                        worktree_id,
                        entry_id,
                    });
                }
                let elapsed = now.elapsed();
                if this.last_reported_update.elapsed() > Duration::from_secs(3600) {
                    telemetry::event!(
                        "Project Panel Updated",
                        elapsed_ms = elapsed.as_millis() as u64,
                        worktree_entries = this
                            .state
                            .visible_entries
                            .iter()
                            .map(|worktree| worktree.entries.len())
                            .sum::<usize>(),
                    )
                }
                if this.update_visible_entries_task.focus_filename_editor {
                    this.update_visible_entries_task.focus_filename_editor = false;
                    this.filename_editor.update(cx, |editor, cx| {
                        window.focus(&editor.focus_handle(cx), cx);
                    });
                }
                if this.update_visible_entries_task.autoscroll {
                    this.update_visible_entries_task.autoscroll = false;
                    this.autoscroll(cx);
                }
                cx.notify();
            })
            .ok();
        });

        self.update_visible_entries_task = UpdateVisibleEntriesTask {
            _visible_entries_task: visible_entries_task,
            focus_filename_editor: focus_filename_editor
                || self.update_visible_entries_task.focus_filename_editor,
            autoscroll: autoscroll || self.update_visible_entries_task.autoscroll,
        };
    }

    pub(super) fn expand_entry(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        cx: &mut Context<Self>,
    ) {
        self.project.update(cx, |project, cx| {
            if let Some((worktree, expanded_dir_ids)) = project
                .worktree_for_id(worktree_id, cx)
                .zip(self.state.expanded_dir_ids.get_mut(&worktree_id))
            {
                project.expand_entry(worktree_id, entry_id, cx);
                let worktree = worktree.read(cx);

                if let Some(mut entry) = worktree.entry_for_id(entry_id) {
                    loop {
                        if let Err(ix) = expanded_dir_ids.binary_search(&entry.id) {
                            expanded_dir_ids.insert(ix, entry.id);
                        }

                        if let Some(parent_entry) =
                            entry.path.parent().and_then(|p| worktree.entry_for_path(p))
                        {
                            entry = parent_entry;
                        } else {
                            break;
                        }
                    }
                }
            }
        });
    }
}
