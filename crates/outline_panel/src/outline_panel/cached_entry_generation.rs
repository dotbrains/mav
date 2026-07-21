use super::*;

impl OutlinePanel {
    pub(super) fn generate_cached_entries(
        &self,
        is_singleton: bool,
        query: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<(Vec<CachedEntry>, Option<usize>)> {
        let project = self.project.clone();
        let Some(active_editor) = self.active_editor() else {
            return Task::ready((Vec::new(), None));
        };
        cx.spawn_in(window, async move |outline_panel, cx| {
            let mut generation_state = GenerationState::default();

            let Ok(()) = outline_panel.update(cx, |outline_panel, cx| {
                let auto_fold_dirs = OutlinePanelSettings::get_global(cx).auto_fold_dirs;
                let mut folded_dirs_entry = None::<(usize, FoldedDirsEntry)>;
                let track_matches = query.is_some();

                #[derive(Debug)]
                struct ParentStats {
                    path: Arc<RelPath>,
                    folded: bool,
                    expanded: bool,
                    depth: usize,
                }

                let search_precomputed =
                    if let ItemsDisplayMode::Search(search_state) = &outline_panel.mode {
                        let multi_buffer_snapshot =
                            active_editor.read(cx).buffer().read(cx).snapshot(cx);
                        let mut folded_buffers = HashSet::default();
                        let mut not_folded_buffers = HashSet::default();
                        let mut matches_by_buffer = HashMap::default();

                        for (match_range, search_data) in &search_state.matches {
                            let Some((start_anchor, _)) =
                                multi_buffer_snapshot.anchor_to_buffer_anchor(match_range.start)
                            else {
                                continue;
                            };
                            let start_buffer_id = start_anchor.buffer_id;
                            let end_buffer_id = multi_buffer_snapshot
                                .anchor_to_buffer_anchor(match_range.end)
                                .map(|(anchor, _)| anchor.buffer_id);

                            let mut any_folded = false;
                            for buffer_id in
                                [Some(start_buffer_id), end_buffer_id].into_iter().flatten()
                            {
                                if folded_buffers.contains(&buffer_id) {
                                    any_folded = true;
                                } else if !not_folded_buffers.contains(&buffer_id) {
                                    if active_editor.read(cx).is_buffer_folded(buffer_id, cx) {
                                        folded_buffers.insert(buffer_id);
                                        any_folded = true;
                                    } else {
                                        not_folded_buffers.insert(buffer_id);
                                    }
                                }
                            }
                            if any_folded {
                                continue;
                            }

                            matches_by_buffer
                                .entry(start_buffer_id)
                                .or_insert_with(Vec::new)
                                .push((match_range.clone(), Arc::clone(search_data)));
                        }

                        Some(SearchPrecomputed {
                            multi_buffer_snapshot,
                            matches_by_buffer,
                            folded_buffers,
                        })
                    } else {
                        None
                    };

                let mut parent_dirs = Vec::<ParentStats>::new();
                for entry in outline_panel.fs_entries.clone() {
                    let is_expanded = outline_panel.is_expanded(&entry);
                    let (depth, should_add) = match &entry {
                        FsEntry::Directory(directory_entry) => {
                            let mut should_add = true;
                            let is_root = project
                                .read(cx)
                                .worktree_for_id(directory_entry.worktree_id, cx)
                                .is_some_and(|worktree| {
                                    worktree.read(cx).root_entry() == Some(&directory_entry.entry)
                                });
                            let folded = auto_fold_dirs
                                && !is_root
                                && outline_panel
                                    .unfolded_dirs
                                    .get(&directory_entry.worktree_id)
                                    .is_none_or(|unfolded_dirs| {
                                        !unfolded_dirs.contains(&directory_entry.entry.id)
                                    });
                            let fs_depth = outline_panel
                                .fs_entries_depth
                                .get(&(directory_entry.worktree_id, directory_entry.entry.id))
                                .copied()
                                .unwrap_or(0);
                            while let Some(parent) = parent_dirs.last() {
                                if !is_root && directory_entry.entry.path.starts_with(&parent.path)
                                {
                                    break;
                                }
                                parent_dirs.pop();
                            }
                            let auto_fold = match parent_dirs.last() {
                                Some(parent) => {
                                    parent.folded
                                        && Some(parent.path.as_ref())
                                            == directory_entry.entry.path.parent()
                                        && outline_panel
                                            .fs_children_count
                                            .get(&directory_entry.worktree_id)
                                            .and_then(|entries| {
                                                entries.get(&directory_entry.entry.path)
                                            })
                                            .copied()
                                            .unwrap_or_default()
                                            .may_be_fold_part()
                                }
                                None => false,
                            };
                            let folded = folded || auto_fold;
                            let (depth, parent_expanded, parent_folded) = match parent_dirs.last() {
                                Some(parent) => {
                                    let parent_folded = parent.folded;
                                    let parent_expanded = parent.expanded;
                                    let new_depth = if parent_folded {
                                        parent.depth
                                    } else {
                                        parent.depth + 1
                                    };
                                    parent_dirs.push(ParentStats {
                                        path: directory_entry.entry.path.clone(),
                                        folded,
                                        expanded: parent_expanded && is_expanded,
                                        depth: new_depth,
                                    });
                                    (new_depth, parent_expanded, parent_folded)
                                }
                                None => {
                                    parent_dirs.push(ParentStats {
                                        path: directory_entry.entry.path.clone(),
                                        folded,
                                        expanded: is_expanded,
                                        depth: fs_depth,
                                    });
                                    (fs_depth, true, false)
                                }
                            };

                            if let Some((folded_depth, mut folded_dirs)) = folded_dirs_entry.take()
                            {
                                if folded
                                    && directory_entry.worktree_id == folded_dirs.worktree_id
                                    && directory_entry.entry.path.parent()
                                        == folded_dirs
                                            .entries
                                            .last()
                                            .map(|entry| entry.path.as_ref())
                                {
                                    folded_dirs.entries.push(directory_entry.entry.clone());
                                    folded_dirs_entry = Some((folded_depth, folded_dirs))
                                } else {
                                    if !is_singleton {
                                        let start_of_collapsed_dir_sequence = !parent_expanded
                                            && parent_dirs
                                                .iter()
                                                .rev()
                                                .nth(folded_dirs.entries.len() + 1)
                                                .is_none_or(|parent| parent.expanded);
                                        if start_of_collapsed_dir_sequence
                                            || parent_expanded
                                            || query.is_some()
                                        {
                                            if parent_folded {
                                                folded_dirs
                                                    .entries
                                                    .push(directory_entry.entry.clone());
                                                should_add = false;
                                            }
                                            let new_folded_dirs =
                                                PanelEntry::FoldedDirs(folded_dirs.clone());
                                            outline_panel.push_entry(
                                                &mut generation_state,
                                                track_matches,
                                                new_folded_dirs,
                                                folded_depth,
                                                cx,
                                            );
                                        }
                                    }

                                    folded_dirs_entry = if parent_folded {
                                        None
                                    } else {
                                        Some((
                                            depth,
                                            FoldedDirsEntry {
                                                worktree_id: directory_entry.worktree_id,
                                                entries: vec![directory_entry.entry.clone()],
                                            },
                                        ))
                                    };
                                }
                            } else if folded {
                                folded_dirs_entry = Some((
                                    depth,
                                    FoldedDirsEntry {
                                        worktree_id: directory_entry.worktree_id,
                                        entries: vec![directory_entry.entry.clone()],
                                    },
                                ));
                            }

                            let should_add =
                                should_add && parent_expanded && folded_dirs_entry.is_none();
                            (depth, should_add)
                        }
                        FsEntry::ExternalFile(..) => {
                            if let Some((folded_depth, folded_dir)) = folded_dirs_entry.take() {
                                let parent_expanded = parent_dirs
                                    .iter()
                                    .rev()
                                    .find(|parent| {
                                        folded_dir
                                            .entries
                                            .iter()
                                            .all(|entry| entry.path != parent.path)
                                    })
                                    .is_none_or(|parent| parent.expanded);
                                if !is_singleton && (parent_expanded || query.is_some()) {
                                    outline_panel.push_entry(
                                        &mut generation_state,
                                        track_matches,
                                        PanelEntry::FoldedDirs(folded_dir),
                                        folded_depth,
                                        cx,
                                    );
                                }
                            }
                            parent_dirs.clear();
                            (0, true)
                        }
                        FsEntry::File(file) => {
                            if let Some((folded_depth, folded_dirs)) = folded_dirs_entry.take() {
                                let parent_expanded = parent_dirs
                                    .iter()
                                    .rev()
                                    .find(|parent| {
                                        folded_dirs
                                            .entries
                                            .iter()
                                            .all(|entry| entry.path != parent.path)
                                    })
                                    .is_none_or(|parent| parent.expanded);
                                if !is_singleton && (parent_expanded || query.is_some()) {
                                    outline_panel.push_entry(
                                        &mut generation_state,
                                        track_matches,
                                        PanelEntry::FoldedDirs(folded_dirs),
                                        folded_depth,
                                        cx,
                                    );
                                }
                            }

                            let fs_depth = outline_panel
                                .fs_entries_depth
                                .get(&(file.worktree_id, file.entry.id))
                                .copied()
                                .unwrap_or(0);
                            while let Some(parent) = parent_dirs.last() {
                                if file.entry.path.starts_with(&parent.path) {
                                    break;
                                }
                                parent_dirs.pop();
                            }
                            match parent_dirs.last() {
                                Some(parent) => {
                                    let new_depth = parent.depth + 1;
                                    (new_depth, parent.expanded)
                                }
                                None => (fs_depth, true),
                            }
                        }
                    };

                    if !is_singleton
                        && (should_add || (query.is_some() && folded_dirs_entry.is_none()))
                    {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::Fs(entry.clone()),
                            depth,
                            cx,
                        );
                    }

                    match outline_panel.mode {
                        ItemsDisplayMode::Search(_) => {
                            if (is_singleton || query.is_some() || (should_add && is_expanded))
                                && let Some(search) = &search_precomputed
                            {
                                outline_panel.add_search_entries(
                                    &mut generation_state,
                                    search,
                                    &entry,
                                    depth,
                                    query.is_some(),
                                    is_singleton,
                                    cx,
                                );
                            }
                        }
                        ItemsDisplayMode::Outline => {
                            let excerpts_to_consider =
                                if is_singleton || query.is_some() || (should_add && is_expanded) {
                                    match &entry {
                                        FsEntry::File(FsEntryFile {
                                            buffer_id,
                                            excerpts,
                                            ..
                                        })
                                        | FsEntry::ExternalFile(FsEntryExternalFile {
                                            buffer_id,
                                            excerpts,
                                            ..
                                        }) => Some((*buffer_id, excerpts)),
                                        _ => None,
                                    }
                                } else {
                                    None
                                };
                            if let Some((buffer_id, _entry_excerpts)) = excerpts_to_consider
                                && !active_editor.read(cx).is_buffer_folded(buffer_id, cx)
                            {
                                outline_panel.add_buffer_entries(
                                    &mut generation_state,
                                    buffer_id,
                                    depth,
                                    track_matches,
                                    is_singleton,
                                    query.as_deref(),
                                    cx,
                                );
                            }
                        }
                    }

                    if is_singleton
                        && matches!(entry, FsEntry::File(..) | FsEntry::ExternalFile(..))
                        && !generation_state.entries.iter().any(|item| {
                            matches!(item.entry, PanelEntry::Outline(..) | PanelEntry::Search(_))
                        })
                    {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::Fs(entry.clone()),
                            0,
                            cx,
                        );
                    }
                }

                if let Some((folded_depth, folded_dirs)) = folded_dirs_entry.take() {
                    let parent_expanded = parent_dirs
                        .iter()
                        .rev()
                        .find(|parent| {
                            folded_dirs
                                .entries
                                .iter()
                                .all(|entry| entry.path != parent.path)
                        })
                        .is_none_or(|parent| parent.expanded);
                    if parent_expanded || query.is_some() {
                        outline_panel.push_entry(
                            &mut generation_state,
                            track_matches,
                            PanelEntry::FoldedDirs(folded_dirs),
                            folded_depth,
                            cx,
                        );
                    }
                }
            }) else {
                return (Vec::new(), None);
            };

            let Some(query) = query else {
                return (
                    generation_state.entries,
                    generation_state
                        .max_width_estimate_and_index
                        .map(|(_, index)| index),
                );
            };

            let mut matched_ids = match_strings(
                &generation_state.match_candidates,
                &query,
                true,
                true,
                usize::MAX,
                &AtomicBool::default(),
                cx.background_executor().clone(),
            )
            .await
            .into_iter()
            .map(|string_match| (string_match.candidate_id, string_match))
            .collect::<HashMap<_, _>>();

            let mut id = 0;
            generation_state.entries.retain_mut(|cached_entry| {
                let retain = match matched_ids.remove(&id) {
                    Some(string_match) => {
                        cached_entry.string_match = Some(string_match);
                        true
                    }
                    None => false,
                };
                id += 1;
                retain
            });

            (
                generation_state.entries,
                generation_state
                    .max_width_estimate_and_index
                    .map(|(_, index)| index),
            )
        })
    }
}
