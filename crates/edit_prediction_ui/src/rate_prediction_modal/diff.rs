use super::*;

impl RatePredictionsModal {
    fn update_buffer_diff(
        diff: &Entity<BufferDiff>,
        new_buffer_snapshot: BufferSnapshot,
        old_buffer_snapshot: BufferSnapshot,
        cx: &mut App,
    ) -> Task<()> {
        diff.update(cx, |diff, cx| {
            diff.set_base_text(
                Some(old_buffer_snapshot.text().into()),
                new_buffer_snapshot.text,
                cx,
            )
        })
    }

    fn insert_editable_region_markers(
        editor: &Entity<Editor>,
        buffer: &Entity<Buffer>,
        marker_range: Range<usize>,
        cx: &mut Context<Self>,
    ) {
        editor.update(cx, |editor, cx| {
            let buffer_snapshot = buffer.read(cx).snapshot();
            let multibuffer_snapshot = editor.buffer().read(cx).snapshot(cx);
            let start_buffer_anchor = buffer_snapshot
                .anchor_after(buffer_snapshot.clip_offset(marker_range.start, Bias::Left));
            let end_buffer_anchor = buffer_snapshot
                .anchor_after(buffer_snapshot.clip_offset(marker_range.end, Bias::Right));
            let Some(start_anchor) = multibuffer_snapshot.anchor_in_excerpt(start_buffer_anchor)
            else {
                return;
            };
            let Some(end_anchor) = multibuffer_snapshot.anchor_in_excerpt(end_buffer_anchor) else {
                return;
            };
            let Some((start_hint_position, _)) =
                multibuffer_snapshot.anchor_to_buffer_anchor(start_anchor)
            else {
                return;
            };
            let Some((end_hint_position, _)) =
                multibuffer_snapshot.anchor_to_buffer_anchor(end_anchor)
            else {
                return;
            };

            editor.splice_inlays(
                &[InlayId::Hint(0), InlayId::Hint(1)],
                vec![
                    Inlay::hint(
                        InlayId::Hint(0),
                        start_anchor,
                        &InlayHint {
                            position: start_hint_position,
                            label: InlayHintLabel::String("╭─ editable region start\n".into()),
                            kind: Some(InlayHintKind::Parameter),
                            padding_left: false,
                            padding_right: false,
                            tooltip: None,
                            resolve_state: ResolveState::Resolved,
                        },
                    ),
                    Inlay::hint(
                        InlayId::Hint(1),
                        end_anchor,
                        &InlayHint {
                            position: end_hint_position,
                            label: InlayHintLabel::String("\n╰─ editable region end".into()),
                            kind: Some(InlayHintKind::Parameter),
                            padding_left: false,
                            padding_right: false,
                            tooltip: None,
                            resolve_state: ResolveState::Resolved,
                        },
                    ),
                ],
                cx,
            );
        });
    }

    pub(super) fn expected_patch_for_active(&self, cx: &App) -> Option<String> {
        let active_prediction = self.active_prediction.as_ref()?;
        let expected_text = active_prediction.expected_buffer.read(cx).snapshot().text();
        let original_text = active_prediction.prediction.snapshot.text();
        let diff_body = language::unified_diff(&original_text, &expected_text);

        if diff_body.is_empty() {
            return None;
        }

        let path = active_prediction
            .prediction
            .snapshot
            .file()
            .map(|file| file.path().as_unix_str());
        let header = match path {
            Some(path) => format!("--- a/{path}\n+++ b/{path}\n"),
            None => String::new(),
        };

        Some(format!("{header}{diff_body}"))
    }

