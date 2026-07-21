use super::*;

impl ProjectPanel {
    pub(super) fn select_prev_diagnostic(
        &mut self,
        action: &SelectPrevDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_entry(
            self.selection.as_ref(),
            true,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                self.selection.is_none_or(|selection| {
                    if selection.worktree_id == worktree_id {
                        selection.entry_id != entry.id
                    } else {
                        true
                    }
                }) && entry.is_file()
                    && self
                        .diagnostics
                        .get(&(worktree_id, entry.path.clone()))
                        .is_some_and(|severity| action.severity.matches(*severity))
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.expand_entry(selection.worktree_id, selection.entry_id, cx);
            self.update_visible_entries(
                Some((selection.worktree_id, selection.entry_id)),
                false,
                true,
                window,
                cx,
            );
            cx.notify();
        }
    }

    pub(super) fn select_next_diagnostic(
        &mut self,
        action: &SelectNextDiagnostic,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_entry(
            self.selection.as_ref(),
            false,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                self.selection.is_none_or(|selection| {
                    if selection.worktree_id == worktree_id {
                        selection.entry_id != entry.id
                    } else {
                        true
                    }
                }) && entry.is_file()
                    && self
                        .diagnostics
                        .get(&(worktree_id, entry.path.clone()))
                        .is_some_and(|severity| action.severity.matches(*severity))
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.expand_entry(selection.worktree_id, selection.entry_id, cx);
            self.update_visible_entries(
                Some((selection.worktree_id, selection.entry_id)),
                false,
                true,
                window,
                cx,
            );
            cx.notify();
        }
    }

    pub(super) fn select_prev_git_entry(
        &mut self,
        _: &SelectPrevGitEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_entry(
            self.selection.as_ref(),
            true,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                (self.selection.is_none()
                    || self.selection.is_some_and(|selection| {
                        if selection.worktree_id == worktree_id {
                            selection.entry_id != entry.id
                        } else {
                            true
                        }
                    }))
                    && entry.is_file()
                    && entry.git_summary.index.modified + entry.git_summary.worktree.modified > 0
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.expand_entry(selection.worktree_id, selection.entry_id, cx);
            self.update_visible_entries(
                Some((selection.worktree_id, selection.entry_id)),
                false,
                true,
                window,
                cx,
            );
            cx.notify();
        }
    }

    pub(super) fn select_prev_directory(
        &mut self,
        _: &SelectPrevDirectory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_visible_entry(
            self.selection.as_ref(),
            true,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                self.selection.is_none_or(|selection| {
                    if selection.worktree_id == worktree_id {
                        selection.entry_id != entry.id
                    } else {
                        true
                    }
                }) && entry.is_dir()
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.autoscroll(cx);
            cx.notify();
        }
    }

    pub(super) fn select_next_directory(
        &mut self,
        _: &SelectNextDirectory,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_visible_entry(
            self.selection.as_ref(),
            false,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                self.selection.is_none_or(|selection| {
                    if selection.worktree_id == worktree_id {
                        selection.entry_id != entry.id
                    } else {
                        true
                    }
                }) && entry.is_dir()
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.autoscroll(cx);
            cx.notify();
        }
    }

    pub(super) fn select_next_git_entry(
        &mut self,
        _: &SelectNextGitEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = self.find_entry(
            self.selection.as_ref(),
            false,
            &|entry: GitEntryRef, worktree_id: WorktreeId| {
                self.selection.is_none_or(|selection| {
                    if selection.worktree_id == worktree_id {
                        selection.entry_id != entry.id
                    } else {
                        true
                    }
                }) && entry.is_file()
                    && entry.git_summary.index.modified + entry.git_summary.worktree.modified > 0
            },
            cx,
        );

        if let Some(selection) = selection {
            self.selection = Some(selection);
            self.expand_entry(selection.worktree_id, selection.entry_id, cx);
            self.update_visible_entries(
                Some((selection.worktree_id, selection.entry_id)),
                false,
                true,
                window,
                cx,
            );
            cx.notify();
        }
    }
}
