use super::*;

impl ProjectPanel {
    pub(super) fn index_for_entry(
        &self,
        entry_id: ProjectEntryId,
        worktree_id: WorktreeId,
    ) -> Option<(usize, usize, usize)> {
        let mut total_ix = 0;
        for (worktree_ix, visible) in self.state.visible_entries.iter().enumerate() {
            if worktree_id != visible.worktree_id {
                total_ix += visible.entries.len();
                continue;
            }

            return visible
                .entries
                .iter()
                .enumerate()
                .find(|(_, entry)| entry.id == entry_id)
                .map(|(ix, _)| (worktree_ix, ix, total_ix + ix));
        }
        None
    }

    pub(super) fn entry_at_index(&self, index: usize) -> Option<(WorktreeId, GitEntryRef<'_>)> {
        let mut offset = 0;
        for worktree in &self.state.visible_entries {
            let current_len = worktree.entries.len();
            if index < offset + current_len {
                return worktree
                    .entries
                    .get(index - offset)
                    .map(|entry| (worktree.worktree_id, entry.to_ref()));
            }
            offset += current_len;
        }
        None
    }

    pub(super) fn iter_visible_entries(
        &self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<ProjectPanel>,
        callback: &mut dyn FnMut(
            &Entry,
            usize,
            &HashSet<Arc<RelPath>>,
            &mut Window,
            &mut Context<ProjectPanel>,
        ),
    ) {
        let mut ix = 0;
        for visible in &self.state.visible_entries {
            if ix >= range.end {
                return;
            }

            if ix + visible.entries.len() <= range.start {
                ix += visible.entries.len();
                continue;
            }

            let end_ix = range.end.min(ix + visible.entries.len());
            let entry_range = range.start.saturating_sub(ix)..end_ix - ix;
            let entries = visible
                .index
                .get_or_init(|| visible.entries.iter().map(|e| e.path.clone()).collect());
            let base_index = ix + entry_range.start;
            for (i, entry) in visible.entries[entry_range].iter().enumerate() {
                let global_index = base_index + i;
                callback(entry, global_index, entries, window, cx);
            }
            ix = end_ix;
        }
    }

    pub(super) fn for_each_visible_entry(
        &self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<ProjectPanel>,
        callback: &mut dyn FnMut(
            ProjectEntryId,
            EntryDetails,
            &mut Window,
            &mut Context<ProjectPanel>,
        ),
    ) {
        let mut ix = 0;
        for visible in &self.state.visible_entries {
            if ix >= range.end {
                return;
            }

            if ix + visible.entries.len() <= range.start {
                ix += visible.entries.len();
                continue;
            }

            let end_ix = range.end.min(ix + visible.entries.len());
            let git_status_setting = {
                let settings = ProjectPanelSettings::get_global(cx);
                settings.git_status
            };
            if let Some(worktree) = self
                .project
                .read(cx)
                .worktree_for_id(visible.worktree_id, cx)
            {
                let snapshot = worktree.read(cx).snapshot();
                let root_name = snapshot.root_name();

                let entry_range = range.start.saturating_sub(ix)..end_ix - ix;
                let entries = visible
                    .index
                    .get_or_init(|| visible.entries.iter().map(|e| e.path.clone()).collect());
                for entry in visible.entries[entry_range].iter() {
                    let status = git_status_setting
                        .then_some(entry.git_summary)
                        .unwrap_or_default();

                    let mut details = self.details_for_entry(
                        entry,
                        visible.worktree_id,
                        root_name,
                        entries,
                        status,
                        None,
                        window,
                        cx,
                    );

                    if let Some(edit_state) = &self.state.edit_state {
                        let is_edited_entry = if edit_state.is_new_entry() {
                            entry.id == NEW_ENTRY_ID
                        } else {
                            entry.id == edit_state.entry_id
                                || self.state.ancestors.get(&entry.id).is_some_and(
                                    |auto_folded_dirs| {
                                        auto_folded_dirs.ancestors.contains(&edit_state.entry_id)
                                    },
                                )
                        };

                        if is_edited_entry {
                            if let Some(processing_filename) = &edit_state.processing_filename {
                                details.is_processing = true;
                                if let Some(ancestors) = edit_state
                                    .leaf_entry_id
                                    .and_then(|entry| self.state.ancestors.get(&entry))
                                {
                                    let position = ancestors.ancestors.iter().position(|entry_id| *entry_id == edit_state.entry_id).expect("Edited sub-entry should be an ancestor of selected leaf entry") + 1;
                                    let all_components = ancestors.ancestors.len();

                                    let prefix_components = all_components - position;
                                    let suffix_components = position.checked_sub(1);
                                    let mut previous_components =
                                        Path::new(&details.filename).components();
                                    let mut new_path = previous_components
                                        .by_ref()
                                        .take(prefix_components)
                                        .collect::<PathBuf>();
                                    if let Some(last_component) =
                                        processing_filename.components().next_back()
                                    {
                                        new_path.push(last_component);
                                        previous_components.next();
                                    }

                                    if suffix_components.is_some() {
                                        new_path.push(previous_components);
                                    }
                                    if let Some(str) = new_path.to_str() {
                                        details.filename.clear();
                                        details.filename.push_str(str);
                                    }
                                } else {
                                    details.filename.clear();
                                    details.filename.push_str(processing_filename.as_unix_str());
                                }
                            } else {
                                if edit_state.is_new_entry() {
                                    details.filename.clear();
                                }
                                details.is_editing = true;
                            }
                        }
                    }

                    callback(entry.id, details, window, cx);
                }
            }
            ix = end_ix;
        }
    }

