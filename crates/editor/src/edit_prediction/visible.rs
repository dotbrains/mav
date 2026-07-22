use super::*;

impl Editor {
    pub(super) fn update_visible_edit_prediction(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        if self.ime_transaction.is_some() {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return None;
        }

        let selection = self.selections.newest_anchor();
        let multibuffer = self.buffer.read(cx).snapshot(cx);
        let cursor = selection.head();
        let (cursor_text_anchor, _) = multibuffer.anchor_to_buffer_anchor(cursor)?;
        let buffer = self.buffer.read(cx).buffer(cursor_text_anchor.buffer_id)?;

        // Check project-level disable_ai setting for the current buffer
        if DisableAiSettings::is_ai_disabled_for_buffer(Some(&buffer), cx) {
            return None;
        }
        let offset_selection = selection.map(|endpoint| endpoint.to_offset(&multibuffer));

        let show_in_menu = self.show_edit_predictions_in_menu();
        let completions_menu_has_precedence = !show_in_menu
            && (self.context_menu.borrow().is_some()
                || (!self.completion_tasks.is_empty() && !self.has_active_edit_prediction()));

        if completions_menu_has_precedence
            || !offset_selection.is_empty()
            || self
                .active_edit_prediction
                .as_ref()
                .is_some_and(|completion| {
                    let Some(invalidation_range) = completion.invalidation_range.as_ref() else {
                        return false;
                    };
                    let invalidation_range = invalidation_range.to_offset(&multibuffer);
                    let invalidation_range = invalidation_range.start..=invalidation_range.end;
                    !invalidation_range.contains(&offset_selection.head())
                })
        {
            self.discard_edit_prediction(EditPredictionDiscardReason::Ignored, cx);
            return None;
        }

        self.take_active_edit_prediction(true, cx);
        let Some(provider) = self.edit_prediction_provider() else {
            self.edit_prediction_settings = EditPredictionSettings::Disabled;
            return None;
        };

        self.edit_prediction_settings =
            self.edit_prediction_settings_at_position(&buffer, cursor_text_anchor, cx);

        self.in_leading_whitespace = multibuffer.is_line_whitespace_upto(cursor);

        if self.in_leading_whitespace {
            let cursor_point = cursor.to_point(&multibuffer);
            let mut suggested_indent = None;
            multibuffer.suggested_indents_callback(
                cursor_point.row..cursor_point.row + 1,
                &mut |_, indent| {
                    suggested_indent = Some(indent);
                    ControlFlow::Break(())
                },
                cx,
            );

            if let Some(indent) = suggested_indent
                && indent.len == cursor_point.column
            {
                self.in_leading_whitespace = false;
            }
        }

        let edit_prediction = provider.suggest(&buffer, cursor_text_anchor, cx)?;

        let (completion_id, edits, predicted_cursor_position, edit_preview) = match edit_prediction
        {
            edit_prediction_types::EditPrediction::Local {
                id,
                edits,
                cursor_position,
                edit_preview,
            } => (id, edits, cursor_position, edit_preview),
            edit_prediction_types::EditPrediction::Jump {
                id,
                snapshot,
                target,
            } => {
                if let Some(provider) = &self.edit_prediction_provider {
                    provider.provider.did_show(SuggestionDisplayType::Jump, cx);
                }
                self.stale_edit_prediction_in_menu = None;
                self.active_edit_prediction = Some(EditPredictionState {
                    inlay_ids: vec![],
                    completion: EditPrediction::MoveOutside { snapshot, target },
                    completion_id: id,
                    invalidation_range: None,
                });
                cx.notify();
                return Some(());
            }
        };

        let edits = edits
            .into_iter()
            .flat_map(|(range, new_text)| {
                Some((
                    multibuffer.buffer_anchor_range_to_anchor_range(range)?,
                    new_text,
                ))
            })
            .collect::<Vec<_>>();
        if edits.is_empty() {
            return None;
        }

        let cursor_position = predicted_cursor_position.and_then(|predicted| {
            let anchor = multibuffer.anchor_in_excerpt(predicted.anchor)?;
            Some((anchor, predicted.offset))
        });

        let Some((first_edit_range, _)) = edits.first() else {
            return None;
        };
        let Some((last_edit_range, _)) = edits.last() else {
            return None;
        };

        let first_edit_start = first_edit_range.start;
        let first_edit_start_point = first_edit_start.to_point(&multibuffer);
        let edit_start_row = first_edit_start_point.row.saturating_sub(2);

        let last_edit_end = last_edit_range.end;
        let last_edit_end_point = last_edit_end.to_point(&multibuffer);
        let edit_end_row = cmp::min(multibuffer.max_point().row, last_edit_end_point.row + 2);

        let cursor_row = cursor.to_point(&multibuffer).row;

        let snapshot = multibuffer
            .buffer_for_id(cursor_text_anchor.buffer_id)
            .cloned()?;

        let mut inlay_ids = Vec::new();
        let invalidation_row_range;
        let move_invalidation_row_range = if cursor_row < edit_start_row {
            Some(cursor_row..edit_end_row)
        } else if cursor_row > edit_end_row {
            Some(edit_start_row..cursor_row)
        } else {
            None
        };
        let supports_jump = self
            .edit_prediction_provider
            .as_ref()
            .map(|provider| provider.provider.supports_jump_to_edit())
            .unwrap_or(true);

        let is_move = supports_jump
            && (move_invalidation_row_range.is_some() || self.edit_predictions_hidden_for_vim_mode);
        let completion = if is_move {
            if let Some(provider) = &self.edit_prediction_provider {
                provider.provider.did_show(SuggestionDisplayType::Jump, cx);
            }
            invalidation_row_range =
                move_invalidation_row_range.unwrap_or(edit_start_row..edit_end_row);

            let (_, snapshot) = multibuffer.anchor_to_buffer_anchor(first_edit_start)?;

            EditPrediction::MoveWithin {
                target: first_edit_start,
                snapshot: snapshot.clone(),
            }
        } else {
            let show_completions_in_menu = self.has_visible_completions_menu();
            let show_completions_in_buffer = !self.edit_prediction_visible_in_cursor_popover(true)
                && !self.edit_predictions_hidden_for_vim_mode;

            let display_mode = if all_edits_insertions_or_deletions(&edits, &multibuffer) {
                if provider.show_tab_accept_marker() {
                    EditDisplayMode::TabAccept
                } else {
                    EditDisplayMode::Inline
                }
            } else {
                EditDisplayMode::DiffPopover
            };

            let report_shown = match display_mode {
                EditDisplayMode::DiffPopover | EditDisplayMode::Inline => {
                    show_completions_in_buffer || show_completions_in_menu
                }
                EditDisplayMode::TabAccept => {
                    show_completions_in_menu || self.edit_prediction_preview_is_active()
                }
            };

            if report_shown && let Some(provider) = &self.edit_prediction_provider {
                let suggestion_display_type = match display_mode {
                    EditDisplayMode::DiffPopover => SuggestionDisplayType::DiffPopover,
                    EditDisplayMode::Inline | EditDisplayMode::TabAccept => {
                        SuggestionDisplayType::GhostText
                    }
                };
                provider.provider.did_show(suggestion_display_type, cx);
            }

            if show_completions_in_buffer {
                if edits
                    .iter()
                    .all(|(range, _)| range.to_offset(&multibuffer).is_empty())
                {
                    let mut inlays = Vec::new();
                    for (range, new_text) in &edits {
                        let inlay = Inlay::edit_prediction(
                            post_inc(&mut self.next_inlay_id),
                            range.start,
                            new_text.as_ref(),
                        );
                        inlay_ids.push(inlay.id);
                        inlays.push(inlay);
                    }

                    self.splice_inlays(&[], inlays, cx);
                } else {
                    let background_color = cx.theme().status().deleted_background;
                    self.highlight_text(
                        HighlightKey::EditPredictionHighlight,
                        edits.iter().map(|(range, _)| range.clone()).collect(),
                        HighlightStyle {
                            background_color: Some(background_color),
                            ..Default::default()
                        },
                        cx,
                    );
                }
            }

            invalidation_row_range = edit_start_row..edit_end_row;

            EditPrediction::Edit {
                edits,
                cursor_position,
                edit_preview,
                display_mode,
                snapshot,
            }
        };

        let invalidation_range = multibuffer
            .anchor_before(Point::new(invalidation_row_range.start, 0))
            ..multibuffer.anchor_after(Point::new(
                invalidation_row_range.end,
                multibuffer.line_len(MultiBufferRow(invalidation_row_range.end)),
            ));

        self.stale_edit_prediction_in_menu = None;
        self.active_edit_prediction = Some(EditPredictionState {
            inlay_ids,
            completion,
            completion_id,
            invalidation_range: Some(invalidation_range),
        });

        cx.notify();

        Some(())
    }
}
