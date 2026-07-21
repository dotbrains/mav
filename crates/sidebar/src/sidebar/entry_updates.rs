use super::*;

impl Sidebar {
    pub(super) fn schedule_update_entries(
        &mut self,
        select_first_after_update: bool,
        cx: &mut Context<Self>,
    ) {
        if self.update_task.is_some() && !select_first_after_update {
            return;
        }

        self.update_task = Some(cx.spawn(async move |this, cx| {
            this.update(cx, |this, cx| {
                this.update_task = None;
                this.update_entries(cx);
                if select_first_after_update {
                    this.select_first_entry();
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Rebuilds the sidebar's visible entries from already-cached state.
    pub(super) fn update_entries(&mut self, cx: &mut Context<Self>) {
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };
        if !multi_workspace.read(cx).multi_workspace_enabled(cx) {
            return;
        }

        let had_notifications = self.has_notifications(cx);
        let previous_shapes: Vec<EntryShape> =
            self.entry_shapes(multi_workspace.read(cx)).collect();

        self.rebuild_contents(cx);
        self.refresh_refilled_draft_times(cx);
        self.refresh_draft_editor_observations(cx);

        // Preserve measurements for unchanged entries so sticky headers do not flicker.
        self.apply_list_state_diff(&previous_shapes, multi_workspace.read(cx));

        self.prefetch_worktree_default_branches(cx);

        if had_notifications != self.has_notifications(cx) {
            multi_workspace.update(cx, |_, cx| {
                cx.notify();
            });
        }

        cx.notify();
    }

    /// Splices only the changed entry range, leaving unchanged item measurements intact.
    pub(super) fn apply_list_state_diff(
        &self,
        previous_shapes: &[EntryShape],
        multi_workspace: &MultiWorkspace,
    ) {
        let mut new_iter = self.entry_shapes(multi_workspace);
        let mut prefix_len = 0;
        let leading_new = loop {
            match (previous_shapes.get(prefix_len), new_iter.next()) {
                (Some(prev), Some(next)) if *prev == next => prefix_len += 1,
                (None, None) => return,
                (_, leading) => break leading,
            }
        };

        let new_tail: Vec<EntryShape> = leading_new.into_iter().chain(new_iter).collect();
        let prev_tail = &previous_shapes[prefix_len..];
        let suffix_len = prev_tail
            .iter()
            .rev()
            .zip(new_tail.iter().rev())
            .take_while(|(prev, next)| prev == next)
            .count();

        let old_changed = prefix_len..previous_shapes.len() - suffix_len;
        let new_changed_count = new_tail.len() - suffix_len;
        self.list_state.splice(old_changed, new_changed_count);
    }

    pub(super) fn entry_shapes<'a>(
        &'a self,
        multi_workspace: &'a MultiWorkspace,
    ) -> impl Iterator<Item = EntryShape> + 'a {
        self.contents.entries.iter().map(move |entry| match entry {
            ListEntry::ProjectHeader {
                key, has_threads, ..
            } => EntryShape::ProjectHeader {
                key: key.clone(),
                has_threads: *has_threads,
                is_collapsed: multi_workspace
                    .group_state_by_key(key)
                    .map(|state| !state.expanded)
                    .unwrap_or(false),
            },
            ListEntry::Thread(thread) => EntryShape::Thread(thread.metadata.thread_id),
            ListEntry::Terminal(terminal) => EntryShape::Terminal(terminal.metadata.terminal_id),
        })
    }

    /// Detects drafts that just went from empty back to having content and
    /// refreshes their interaction time to now, so a re-filled draft sorts to
    /// the top of the list instead of falling back to its original creation time.
    pub(super) fn refresh_refilled_draft_times(&mut self, cx: &mut Context<Self>) {
        let mut new_kinds: HashMap<ThreadId, DraftKind> = HashMap::new();
        let mut refilled: Vec<ThreadId> = Vec::new();

        for entry in &self.contents.entries {
            let ListEntry::Thread(thread) = entry else {
                continue;
            };
            let Some(kind) = thread.draft else {
                continue;
            };
            let thread_id = thread.metadata.thread_id;

            if kind == DraftKind::WithContent
                && self.draft_kinds.get(&thread_id) == Some(&DraftKind::Empty)
            {
                refilled.push(thread_id);
            }
            new_kinds.insert(thread_id, kind);
        }
        self.draft_kinds = new_kinds;

        if refilled.is_empty() {
            return;
        }

        let now = Utc::now();

        ThreadMetadataStore::global(cx).update(cx, |store, store_cx| {
            for thread_id in refilled {
                store.update_interacted_at(&thread_id, now, store_cx);
            }
        });
    }

    /// Re-establishes subscriptions to each visible draft's message editor
    /// so we rebuild entries (and their displayed titles) as the user types.
    pub(super) fn refresh_draft_editor_observations(&mut self, cx: &mut Context<Self>) {
        self._draft_editor_observations.clear();
        let Some(multi_workspace) = self.multi_workspace.upgrade() else {
            return;
        };

        let draft_conversation_views: Vec<Entity<agent_ui::ConversationView>> = multi_workspace
            .read(cx)
            .workspaces()
            .flat_map(|ws| {
                ws.read(cx)
                    .items_of_type::<AgentThreadItem>(cx)
                    .map(|item| item.read(cx).conversation_view())
            })
            .collect();

        for cv in draft_conversation_views {
            if let Some(thread_view) = cv.read(cx).active_thread() {
                let editor = thread_view.read(cx).message_editor.clone();
                self._draft_editor_observations.push(cx.subscribe(
                    &editor,
                    |this, _editor, event, cx| match event {
                        MessageEditorEvent::Edited => this.schedule_update_entries(false, cx),
                        _ => (),
                    },
                ));
            }
            // Also subscribe to the ConversationView itself so that editor
            // replacements during lifecycle transitions (Loading →
            // Connected) re-wire the editor observation above.
            self._draft_editor_observations.push(cx.subscribe(
                &cv,
                |this, _cv, _event: &StateChange, cx| {
                    this.schedule_update_entries(false, cx);
                },
            ));
        }
    }
}