    pub fn select_completion(
        &mut self,
        prediction: Option<EditPrediction>,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Avoid resetting completion rating if it's already selected.
        if let Some(prediction) = prediction {
            self.selected_index = self
                .ep_store
                .read(cx)
                .rateable_predictions()
                .enumerate()
                .find(|(_, completion_b)| prediction.id == completion_b.id)
                .map(|(ix, _)| ix)
                .unwrap_or(self.selected_index);
            cx.notify();

            if let Some(prev_prediction) = self.active_prediction.as_ref()
                && prediction.id == prev_prediction.prediction.id
            {
                if focus {
                    window.focus(&prev_prediction.feedback_editor.focus_handle(cx), cx);
                }
                return;
            }

            let editable_range = prediction.editable_range.clone().or_else(|| {
                Some(prediction.edits.first()?.0.start..prediction.edits.last()?.0.end)
            });
            let predicted_buffer = prediction.edit_preview.build_result_buffer(cx);
            let predicted_buffer_snapshot = predicted_buffer.read(cx).snapshot();
            let visible_range = prediction
                .edit_preview
                .compute_visible_range(&prediction.edits)
                .or_else(|| {
                    editable_range.as_ref().map(|range| {
                        range.start.to_point(&prediction.snapshot)
                            ..range.end.to_point(&prediction.snapshot)
                    })
                })
                .unwrap_or(Point::zero()..Point::zero());
            let visible_range_with_context =
                Point::new(visible_range.start.row.saturating_sub(5), 0)
                    ..Point::new(visible_range.end.row.saturating_add(5), 0)
                        .min(predicted_buffer_snapshot.max_point());
            let predicted_diff_task = self.diff_editor.update(cx, |editor, cx| {
                let predicted_buffer_id = predicted_buffer_snapshot.remote_id();
                let diff = cx.new(|cx| {
                    BufferDiff::new(
                        &predicted_buffer_snapshot.text,
                        predicted_buffer_snapshot.language().cloned(),
                        predicted_buffer.read(cx).language_registry(),
                        cx,
                    )
                });
                let predicted_diff_task = Self::update_buffer_diff(
                    &diff,
                    predicted_buffer_snapshot.clone(),
                    prediction.snapshot.clone(),
                    cx,
                );

                editor.disable_header_for_buffer(predicted_buffer_id, cx);
                editor.buffer().update(cx, |multibuffer, cx| {
                    multibuffer.clear(cx);
                    multibuffer.set_excerpts_for_buffer(
                        predicted_buffer.clone(),
                        [visible_range_with_context],
                        0,
                        cx,
                    );
                    multibuffer.add_diff(diff, cx);
                });
                predicted_diff_task
            });

            if let Some(editable_range) = editable_range.as_ref() {
                Self::insert_editable_region_markers(
                    &self.diff_editor,
                    &predicted_buffer,
                    prediction
                        .edit_preview
                        .anchor_to_offset_in_result(editable_range.start)
                        ..prediction
                            .edit_preview
                            .anchor_to_offset_in_result(editable_range.end),
                    cx,
                );
            }

            self.diff_editor.update(cx, |editor, cx| {
                if let Some(cursor_position) = prediction.cursor_position.as_ref() {
                    let multibuffer_snapshot = editor.buffer().read(cx).snapshot(cx);
                    let cursor_offset = prediction
                        .edit_preview
                        .anchor_to_offset_in_result(cursor_position.anchor)
                        + cursor_position.offset;
                    let predicted_buffer_snapshot = predicted_buffer.read(cx).snapshot();
                    let cursor_anchor = predicted_buffer_snapshot.anchor_after(
                        predicted_buffer_snapshot.clip_offset(cursor_offset, Bias::Right),
                    );

                    if let Some(anchor) = multibuffer_snapshot.anchor_in_excerpt(cursor_anchor) {
                        editor.splice_inlays(
                            &[InlayId::EditPrediction(0)],
                            vec![Inlay::edit_prediction(0, anchor, "▏")],
                            cx,
                        );
                    }
                }
            });

            let mut formatted_inputs = String::new();
            Self::write_formatted_inputs(&mut formatted_inputs, &prediction.inputs);

            let current_editable_region = editable_range.as_ref().map(|range| {
                prediction
                    .buffer
                    .read(cx)
                    .snapshot()
                    .text_for_range(range.clone())
                    .collect::<String>()
            });
            let expected_buffer = cx.new(|cx| {
                let mut buffer = Buffer::local(prediction.snapshot.text(), cx);
                buffer.set_language_async(prediction.snapshot.language().cloned(), cx);
                buffer
            });
            let expected_editable_range = editable_range.as_ref().map(|editable_range| {
                expected_buffer.update(cx, |buffer, cx| {
                    let snapshot = buffer.snapshot();
                    let editable_point_range = editable_range.start.to_point(&prediction.snapshot)
                        ..editable_range.end.to_point(&prediction.snapshot);
                    let expected_editable_range = snapshot.anchor_before(editable_point_range.start)
                        ..snapshot.anchor_after(editable_point_range.end);
                    if let Some(current_editable_region) = current_editable_region {
                        buffer.edit(
                            [(expected_editable_range.clone(), current_editable_region)],
                            None,
                            cx,
                        );
                    }
                    expected_editable_range
                })
            });
            let expected_buffer_snapshot = expected_buffer.read(cx).snapshot();
            let expected_excerpt_range = expected_editable_range
                .as_ref()
                .map(|range| {
                    range.start.to_point(&expected_buffer_snapshot)
                        ..range.end.to_point(&expected_buffer_snapshot)
                })
                .unwrap_or(visible_range);
            let expected_diff = cx.new(|cx| {
                BufferDiff::new(
                    &expected_buffer_snapshot.text,
                    expected_buffer_snapshot.language().cloned(),
                    expected_buffer.read(cx).language_registry(),
                    cx,
                )
            });
            let expected_diff_task = Self::update_buffer_diff(
                &expected_diff,
                expected_buffer_snapshot.clone(),
                prediction.snapshot.clone(),
                cx,
            );
            let expected_editor = cx.new(|cx| {
                let multibuffer = cx.new(|cx| {
                    let mut multibuffer = MultiBuffer::new(language::Capability::ReadWrite);
                    multibuffer.set_excerpts_for_buffer(
                        expected_buffer.clone(),
                        [expected_excerpt_range],
                        0,
                        cx,
                    );
                    multibuffer.add_diff(expected_diff.clone(), cx);
                    multibuffer
                });
                let mut editor = Editor::for_multibuffer(multibuffer, None, window, cx);
                let expected_buffer_id = expected_buffer.read(cx).remote_id();
                editor.disable_header_for_buffer(expected_buffer_id, cx);
                editor.disable_inline_diagnostics();
                editor.set_expand_all_diff_hunks(cx);
                editor.set_show_git_diff_gutter(false, cx);
                editor.set_show_code_actions(false, cx);
                editor.set_show_runnables(false, cx);
                editor.set_show_bookmarks(false, cx);
                editor.set_show_breakpoints(false, cx);
                editor.set_show_wrap_guides(false, cx);
                editor.set_show_edit_predictions(Some(false), window, cx);
                editor
            });
            if let Some(expected_editable_range) = expected_editable_range.as_ref() {
                let expected_buffer_snapshot = expected_buffer.read(cx).snapshot();
                Self::insert_editable_region_markers(
                    &expected_editor,
                    &expected_buffer,
                    expected_editable_range
                        .start
                        .to_offset(&expected_buffer_snapshot)
                        ..expected_editable_range
                            .end
                            .to_offset(&expected_buffer_snapshot),
                    cx,
                );
            }

            let expected_buffer_subscription = cx.subscribe(&expected_buffer, {
                let expected_diff = expected_diff.clone();
                let original_snapshot = prediction.snapshot.clone();
                move |this, buffer, event, cx| match event {
                    language::BufferEvent::Edited { .. }
                    | language::BufferEvent::LanguageChanged(_)
                    | language::BufferEvent::Reparsed => {
                        let task = Self::update_buffer_diff(
                            &expected_diff,
                            buffer.read(cx).snapshot(),
                            original_snapshot.clone(),
                            cx,
                        );
                        if let Some(active_prediction) = this.active_prediction.as_mut() {
                            active_prediction.expected_diff_task = task;
                        }
                    }
                    _ => {}
                }
            });

            self.active_prediction = Some(ActivePrediction {
                prediction,
                feedback_editor: cx.new(|cx| {
                    let mut editor = Editor::multi_line(window, cx);
                    editor.disable_scrollbars_and_minimap(window, cx);
                    editor.set_soft_wrap_mode(language_settings::SoftWrap::EditorWidth, cx);
                    editor.set_show_line_numbers(false, cx);
                    editor.set_show_git_diff_gutter(false, cx);
                    editor.set_show_code_actions(false, cx);
                    editor.set_show_runnables(false, cx);
                    editor.set_show_bookmarks(false, cx);
                    editor.set_show_breakpoints(false, cx);
                    editor.set_show_wrap_guides(false, cx);
                    editor.set_show_indent_guides(false, cx);
                    editor.set_show_edit_predictions(Some(false), window, cx);
                    editor.set_placeholder_text("Add your feedback…", window, cx);
                    editor.set_completion_provider(Some(Rc::new(FeedbackCompletionProvider)));
                    if focus {
                        cx.focus_self(window);
                    }
                    editor
                }),
                expected_buffer,
                expected_editor,
                _expected_buffer_subscription: expected_buffer_subscription,
                _predicted_diff_task: predicted_diff_task,
                expected_diff_task,
                formatted_inputs: cx.new(|cx| {
                    Markdown::new(
                        formatted_inputs.into(),
                        Some(self.language_registry.clone()),
                        None,
                        cx,
                    )
                }),
            });
        } else {
            self.active_prediction = None;
        }

        cx.notify();
    }
}
