use super::*;

impl Editor {
    pub(crate) fn refresh_code_lenses(
        &mut self,
        for_buffer: Option<BufferId>,
        _window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !self.lsp_data_enabled() || self.code_lens.is_none() {
            return;
        }
        let Some(project) = self.project.clone() else {
            return;
        };

        let buffers_to_query = self
            .visible_buffers(cx)
            .into_iter()
            .filter(|buffer| self.is_lsp_relevant(buffer.read(cx).file(), cx))
            .chain(for_buffer.and_then(|buffer_id| self.buffer.read(cx).buffer(buffer_id)))
            .filter(|editor_buffer| {
                let editor_buffer_id = editor_buffer.read(cx).remote_id();
                for_buffer.is_none_or(|buffer_id| buffer_id == editor_buffer_id)
                    && self.registered_buffers.contains_key(&editor_buffer_id)
            })
            .unique_by(|buffer| buffer.read(cx).remote_id())
            .collect::<Vec<_>>();

        if buffers_to_query.is_empty() {
            return;
        }

        let project = project.downgrade();
        self.refresh_code_lens_task = cx.spawn(async move |editor, cx| {
            cx.background_executor()
                .timer(LSP_REQUEST_DEBOUNCE_TIMEOUT)
                .await;

            let Some(tasks_per_buffer) = project
                .update(cx, |project, cx| {
                    project.lsp_store().update(cx, |lsp_store, cx| {
                        buffers_to_query
                            .into_iter()
                            .map(|buffer| {
                                let buffer_id = buffer.read(cx).remote_id();
                                let task = lsp_store.code_lens_actions(&buffer, cx);
                                async move { (buffer_id, task.await) }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .ok()
            else {
                return;
            };

            let code_lens_per_buffer = join_all(tasks_per_buffer).await;
            if code_lens_per_buffer.is_empty() {
                return;
            }

            editor
                .update(cx, |editor, cx| {
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    for (buffer_id, result) in code_lens_per_buffer {
                        let actions = match result {
                            Ok(Some(actions)) => actions,
                            Ok(None) => continue,
                            Err(e) => {
                                log::error!(
                                    "Failed to fetch code lenses for buffer {buffer_id:?}: {e:#}"
                                );
                                continue;
                            }
                        };
                        editor.apply_lens_actions_for_buffer(buffer_id, actions, &snapshot, cx);
                    }
                    editor.resolve_visible_code_lenses(cx);
                })
                .ok();
        });
    }

    /// Reconcile blocks for `buffer_id` against the latest `actions`.
    ///
    /// Lenses without a command cannot be rendered, as it's the only textual data in the [`lsp::CodeLens`].
    /// Worst case, we can know it only after asynchronously issuing a resolve request to the server.
    /// To avoid flickering during typing, keep a placeholder block for each lens and replace it with a resolved command's block when available,
    /// or with a synthetic "0 references" title if the server resolve did not return a command.
    ///
    /// Also keep the old block until the fresh resolve lands, to avoid flickering during typing.
    fn apply_lens_actions_for_buffer(
        &mut self,
        buffer_id: BufferId,
        actions: CodeLensActions,
        snapshot: &MultiBufferSnapshot,
        cx: &mut Context<Self>,
    ) {
        let mut all_lenses = Vec::new();
        for (_, action) in actions.iter().sorted_by_key(|(id, _)| **id) {
            let Some(position) = snapshot.anchor_in_excerpt(action.range.start) else {
                continue;
            };
            if let project::LspAction::CodeLens(lens) = &action.lsp_action {
                let title = lens
                    .command
                    .as_ref()
                    .filter(|cmd| !cmd.title.is_empty())
                    .map(|cmd| SharedString::from(&cmd.title));
                all_lenses.push((
                    position,
                    CodeLensItem {
                        title,
                        action: action.clone(),
                    },
                ));
            }
        }

        let mut new_lines_by_row = group_lenses_by_row(all_lenses, snapshot)
            .map(|line| (MultiBufferRow(line.position.to_point(snapshot).row), line))
            .collect::<HashMap<_, _>>();

        let editor_handle = cx.entity().downgrade();
        let code_lens = self.code_lens.get_or_insert_with(CodeLensState::default);
        let old_blocks = code_lens.blocks.remove(&buffer_id).unwrap_or_default();

        let mut kept_blocks = Vec::new();
        let mut renderers_to_replace = HashMap::default();
        let mut blocks_to_remove = HashSet::default();
        let mut covered_rows = HashSet::default();

        for old in old_blocks {
            let row = MultiBufferRow(old.anchor.to_point(snapshot).row);
            let Some(new_line) = new_lines_by_row.remove(&row) else {
                blocks_to_remove.insert(old.block_id);
                continue;
            };
            covered_rows.insert(row);
            let new_all_pending = new_line
                .items
                .iter()
                .all(|item| item.title.is_none() && !item.action.resolved);
            let old_has_rendered = old
                .line
                .items
                .iter()
                .any(|item| displayed_title(item).is_some());
            if new_all_pending && old_has_rendered {
                kept_blocks.push(old);
                continue;
            }
            if rendered_text_matches(&old.line, &new_line) {
                kept_blocks.push(old);
            } else {
                let mut updated = old;
                updated.line = new_line.clone();
                renderers_to_replace.insert(
                    updated.block_id,
                    build_code_lens_renderer(new_line, editor_handle.clone()),
                );
                kept_blocks.push(updated);
            }
        }

        let mut to_insert = Vec::new();
        for (row, new_line) in new_lines_by_row {
            if covered_rows.contains(&row) {
                continue;
            }
            let anchor = new_line.position;
            let props = BlockProperties {
                placement: BlockPlacement::Above(anchor),
                height: Some(1),
                style: BlockStyle::Spacer,
                render: build_code_lens_renderer(new_line.clone(), editor_handle.clone()),
                priority: 0,
            };
            to_insert.push((props, anchor, new_line));
        }

        if !blocks_to_remove.is_empty() {
            self.remove_blocks(blocks_to_remove, None, cx);
        }
        if !renderers_to_replace.is_empty() {
            self.replace_blocks(renderers_to_replace, None, cx);
        }
        if !to_insert.is_empty() {
            let mut props = Vec::with_capacity(to_insert.len());
            let mut metadata = Vec::with_capacity(to_insert.len());
            for (p, anchor, line) in to_insert {
                props.push(p);
                metadata.push((anchor, line));
            }
            let block_ids = self.insert_blocks(props, None, cx);
            for (block_id, (anchor, line)) in block_ids.into_iter().zip(metadata) {
                kept_blocks.push(CodeLensBlock {
                    block_id,
                    anchor,
                    line,
                });
            }
        }

        let code_lens = self.code_lens.get_or_insert_with(CodeLensState::default);
        if actions.is_empty() {
            code_lens.actions.remove(&buffer_id);
        } else {
            code_lens.actions.insert(buffer_id, actions);
        }
        if kept_blocks.is_empty() {
            code_lens.blocks.remove(&buffer_id);
        } else {
            code_lens.blocks.insert(buffer_id, kept_blocks);
        }
        cx.notify();
    }

    pub fn supports_code_lens(&self, cx: &ui::App) -> bool {
        let Some(project) = self.project.as_ref() else {
            return false;
        };
        let lsp_store = project.read(cx).lsp_store().read(cx);
        lsp_store
            .lsp_server_capabilities
            .values()
            .any(|caps| caps.code_lens_provider.is_some())
    }

    pub fn code_lens_enabled(&self) -> bool {
        self.code_lens.is_some()
    }

    pub fn toggle_code_lens_action(
        &mut self,
        _: &ToggleCodeLens,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let currently_enabled = self.code_lens.is_some();
        self.toggle_code_lens(!currently_enabled, window, cx);
    }

    pub(crate) fn toggle_code_lens(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if enabled {
            self.code_lens.get_or_insert_with(CodeLensState::default);
            self.refresh_code_lenses(None, window, cx);
        } else {
            self.clear_code_lenses(cx);
        }
    }

    pub(crate) fn resolve_visible_code_lenses(&mut self, cx: &mut Context<Self>) {
        if !self.lsp_data_enabled() || self.code_lens.is_none() {
            return;
        }
        let Some(project) = self.project.clone() else {
            return;
        };

        let lsp_store = project.read(cx).lsp_store();

        let mut pending_resolves = Vec::new();
        for (buffer_snapshot, visible_range, _) in self.visible_buffer_ranges(cx) {
            let buffer_id = buffer_snapshot.remote_id();
            let Some(buffer) = self.buffer.read(cx).buffer(buffer_id) else {
                continue;
            };
            let Some(actions) = self
                .code_lens
                .as_ref()
                .and_then(|state| state.actions.get(&buffer_id))
            else {
                continue;
            };
            for (lens_id, action) in actions {
                if action.resolved {
                    continue;
                }
                if let project::LspAction::CodeLens(lens) = &action.lsp_action {
                    if lens.command.is_some() {
                        continue;
                    }
                }
                let action_offset = action.range.start.to_offset(&buffer_snapshot);
                if action_offset < visible_range.start.0 || action_offset > visible_range.end.0 {
                    continue;
                }
                let resolve_task = lsp_store.update(cx, |lsp_store, cx| {
                    lsp_store.resolve_code_lens(&buffer, action.server_id, *lens_id, cx)
                });
                pending_resolves.push((buffer_id, resolve_task));
            }
        }
        if pending_resolves.is_empty() {
            return;
        }

        let code_lens = self.code_lens.get_or_insert_with(CodeLensState::default);
        code_lens.resolve_task = cx.spawn(async move |editor, cx| {
            let mut resolves_in_progress = pending_resolves
                .into_iter()
                .map(|(buffer_id, task)| async move { (buffer_id, task.await) })
                .collect::<FuturesUnordered<_>>();
            while let Some((buffer_id, resolve_result)) = resolves_in_progress.next().await {
                let Some((resolved_id, resolved)) = resolve_result else {
                    continue;
                };
                editor
                    .update(cx, |editor, cx| {
                        let snapshot = editor.buffer().read(cx).snapshot(cx);
                        let Some(mut actions) = editor
                            .code_lens
                            .as_ref()
                            .and_then(|state| state.actions.get(&buffer_id))
                            .cloned()
                        else {
                            return;
                        };
                        if let Some(slot) = actions.get_mut(&resolved_id) {
                            *slot = resolved;
                        }
                        editor.apply_lens_actions_for_buffer(buffer_id, actions, &snapshot, cx);
                    })
                    .ok();
            }
        });
    }

    pub(crate) fn clear_code_lenses(&mut self, cx: &mut Context<Self>) {
        if let Some(code_lens) = self.code_lens.take() {
            let all_blocks = code_lens
                .blocks
                .into_values()
                .flatten()
                .map(|block| block.block_id)
                .collect::<HashSet<_>>();
            if !all_blocks.is_empty() {
                self.remove_blocks(all_blocks, None, cx);
            }
            cx.notify();
        }
        self.refresh_code_lens_task = Task::ready(());
    }
}
