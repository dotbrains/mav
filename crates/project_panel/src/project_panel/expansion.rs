use super::*;

impl ProjectPanel {
    pub(super) fn is_unfoldable(&self, entry: &Entry, worktree: &Worktree) -> bool {
        if !entry.is_dir() || self.state.unfolded_dir_ids.contains(&entry.id) {
            return false;
        }

        if let Some(parent_path) = entry.path.parent() {
            let snapshot = worktree.snapshot();
            let mut child_entries = snapshot.child_entries(parent_path);
            if let Some(child) = child_entries.next()
                && child_entries.next().is_none()
            {
                return child.kind.is_dir();
            }
        };
        false
    }

    pub(super) fn is_foldable(&self, entry: &Entry, worktree: &Worktree) -> bool {
        if entry.is_dir() {
            let snapshot = worktree.snapshot();

            let mut child_entries = snapshot.child_entries(&entry.path);
            if let Some(child) = child_entries.next()
                && child_entries.next().is_none()
            {
                return child.kind.is_dir();
            }
        }
        false
    }

    pub(super) fn expand_selected_entry(
        &mut self,
        _: &ExpandSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            if let Some(folded_ancestors) = self.state.ancestors.get_mut(&entry.id)
                && folded_ancestors.current_ancestor_depth > 0
            {
                folded_ancestors.current_ancestor_depth -= 1;
                cx.notify();
                return;
            }
            if entry.is_dir() {
                let worktree_id = worktree.id();
                let entry_id = entry.id;
                let expanded_dir_ids = if let Some(expanded_dir_ids) =
                    self.state.expanded_dir_ids.get_mut(&worktree_id)
                {
                    expanded_dir_ids
                } else {
                    return;
                };

                match expanded_dir_ids.binary_search(&entry_id) {
                    Ok(_) => self.select_next(&SelectNext, window, cx),
                    Err(ix) => {
                        self.project.update(cx, |project, cx| {
                            project.expand_entry(worktree_id, entry_id, cx);
                        });

                        expanded_dir_ids.insert(ix, entry_id);
                        self.update_visible_entries(None, false, false, window, cx);
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(super) fn collapse_selected_entry(
        &mut self,
        _: &CollapseSelectedEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((worktree, entry)) = self.selected_entry_handle(cx) else {
            return;
        };
        self.collapse_entry(entry.clone(), worktree, window, cx)
    }

    pub(super) fn collapse_entry(
        &mut self,
        entry: Entry,
        worktree: Entity<Worktree>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let worktree = worktree.read(cx);
        if let Some(folded_ancestors) = self.state.ancestors.get_mut(&entry.id)
            && folded_ancestors.current_ancestor_depth + 1 < folded_ancestors.max_ancestor_depth()
        {
            folded_ancestors.current_ancestor_depth += 1;
            cx.notify();
            return;
        }
        let worktree_id = worktree.id();
        let expanded_dir_ids =
            if let Some(expanded_dir_ids) = self.state.expanded_dir_ids.get_mut(&worktree_id) {
                expanded_dir_ids
            } else {
                return;
            };

        let mut entry = &entry;
        loop {
            let entry_id = entry.id;
            match expanded_dir_ids.binary_search(&entry_id) {
                Ok(ix) => {
                    expanded_dir_ids.remove(ix);
                    self.update_visible_entries(
                        Some((worktree_id, entry_id)),
                        false,
                        false,
                        window,
                        cx,
                    );
                    cx.notify();
                    break;
                }
                Err(_) => {
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
    }

    pub(super) fn collapse_selected_entry_and_children(
        &mut self,
        _: &CollapseSelectedEntryAndChildren,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            let worktree_id = worktree.id();
            let entry_id = entry.id;

            self.collapse_all_for_entry(worktree_id, entry_id, cx);

            self.update_visible_entries(Some((worktree_id, entry_id)), false, false, window, cx);
            cx.notify();
        }
    }

    pub(super) fn collapse_worktree_expanded_dirs(
        &mut self,
        worktree_id: WorktreeId,
        root_id: ProjectEntryId,
        cx: &App,
    ) {
        let single_worktree = self.project.read(cx).visible_worktrees(cx).count() == 1;
        if let Some(expanded_dir_ids) = self.state.expanded_dir_ids.get_mut(&worktree_id) {
            if single_worktree {
                expanded_dir_ids.retain(|id| id == &root_id);
            } else {
                expanded_dir_ids.clear();
            }
        }
    }

    pub(super) fn all_worktree_roots(&self, cx: &App) -> Vec<(WorktreeId, ProjectEntryId)> {
        self.project
            .read(cx)
            .visible_worktrees(cx)
            .filter_map(|worktree| {
                let worktree = worktree.read(cx);
                Some((worktree.id(), worktree.root_entry()?.id))
            })
            .collect()
    }

    pub(super) fn expand_worktree_roots(
        &mut self,
        roots: Vec<(WorktreeId, ProjectEntryId)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (worktree_id, root_id) in roots {
            self.expand_all_for_entry(worktree_id, root_id, cx);
            self.synchronously_expand_all_directories_internal(worktree_id, root_id, cx);
        }

        self.update_visible_entries(None, false, false, window, cx);
        cx.notify();
    }

    pub(super) fn collapse_worktree_roots(
        &mut self,
        roots: Vec<(WorktreeId, ProjectEntryId)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (worktree_id, root_id) in roots {
            self.collapse_worktree_expanded_dirs(worktree_id, root_id, cx);
        }

        self.update_visible_entries(None, false, false, window, cx);
        cx.notify();
    }

    pub(super) fn collapse_all_entries(
        &mut self,
        _: &CollapseAllEntries,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let roots = self.all_worktree_roots(cx);
        self.collapse_worktree_roots(roots, window, cx);
    }

    pub(super) fn expand_all_entries(
        &mut self,
        _: &ExpandAllEntries,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let roots = self.all_worktree_roots(cx);
        self.expand_worktree_roots(roots, window, cx);
    }

    pub(super) fn expand_all_for_entry_and_refresh(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.expand_all_for_entry(worktree_id, entry_id, cx);
        self.synchronously_expand_all_directories(worktree_id, entry_id, window, cx);
    }

    pub(super) fn expand_selected_entry_and_children(
        &mut self,
        _: &ExpandSelectedEntryAndChildren,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((worktree, entry)) = self.selected_entry(cx) {
            let worktree_id = worktree.id();
            let entry_id = entry.id;
            self.expand_all_for_entry_and_refresh(worktree_id, entry_id, window, cx);
        }
    }

    pub(super) fn synchronously_expand_all_directories(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.synchronously_expand_all_directories_internal(worktree_id, entry_id, cx);
        self.update_visible_entries(None, false, false, window, cx);
        cx.notify();
    }

    pub(super) fn synchronously_expand_all_directories_internal(
        &mut self,
        worktree_id: WorktreeId,
        entry_id: ProjectEntryId,
        cx: &mut Context<Self>,
    ) {
        let project = self.project.read(cx);
        let Some((worktree, expanded_dir_ids)) = project
            .worktree_for_id(worktree_id, cx)
            .zip(self.state.expanded_dir_ids.get_mut(&worktree_id))
        else {
            return;
        };

        let worktree = worktree.read(cx);
        let Some(entry) = worktree.entry_for_id(entry_id) else {
            return;
        };
        let include_ignored_dirs = !entry.is_ignored;

        if let Err(ix) = expanded_dir_ids.binary_search(&entry_id) {
            expanded_dir_ids.insert(ix, entry_id);
        }

        let mut dirs_to_expand = vec![entry_id];
        while let Some(current_id) = dirs_to_expand.pop() {
            let Some(current_entry) = worktree.entry_for_id(current_id) else {
                continue;
            };
            for child in worktree.child_entries(&current_entry.path) {
                if !child.is_dir() || (include_ignored_dirs && child.is_ignored) {
                    continue;
                }

                dirs_to_expand.push(child.id);

                if let Err(ix) = expanded_dir_ids.binary_search(&child.id) {
                    expanded_dir_ids.insert(ix, child.id);
                }
                self.state.unfolded_dir_ids.insert(child.id);
            }
        }
    }

    pub(super) fn toggle_expanded(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(worktree_id) = self.project.read(cx).worktree_id_for_entry(entry_id, cx)
            && let Some(expanded_dir_ids) = self.state.expanded_dir_ids.get_mut(&worktree_id)
        {
            self.project.update(cx, |project, cx| {
                match expanded_dir_ids.binary_search(&entry_id) {
                    Ok(ix) => {
                        expanded_dir_ids.remove(ix);
                    }
                    Err(ix) => {
                        project.expand_entry(worktree_id, entry_id, cx);
                        expanded_dir_ids.insert(ix, entry_id);
                    }
                }
            });
            self.update_visible_entries(Some((worktree_id, entry_id)), false, false, window, cx);
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    pub(super) fn toggle_expand_all(
        &mut self,
        entry_id: ProjectEntryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(worktree_id) = self.project.read(cx).worktree_id_for_entry(entry_id, cx)
            && let Some(expanded_dir_ids) = self.state.expanded_dir_ids.get_mut(&worktree_id)
        {
            match expanded_dir_ids.binary_search(&entry_id) {
                Ok(_ix) => {
                    self.collapse_all_for_entry(worktree_id, entry_id, cx);
                }
                Err(_ix) => {
                    self.expand_all_for_entry(worktree_id, entry_id, cx);
                }
            }
            self.update_visible_entries(Some((worktree_id, entry_id)), false, false, window, cx);
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    pub(super) fn expand_all_for_entry(
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
                if let Some(task) = project.expand_all_for_entry(worktree_id, entry_id, cx) {
                    task.detach();
                }

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

    pub(super) fn collapse_all_for_entry(
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
                let worktree = worktree.read(cx);
                let mut dirs_to_collapse = vec![entry_id];
                let auto_fold_enabled = ProjectPanelSettings::get_global(cx).auto_fold_dirs;
                while let Some(current_id) = dirs_to_collapse.pop() {
                    let Some(current_entry) = worktree.entry_for_id(current_id) else {
                        continue;
                    };
                    if let Ok(ix) = expanded_dir_ids.binary_search(&current_id) {
                        expanded_dir_ids.remove(ix);
                    }
                    if auto_fold_enabled {
                        self.state.unfolded_dir_ids.remove(&current_id);
                    }
                    for child in worktree.child_entries(&current_entry.path) {
                        if child.is_dir() {
                            dirs_to_collapse.push(child.id);
                        }
                    }
                }
            }
        });
    }
}
