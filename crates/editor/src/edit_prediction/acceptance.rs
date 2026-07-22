use super::*;

impl Editor {
    pub fn accept_partial_edit_prediction(
        &mut self,
        granularity: EditPredictionGranularity,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        if self.show_edit_predictions_in_menu() {
            self.hide_context_menu(window, cx);
        }

        let Some(active_edit_prediction) = self.active_edit_prediction.as_ref() else {
            return;
        };

        if !matches!(granularity, EditPredictionGranularity::Full) && self.selections.count() != 1 {
            return;
        }

        match &active_edit_prediction.completion {
            EditPrediction::MoveWithin { target, .. } => {
                let target = *target;

                if matches!(granularity, EditPredictionGranularity::Full) {
                    if let Some(position_map) = &self.last_position_map {
                        let target_row = target.to_display_point(&position_map.snapshot).row();
                        let is_visible = position_map.visible_row_range.contains(&target_row);

                        if is_visible || !self.edit_prediction_requires_modifier() {
                            self.unfold_ranges(&[target..target], true, false, cx);
                            self.change_selections(
                                SelectionEffects::scroll(Autoscroll::newest()),
                                window,
                                cx,
                                |selections| {
                                    selections.select_anchor_ranges([target..target]);
                                },
                            );
                            self.clear_row_highlights::<EditPredictionPreview>();
                            self.edit_prediction_preview
                                .set_previous_scroll_position(None);
                        } else {
                            // Highlight and request scroll
                            self.edit_prediction_preview
                                .set_previous_scroll_position(Some(
                                    position_map.snapshot.scroll_anchor,
                                ));
                            self.highlight_rows::<EditPredictionPreview>(
                                target..target,
                                |cx| cx.theme().colors().editor_highlighted_line_background,
                                RowHighlightOptions {
                                    autoscroll: true,
                                    ..Default::default()
                                },
                                cx,
                            );
                            self.request_autoscroll(Autoscroll::fit(), cx);
                        }
                    }
                } else {
                    self.change_selections(
                        SelectionEffects::scroll(Autoscroll::newest()),
                        window,
                        cx,
                        |selections| {
                            selections.select_anchor_ranges([target..target]);
                        },
                    );
                }
            }
            EditPrediction::MoveOutside { snapshot, target } => {
                if let Some(workspace) = self.workspace() {
                    Self::open_editor_at_anchor(snapshot, *target, &workspace, window, cx)
                        .detach_and_log_err(cx);
                }
            }
            EditPrediction::Edit {
                edits,
                cursor_position,
                ..
            } => {
                self.report_edit_prediction_event(
                    active_edit_prediction.completion_id.clone(),
                    true,
                    cx,
                );

                match granularity {
                    EditPredictionGranularity::Full => {
                        let transaction_id_prev = self.buffer.read(cx).last_transaction_id(cx);

                        // Compute fallback cursor position BEFORE applying the edit,
                        // so the anchor tracks through the edit correctly
                        let fallback_cursor_target = {
                            let snapshot = self.buffer.read(cx).snapshot(cx);
                            let Some((last_edit_range, _)) = edits.last() else {
                                return;
                            };
                            last_edit_range.end.bias_right(&snapshot)
                        };

                        self.buffer.update(cx, |buffer, cx| {
                            buffer.edit(edits.iter().cloned(), None, cx)
                        });

                        if let Some(provider) = self.edit_prediction_provider() {
                            provider.accept(cx);
                        }

                        // Resolve cursor position after the edit is applied
                        let cursor_target = if let Some((anchor, offset)) = cursor_position {
                            // The anchor tracks through the edit, then we add the offset
                            let snapshot = self.buffer.read(cx).snapshot(cx);
                            let base_offset = anchor.to_offset(&snapshot).0;
                            let target_offset =
                                MultiBufferOffset((base_offset + offset).min(snapshot.len().0));
                            snapshot.anchor_after(target_offset)
                        } else {
                            fallback_cursor_target
                        };

                        self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                            s.select_anchor_ranges([cursor_target..cursor_target]);
                        });

                        let selections = self.selections.disjoint_anchors_arc();
                        if let Some(transaction_id_now) =
                            self.buffer.read(cx).last_transaction_id(cx)
                        {
                            if transaction_id_prev != Some(transaction_id_now) {
                                self.selection_history
                                    .insert_transaction(transaction_id_now, selections);
                            }
                        }

                        self.update_visible_edit_prediction(window, cx);
                        if self.active_edit_prediction.is_none() {
                            self.refresh_edit_prediction(
                                true,
                                true,
                                EditPredictionRequestTrigger::PredictionAccepted,
                                window,
                                cx,
                            );
                        }
                        cx.notify();
                    }
                    _ => {
                        let snapshot = self.buffer.read(cx).snapshot(cx);
                        let cursor_offset = self
                            .selections
                            .newest::<MultiBufferOffset>(&self.display_snapshot(cx))
                            .head();

                        let insertion = edits.iter().find_map(|(range, text)| {
                            let range = range.to_offset(&snapshot);
                            if range.is_empty() && range.start == cursor_offset {
                                Some(text)
                            } else {
                                None
                            }
                        });

                        if let Some(text) = insertion {
                            let text_to_insert = match granularity {
                                EditPredictionGranularity::Word => {
                                    let mut partial = text
                                        .chars()
                                        .by_ref()
                                        .take_while(|c| c.is_alphabetic())
                                        .collect::<String>();
                                    if partial.is_empty() {
                                        partial = text
                                            .chars()
                                            .by_ref()
                                            .take_while(|c| c.is_whitespace() || !c.is_alphabetic())
                                            .collect::<String>();
                                    }
                                    partial
                                }
                                EditPredictionGranularity::Line => {
                                    if let Some(line) = text.split_inclusive('\n').next() {
                                        line.to_string()
                                    } else {
                                        text.to_string()
                                    }
                                }
                                EditPredictionGranularity::Full => unreachable!(),
                            };

                            cx.emit(EditorEvent::InputHandled {
                                utf16_range_to_replace: None,
                                text: text_to_insert.clone().into(),
                            });

                            self.replace_selections(&text_to_insert, None, window, cx, false);
                            self.refresh_edit_prediction(
                                true,
                                true,
                                EditPredictionRequestTrigger::PredictionPartiallyAccepted,
                                window,
                                cx,
                            );
                            cx.notify();
                        } else {
                            self.accept_partial_edit_prediction(
                                EditPredictionGranularity::Full,
                                window,
                                cx,
                            );
                        }
                    }
                }
            }
        }
    }

    pub fn accept_next_word_edit_prediction(
        &mut self,
        _: &AcceptNextWordEditPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_partial_edit_prediction(EditPredictionGranularity::Word, window, cx);
    }

    pub fn accept_next_line_edit_prediction(
        &mut self,
        _: &AcceptNextLineEditPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_partial_edit_prediction(EditPredictionGranularity::Line, window, cx);
    }

    pub fn accept_edit_prediction(
        &mut self,
        _: &AcceptEditPrediction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.accept_partial_edit_prediction(EditPredictionGranularity::Full, window, cx);
    }

    pub fn has_active_edit_prediction(&self) -> bool {
        self.active_edit_prediction.is_some()
    }

    /// Returns true when we're displaying the edit prediction popover below the cursor
    /// like we are not previewing and the LSP autocomplete menu is visible
    /// or we are in `when_holding_modifier` mode.
    pub fn edit_prediction_visible_in_cursor_popover(&self, has_completion: bool) -> bool {
        if self.edit_prediction_preview_is_active()
            || !self.show_edit_predictions_in_menu()
            || !self.edit_predictions_enabled()
        {
            return false;
        }

        if self.has_visible_completions_menu() {
            return true;
        }

        has_completion && self.edit_prediction_requires_modifier()
    }
}
