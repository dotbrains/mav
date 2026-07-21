use super::*;

impl Sidebar {
    pub(super) fn select_first_entry(&mut self) {
        self.selection = self
            .contents
            .entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Thread(_) | ListEntry::Terminal(_)))
            .or_else(|| {
                if self.contents.entries.is_empty() {
                    None
                } else {
                    Some(0)
                }
            });
    }

    pub(super) fn expand_selected_entry(
        &mut self,
        _: &SelectChild,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        match self.contents.entries.get(ix) {
            Some(ListEntry::ProjectHeader { key, .. }) => {
                let key = key.clone();
                if self.is_group_collapsed(&key, cx) {
                    self.set_group_expanded(&key, true, cx);
                    self.update_entries(cx);
                } else if ix + 1 < self.contents.entries.len() {
                    self.selection = Some(ix + 1);
                    self.list_state.scroll_to_reveal_item(ix + 1);
                    cx.notify();
                }
            }
            _ => {}
        }
    }

    pub(super) fn collapse_selected_entry(
        &mut self,
        _: &SelectParent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        match self.contents.entries.get(ix) {
            Some(ListEntry::ProjectHeader { key, .. }) => {
                let key = key.clone();
                if !self.is_group_collapsed(&key, cx) {
                    self.set_group_expanded(&key, false, cx);
                    self.update_entries(cx);
                }
            }
            Some(ListEntry::Thread(_) | ListEntry::Terminal(_)) => {
                for i in (0..ix).rev() {
                    if let Some(ListEntry::ProjectHeader { key, .. }) = self.contents.entries.get(i)
                    {
                        let key = key.clone();
                        self.selection = Some(i);
                        self.set_group_expanded(&key, false, cx);
                        self.update_entries(cx);
                        break;
                    }
                }
            }
            None => {}
        }
    }

    pub(super) fn toggle_selected_fold(
        &mut self,
        _: &editor::actions::ToggleFold,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.selection else { return };

        // Find the group header for the current selection.
        let header_ix = match self.contents.entries.get(ix) {
            Some(ListEntry::ProjectHeader { .. }) => Some(ix),
            Some(ListEntry::Thread(_) | ListEntry::Terminal(_)) => (0..ix).rev().find(|&i| {
                matches!(
                    self.contents.entries.get(i),
                    Some(ListEntry::ProjectHeader { .. })
                )
            }),
            None => None,
        };

        if let Some(header_ix) = header_ix {
            if let Some(ListEntry::ProjectHeader { key, .. }) = self.contents.entries.get(header_ix)
            {
                let key = key.clone();
                if self.is_group_collapsed(&key, cx) {
                    self.set_group_expanded(&key, true, cx);
                } else {
                    self.selection = Some(header_ix);
                    self.set_group_expanded(&key, false, cx);
                }
                self.update_entries(cx);
            }
        }
    }

    pub(super) fn fold_all(
        &mut self,
        _: &editor::actions::FoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mw) = self.multi_workspace.upgrade() {
            mw.update(cx, |mw, _cx| {
                mw.set_all_groups_expanded(false);
            });
        }
        self.update_entries(cx);
    }

    pub(super) fn unfold_all(
        &mut self,
        _: &editor::actions::UnfoldAll,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(mw) = self.multi_workspace.upgrade() {
            mw.update(cx, |mw, _cx| {
                mw.set_all_groups_expanded(true);
            });
        }
        self.update_entries(cx);
    }

    pub(super) fn stop_thread(&mut self, thread_id: &agent_ui::ThreadId, cx: &mut Context<Self>) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        let workspaces: Vec<_> = multi_workspace.read(cx).workspaces().cloned().collect();
        for workspace in workspaces {
            let item = workspace
                .read(cx)
                .items_of_type::<AgentThreadItem>(cx)
                .find(|item| item.read(cx).thread_id(cx) == *thread_id);
            if let Some(item) = item {
                item.update(cx, |item, cx| item.cancel_thread(cx));
                return;
            }
        }
    }

    /// Find the neighbor thread in the sidebar (by display position).
    /// Look below first, then above, for the nearest thread that isn't
    /// the one being archived. We capture both the neighbor's metadata
    /// (for activation) and its workspace paths (for the workspace
    /// removal fallback).
    pub(super) fn neighboring_activatable_entry(
        &self,
        current_position: usize,
    ) -> Option<ActivatableEntry> {
        let after = self
            .contents
            .entries
            .get(current_position.checked_add(1)?..)?;
        let before = self.contents.entries.get(..current_position)?;
        after
            .iter()
            .chain(before.iter().rev())
            .find_map(ActivatableEntry::from_list_entry)
    }

    pub(super) fn activate_entry(
        &mut self,
        entry: &ActivatableEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match entry {
            ActivatableEntry::Thread { metadata, .. } => {
                let Some(workspace) = self.multi_workspace.upgrade().and_then(|multi_workspace| {
                    multi_workspace
                        .read(cx)
                        .workspace_for_paths(metadata.folder_paths(), None, cx)
                }) else {
                    return false;
                };

                self.active_entry = Some(ActiveEntry::Thread {
                    thread_id: metadata.thread_id,
                    session_id: metadata.session_id.clone(),
                    workspace: workspace.clone(),
                });
                self.activate_workspace(&workspace, window, cx);
                Self::load_agent_thread_in_workspace(&workspace, metadata, true, window, cx);
                true
            }
            ActivatableEntry::Terminal {
                metadata,
                workspace,
            } => {
                self.activate_terminal_entry(
                    metadata.clone(),
                    workspace.clone(),
                    false,
                    window,
                    cx,
                );
                true
            }
        }
    }

    pub(super) fn activate_workspace(
        &self,
        workspace: &Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(multi_workspace) = self.multi_workspace.upgrade() {
            multi_workspace.update(cx, |mw, cx| {
                mw.activate(workspace.clone(), None, window, cx);
            });
        }
    }
}
