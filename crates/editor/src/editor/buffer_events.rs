use super::*;

impl Editor {
    pub(super) fn on_buffer_event(
        &mut self,
        multibuffer: &Entity<MultiBuffer>,
        event: &multi_buffer::Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            multi_buffer::Event::Edited {
                edited_buffer,
                source,
            } => {
                self.scrollbar_marker_state.dirty = true;
                self.active_indent_guides_state.dirty = true;
                self.refresh_active_diagnostics(cx);
                self.refresh_code_actions_for_selection(window, cx);
                self.refresh_single_line_folds(window, cx);
                let snapshot = self.snapshot(window, cx);
                self.refresh_matching_bracket_highlights(&snapshot, cx);
                self.refresh_outline_symbols_at_cursor(cx);
                self.refresh_sticky_headers(&snapshot, cx);
                if source.is_local() && self.has_active_edit_prediction() {
                    self.update_visible_edit_prediction(window, cx);
                }

                self.cleanup_orphaned_review_comments(cx);

                if let Some(buffer) = edited_buffer {
                    if buffer.read(cx).file().is_none() {
                        cx.emit(EditorEvent::TitleChanged);
                    }

                    if self.project.is_some() {
                        let buffer_id = buffer.read(cx).remote_id();
                        self.register_buffer(buffer_id, cx);
                        self.update_lsp_data(Some(buffer_id), window, cx);
                        self.refresh_inlay_hints(
                            InlayHintRefreshReason::BufferEdited(buffer_id),
                            cx,
                        );
                    }
                }

                cx.emit(EditorEvent::BufferEdited);
                cx.emit(SearchEvent::MatchesInvalidated);

                let Some(project) = &self.project else { return };
                let (telemetry, is_via_ssh) = {
                    let project = project.read(cx);
                    let telemetry = project.client().telemetry().clone();
                    let is_via_ssh = project.is_via_remote_server();
                    (telemetry, is_via_ssh)
                };
                telemetry.log_edit_event("editor", is_via_ssh);
            }
            multi_buffer::Event::BufferRangesUpdated {
                buffer,
                ranges,
                path_key,
            } => {
                self.refresh_document_highlights(cx);
                let buffer_id = buffer.read(cx).remote_id();
                if self.buffer.read(cx).diff_for(buffer_id).is_none()
                    && let Some(project) = &self.project
                {
                    update_uncommitted_diff_for_buffer(
                        cx.entity(),
                        project,
                        [buffer.clone()],
                        self.buffer.clone(),
                        cx,
                    )
                    .detach();
                }
                self.register_visible_buffers(cx);
                self.update_lsp_data(Some(buffer_id), window, cx);
                self.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
                self.refresh_runnables(None, window, cx);
                self.bracket_fetched_tree_sitter_chunks
                    .retain(|range, _| range.start.buffer_id != buffer_id);
                self.colorize_brackets(false, cx);
                self.refresh_selected_text_highlights(&self.display_snapshot(cx), true, window, cx);
                self.semantic_token_state.invalidate_buffer(&buffer_id);
                cx.emit(EditorEvent::BufferRangesUpdated {
                    buffer: buffer.clone(),
                    ranges: ranges.clone(),
                    path_key: path_key.clone(),
                });
            }
            multi_buffer::Event::BuffersRemoved { removed_buffer_ids } => {
                if let Some(inlay_hints) = &mut self.inlay_hints {
                    inlay_hints.remove_inlay_chunk_data(removed_buffer_ids);
                }
                self.refresh_inlay_hints(
                    InlayHintRefreshReason::BuffersRemoved(removed_buffer_ids.clone()),
                    cx,
                );
                for buffer_id in removed_buffer_ids {
                    self.registered_buffers.remove(buffer_id);
                    self.clear_runnables(Some(*buffer_id));
                    self.semantic_token_state.invalidate_buffer(buffer_id);
                    self.lsp_document_symbols.remove(buffer_id);
                    self.lsp_document_links.per_buffer.remove(buffer_id);
                    self.display_map.update(cx, |display_map, cx| {
                        display_map.invalidate_semantic_highlights(*buffer_id);
                        display_map.clear_lsp_folding_ranges(*buffer_id, cx);
                    });
                }

                self.display_map.update(cx, |display_map, cx| {
                    display_map.unfold_buffers(removed_buffer_ids.iter().copied(), cx);
                });

                jsx_tag_auto_close::refresh_enabled_in_any_buffer(self, multibuffer, cx);
                cx.emit(EditorEvent::BuffersRemoved {
                    removed_buffer_ids: removed_buffer_ids.clone(),
                });
            }
            multi_buffer::Event::BuffersEdited { buffer_ids } => {
                self.display_map.update(cx, |map, cx| {
                    map.unfold_buffers(buffer_ids.iter().copied(), cx)
                });
                cx.emit(EditorEvent::BuffersEdited {
                    buffer_ids: buffer_ids.clone(),
                });
            }
            multi_buffer::Event::Reparsed(buffer_id) => {
                self.refresh_runnables(Some(*buffer_id), window, cx);
                self.refresh_selected_text_highlights(&self.display_snapshot(cx), true, window, cx);
                self.colorize_brackets(true, cx);
                jsx_tag_auto_close::refresh_enabled_in_any_buffer(self, multibuffer, cx);

                cx.emit(EditorEvent::Reparsed(*buffer_id));
            }
            multi_buffer::Event::DiffHunksToggled => {
                self.refresh_runnables(None, window, cx);
            }
            multi_buffer::Event::LanguageChanged(buffer_id, is_fresh_language) => {
                if !is_fresh_language {
                    self.registered_buffers.remove(&buffer_id);
                }
                jsx_tag_auto_close::refresh_enabled_in_any_buffer(self, multibuffer, cx);
                cx.emit(EditorEvent::Reparsed(*buffer_id));
                self.update_edit_prediction_settings(cx);
                cx.notify();
            }
            multi_buffer::Event::DirtyChanged => cx.emit(EditorEvent::DirtyChanged),
            multi_buffer::Event::Saved => cx.emit(EditorEvent::Saved),
            multi_buffer::Event::FileHandleChanged => {
                cx.emit(EditorEvent::TitleChanged);
                cx.emit(EditorEvent::FileHandleChanged);
            }
            multi_buffer::Event::Reloaded | multi_buffer::Event::BufferDiffChanged => {
                cx.emit(EditorEvent::TitleChanged)
            }
            multi_buffer::Event::DiagnosticsUpdated => {
                self.update_diagnostics_state(window, cx);
            }
            _ => {}
        };
    }
}