    pub(super) fn find_entry_in_worktree(
        &self,
        worktree_id: WorktreeId,
        reverse_search: bool,
        only_visible_entries: bool,
        predicate: &dyn Fn(GitEntryRef, WorktreeId) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<GitEntry> {
        if only_visible_entries {
            let entries = self
                .state
                .visible_entries
                .iter()
                .find_map(|visible| {
                    if worktree_id == visible.worktree_id {
                        Some(&visible.entries)
                    } else {
                        None
                    }
                })?
                .clone();

            return utils::ReversibleIterable::new(entries.iter(), reverse_search)
                .find(|ele| predicate(ele.to_ref(), worktree_id))
                .cloned();
        }

        let repo_snapshots = self
            .project
            .read(cx)
            .git_store()
            .read(cx)
            .repo_snapshots(cx);
        let worktree = self.project.read(cx).worktree_for_id(worktree_id, cx)?;
        worktree.read_with(cx, |tree, _| {
            utils::ReversibleIterable::new(
                GitTraversal::new(&repo_snapshots, tree.entries(true, 0usize)),
                reverse_search,
            )
            .find_single_ended(|ele| predicate(*ele, worktree_id))
            .map(|ele| ele.to_owned())
        })
    }

    pub(super) fn find_entry(
        &self,
        start: Option<&SelectedEntry>,
        reverse_search: bool,
        predicate: &dyn Fn(GitEntryRef, WorktreeId) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<SelectedEntry> {
        let mut worktree_ids: Vec<_> = self
            .state
            .visible_entries
            .iter()
            .map(|worktree| worktree.worktree_id)
            .collect();
        let repo_snapshots = self
            .project
            .read(cx)
            .git_store()
            .read(cx)
            .repo_snapshots(cx);

        let mut last_found: Option<SelectedEntry> = None;

        if let Some(start) = start {
            let worktree = self
                .project
                .read(cx)
                .worktree_for_id(start.worktree_id, cx)?
                .read(cx);

            let search = {
                let entry = worktree.entry_for_id(start.entry_id)?;
                let root_entry = worktree.root_entry()?;
                let tree_id = worktree.id();

                let mut first_iter = GitTraversal::new(
                    &repo_snapshots,
                    worktree.traverse_from_path(true, true, true, entry.path.as_ref()),
                );

                if reverse_search {
                    first_iter.next();
                }

                let first = first_iter
                    .enumerate()
                    .take_until(|(count, entry)| entry.entry == root_entry && *count != 0usize)
                    .map(|(_, entry)| entry)
                    .find(|ele| predicate(*ele, tree_id))
                    .map(|ele| ele.to_owned());

                let second_iter =
                    GitTraversal::new(&repo_snapshots, worktree.entries(true, 0usize));

                let second = if reverse_search {
                    second_iter
                        .take_until(|ele| ele.id == start.entry_id)
                        .filter(|ele| predicate(*ele, tree_id))
                        .last()
                        .map(|ele| ele.to_owned())
                } else {
                    second_iter
                        .take_while(|ele| ele.id != start.entry_id)
                        .filter(|ele| predicate(*ele, tree_id))
                        .last()
                        .map(|ele| ele.to_owned())
                };

                if reverse_search {
                    Some((second, first))
                } else {
                    Some((first, second))
                }
            };

            if let Some((first, second)) = search {
                let first = first.map(|entry| SelectedEntry {
                    worktree_id: start.worktree_id,
                    entry_id: entry.id,
                });

                let second = second.map(|entry| SelectedEntry {
                    worktree_id: start.worktree_id,
                    entry_id: entry.id,
                });

                if first.is_some() {
                    return first;
                }
                last_found = second;

                let idx = worktree_ids
                    .iter()
                    .enumerate()
                    .find(|(_, ele)| **ele == start.worktree_id)
                    .map(|(idx, _)| idx);

                if let Some(idx) = idx {
                    worktree_ids.rotate_left(idx + 1usize);
                    worktree_ids.pop();
                }
            }
        }

        for tree_id in worktree_ids.into_iter() {
            if let Some(found) =
                self.find_entry_in_worktree(tree_id, reverse_search, false, &predicate, cx)
            {
                return Some(SelectedEntry {
                    worktree_id: tree_id,
                    entry_id: found.id,
                });
            }
        }

        last_found
    }

    pub(super) fn find_visible_entry(
        &self,
        start: Option<&SelectedEntry>,
        reverse_search: bool,
        predicate: &dyn Fn(GitEntryRef, WorktreeId) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<SelectedEntry> {
        let mut worktree_ids: Vec<_> = self
            .state
            .visible_entries
            .iter()
            .map(|worktree| worktree.worktree_id)
            .collect();

        let mut last_found: Option<SelectedEntry> = None;

        if let Some(start) = start {
            let entries = self
                .state
                .visible_entries
                .iter()
                .find(|worktree| worktree.worktree_id == start.worktree_id)
                .map(|worktree| &worktree.entries)?;

            let mut start_idx = entries
                .iter()
                .enumerate()
                .find(|(_, ele)| ele.id == start.entry_id)
                .map(|(idx, _)| idx)?;

            if reverse_search {
                start_idx = start_idx.saturating_add(1usize);
            }

            let (left, right) = entries.split_at_checked(start_idx)?;

            let (first_iter, second_iter) = if reverse_search {
                (
                    utils::ReversibleIterable::new(left.iter(), reverse_search),
                    utils::ReversibleIterable::new(right.iter(), reverse_search),
                )
            } else {
                (
                    utils::ReversibleIterable::new(right.iter(), reverse_search),
                    utils::ReversibleIterable::new(left.iter(), reverse_search),
                )
            };

            let first_search = first_iter.find(|ele| predicate(ele.to_ref(), start.worktree_id));
            let second_search = second_iter.find(|ele| predicate(ele.to_ref(), start.worktree_id));

            if first_search.is_some() {
                return first_search.map(|entry| SelectedEntry {
                    worktree_id: start.worktree_id,
                    entry_id: entry.id,
                });
            }

            last_found = second_search.map(|entry| SelectedEntry {
                worktree_id: start.worktree_id,
                entry_id: entry.id,
            });

            let idx = worktree_ids
                .iter()
                .enumerate()
                .find(|(_, ele)| **ele == start.worktree_id)
                .map(|(idx, _)| idx);

            if let Some(idx) = idx {
                worktree_ids.rotate_left(idx + 1usize);
                worktree_ids.pop();
            }
        }

        for tree_id in worktree_ids.into_iter() {
            if let Some(found) =
                self.find_entry_in_worktree(tree_id, reverse_search, true, &predicate, cx)
            {
                return Some(SelectedEntry {
                    worktree_id: tree_id,
                    entry_id: found.id,
                });
            }
        }

        last_found
    }

    pub(super) fn calculate_depth_and_difference(
        entry: &Entry,
        visible_worktree_entries: &HashSet<Arc<RelPath>>,
    ) -> (usize, usize) {
        let (depth, difference) = entry
            .path
            .ancestors()
            .skip(1) // Skip the entry itself
            .find_map(|ancestor| {
                if let Some(parent_entry) = visible_worktree_entries.get(ancestor) {
                    let entry_path_components_count = entry.path.components().count();
                    let parent_path_components_count = parent_entry.components().count();
                    let difference = entry_path_components_count - parent_path_components_count;
                    let depth = parent_entry
                        .ancestors()
                        .skip(1)
                        .filter(|ancestor| visible_worktree_entries.contains(*ancestor))
                        .count();
                    Some((depth + 1, difference))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| (0, entry.path.components().count()));

        (depth, difference)
    }
}
