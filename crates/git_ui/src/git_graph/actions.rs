use super::*;

impl GitGraph {
    pub(super) fn cancel(&mut self, _: &Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_entry_idx = None;
        self.selected_commit_diff = None;
        self.selected_commit_diff_stats = None;
        self.changed_files_expanded_dirs.clear();
        cx.emit(ItemEvent::Edit);
        cx.notify();
    }

    pub(super) fn select_first(
        &mut self,
        _: &SelectFirst,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_entry(0, ScrollStrategy::Nearest, cx);
    }

    pub(super) fn select_prev(
        &mut self,
        _: &SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selected_entry_idx) = &self.selected_entry_idx {
            self.select_entry(
                selected_entry_idx.saturating_sub(1),
                ScrollStrategy::Nearest,
                cx,
            );
        } else {
            self.select_first(&SelectFirst, window, cx);
        }
    }

    pub(super) fn select_next(
        &mut self,
        _: &SelectNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selected_entry_idx) = &self.selected_entry_idx {
            self.select_entry(
                selected_entry_idx
                    .saturating_add(1)
                    .min(self.graph_data.commits.len().saturating_sub(1)),
                ScrollStrategy::Nearest,
                cx,
            );
        } else {
            self.select_prev(&SelectPrevious, window, cx);
        }
    }

    pub(super) fn select_last(
        &mut self,
        _: &SelectLast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_entry(
            self.graph_data.commits.len().saturating_sub(1),
            ScrollStrategy::Nearest,
            cx,
        );
    }

    pub(super) fn scroll_up(&mut self, _: &ScrollUp, window: &mut Window, cx: &mut Context<Self>) {
        let step = (self.visible_row_count(window, cx) / 2).max(1);
        let target_idx = self.selected_entry_idx.unwrap_or(0).saturating_sub(step);

        self.select_entry(target_idx, ScrollStrategy::Nearest, cx);
    }

    pub(super) fn scroll_down(
        &mut self,
        _: &ScrollDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(last_entry_idx) = self.graph_data.commits.len().checked_sub(1) else {
            return;
        };

        let step = (self.visible_row_count(window, cx) / 2).max(1);
        let target_idx = self
            .selected_entry_idx
            .unwrap_or(0)
            .saturating_add(step)
            .min(last_entry_idx);

        self.select_entry(target_idx, ScrollStrategy::Nearest, cx);
    }

    pub(super) fn confirm(
        &mut self,
        _: &menu::Confirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_selected_commit_view(window, cx);
    }

    pub(super) fn toggle_changed_files_view(
        &mut self,
        _: &ToggleChangedFilesView,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.changed_files_view_mode = self.changed_files_view_mode.toggled();
        self.changed_files_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    pub(super) fn search(&mut self, query: SharedString, cx: &mut Context<Self>) {
        let Some(repo) = self.get_repository(cx) else {
            return;
        };

        self.search_state.matches.clear();
        self.search_state.selected_index = None;
        self.search_state.editor.update(cx, |editor, _cx| {
            editor.set_text_style_refinement(Default::default());
        });

        if query.as_str().is_empty() {
            self.search_state.state = QueryState::Empty;
            cx.notify();
            return;
        }

        let (request_tx, request_rx) = async_channel::unbounded::<Oid>();

        repo.update(cx, |repo, cx| {
            repo.search_commits(
                self.log_source.clone(),
                SearchCommitArgs {
                    query: query.clone(),
                    case_sensitive: self.search_state.case_sensitive,
                },
                request_tx,
                cx,
            );
        });

        let search_task = cx.spawn(async move |this, cx| {
            while let Ok(first_oid) = request_rx.recv().await {
                let mut pending_oids = vec![first_oid];
                while let Ok(oid) = request_rx.try_recv() {
                    pending_oids.push(oid);
                }

                this.update(cx, |this, cx| {
                    if this.search_state.selected_index.is_none() {
                        this.search_state.selected_index = Some(0);
                        this.select_commit_by_sha(first_oid, cx);
                    }

                    this.search_state.matches.extend(pending_oids);
                    cx.notify();
                })
                .ok();
            }

            this.update(cx, |this, cx| {
                if this.search_state.matches.is_empty() {
                    this.search_state.editor.update(cx, |editor, cx| {
                        editor.set_text_style_refinement(TextStyleRefinement {
                            color: Some(Color::Error.color(cx)),
                            ..Default::default()
                        });
                    });
                }
            })
            .ok();
        });

        self.search_state.state = QueryState::Confirmed((query, search_task));
        cx.emit(ItemEvent::Edit);
    }

    pub(super) fn confirm_search(
        &mut self,
        _: &menu::Confirm,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.search_state.editor.read(cx).text(cx).into();
        self.search(query, cx);
    }

    pub(super) fn activate_search_editor_if_focused(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_state.editor.update(cx, |editor, cx| {
            if editor.is_focused(window) {
                editor.select_all(&Default::default(), window, cx);
                editor.show_cursor(cx);
            }
        });
    }

    pub(super) fn focus_next_tab_stop(
        &mut self,
        _: &FocusNextTabStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_next(cx);
        self.activate_search_editor_if_focused(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn focus_previous_tab_stop(
        &mut self,
        _: &FocusPreviousTabStop,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus_prev(cx);
        self.activate_search_editor_if_focused(window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn select_entry(
        &mut self,
        idx: usize,
        scroll_strategy: ScrollStrategy,
        cx: &mut Context<Self>,
    ) {
        if self.selected_entry_idx == Some(idx) || idx >= self.graph_data.commits.len() {
            debug_assert!(
                idx < self.graph_data.commits.len(),
                "attempted to select out of bounds index: {idx}, commits.len: {}",
                self.graph_data.commits.len()
            );
            return;
        }

        self.selected_entry_idx = Some(idx);
        self.selected_commit_diff = None;
        self.selected_commit_diff_stats = None;
        self.changed_files_expanded_dirs.clear();
        self.changed_files_scroll_handle
            .scroll_to_item(0, ScrollStrategy::Top);
        self.table_interaction_state.update(cx, |state, cx| {
            state.scroll_handle.scroll_to_item(idx, scroll_strategy);
            cx.notify();
        });

        let Some(commit) = self.graph_data.commits.get(idx) else {
            return;
        };

        let sha = commit.data.sha.to_string();

        let Some(repository) = self.get_repository(cx) else {
            return;
        };

        let diff_receiver = repository.update(cx, |repo, _| repo.load_commit_diff(sha));

        self._commit_diff_task = Some(cx.spawn(async move |this, cx| {
            if let Ok(Ok(diff)) = diff_receiver.await {
                this.update(cx, |this, cx| {
                    let stats = compute_diff_stats(&diff);
                    this.selected_commit_diff = Some(diff);
                    this.selected_commit_diff_stats = Some(stats);
                    cx.notify();
                })
                .ok();
            }
        }));

        cx.emit(ItemEvent::Edit);
        cx.notify();
    }

    pub(super) fn select_previous_match(&mut self, cx: &mut Context<Self>) {
        if self.search_state.matches.is_empty() {
            return;
        }

        let mut prev_selection = self.search_state.selected_index.unwrap_or_default();

        if prev_selection == 0 {
            prev_selection = self.search_state.matches.len() - 1;
        } else {
            prev_selection -= 1;
        }

        let Some(&oid) = self.search_state.matches.get_index(prev_selection) else {
            return;
        };

        self.search_state.selected_index = Some(prev_selection);
        self.select_commit_by_sha(oid, cx);
    }

    pub(super) fn select_next_match(&mut self, cx: &mut Context<Self>) {
        if self.search_state.matches.is_empty() {
            return;
        }

        let mut next_selection = self
            .search_state
            .selected_index
            .map(|index| index + 1)
            .unwrap_or_default();

        if next_selection >= self.search_state.matches.len() {
            next_selection = 0;
        }

        let Some(&oid) = self.search_state.matches.get_index(next_selection) else {
            return;
        };

        self.search_state.selected_index = Some(next_selection);
        self.select_commit_by_sha(oid, cx);
    }

    pub fn set_repo_id(&mut self, repo_id: RepositoryId, cx: &mut Context<Self>) {
        if repo_id != self.repo_id
            && self
                .git_store
                .read(cx)
                .repositories()
                .contains_key(&repo_id)
        {
            self.repo_id = repo_id;
            self.invalidate_state(cx);
        }
    }

    pub fn select_commit_by_sha(&mut self, sha: impl TryInto<Oid>, cx: &mut Context<Self>) {
        fn inner(this: &mut GitGraph, oid: Oid, cx: &mut Context<GitGraph>) {
            let Some(selected_repository) = this.get_repository(cx) else {
                return;
            };

            let Some(index) = selected_repository
                .read(cx)
                .get_graph_data(this.log_source.clone(), this.log_order)
                .and_then(|data| data.commit_oid_to_index.get(&oid))
                .copied()
            else {
                this.pending_select_sha = Some(oid);
                return;
            };

            this.pending_select_sha = None;
            this.select_entry(index, ScrollStrategy::Center, cx);
        }

        if let Ok(oid) = sha.try_into() {
            inner(self, oid, cx);
        }
    }

    pub(super) fn open_selected_commit_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };

        self.open_commit_view(selected_entry_index, window, cx);
    }

    pub(super) fn open_commit_view(
        &mut self,
        entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit_entry) = self.graph_data.commits.get(entry_index) else {
            return;
        };

        let Some(repository) = self.get_repository(cx) else {
            return;
        };

        CommitView::open(
            commit_entry.data.sha.to_string(),
            repository.downgrade(),
            self.workspace.clone(),
            None,
            None,
            window,
            cx,
        );
    }

    pub(super) fn copy_commit_sha(&mut self, entry_index: usize, cx: &mut Context<Self>) {
        let Some(commit) = self.graph_data.commits.get(entry_index) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(commit.data.sha.to_string()));
    }

    pub(super) fn copy_selected_commit_sha(
        &mut self,
        _: &CopyCommitSha,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };
        self.copy_commit_sha(selected_entry_index, cx);
    }

    pub(super) fn copy_commit_tag(
        &mut self,
        entry_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(commit) = self.graph_data.commits.get(entry_index) else {
            return;
        };

        let tag_names = commit
            .data
            .tag_names()
            .into_iter()
            .map(|tag_name| SharedString::from(tag_name.to_string()))
            .collect::<Vec<_>>();

        match tag_names.as_slice() {
            [] => {}
            [tag_name] => cx.write_to_clipboard(ClipboardItem::new_string(tag_name.to_string())),
            _ => {
                self.workspace
                    .update(cx, |workspace, cx| {
                        workspace.toggle_modal(window, cx, |window, cx| {
                            CommitTagPicker::new(tag_names, window, cx)
                        });
                    })
                    .ok();
            }
        }
    }

    pub(super) fn copy_selected_commit_tag(
        &mut self,
        _: &CopyCommitTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected_entry_index) = self.selected_entry_idx else {
            return;
        };
        self.copy_commit_tag(selected_entry_index, window, cx);
    }
}
