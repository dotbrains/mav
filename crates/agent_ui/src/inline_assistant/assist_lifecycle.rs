use super::*;

impl InlineAssistant {
    pub fn finish_assist(
        &mut self,
        assist_id: InlineAssistId,
        undo: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(assist) = self.assists.get(&assist_id) {
            let assist_group_id = assist.group_id;
            if self.assist_groups[&assist_group_id].linked {
                for assist_id in self.unlink_assist_group(assist_group_id, window, cx) {
                    self.finish_assist(assist_id, undo, window, cx);
                }
                return;
            }
        }

        self.dismiss_assist(assist_id, window, cx);

        if let Some(assist) = self.assists.remove(&assist_id) {
            if let hash_map::Entry::Occupied(mut entry) = self.assist_groups.entry(assist.group_id)
            {
                entry.get_mut().assist_ids.retain(|id| *id != assist_id);
                if entry.get().assist_ids.is_empty() {
                    entry.remove();
                }
            }

            if let hash_map::Entry::Occupied(mut entry) =
                self.assists_by_editor.entry(assist.editor.clone())
            {
                entry.get_mut().assist_ids.retain(|id| *id != assist_id);
                if entry.get().assist_ids.is_empty() {
                    entry.remove();
                    if let Some(editor) = assist.editor.upgrade() {
                        self.update_editor_highlights(&editor, cx);
                    }
                } else {
                    entry.get_mut().highlight_updates.send(()).ok();
                }
            }

            let active_alternative = assist.codegen.read(cx).active_alternative().clone();
            if let Some(model) = LanguageModelRegistry::read_global(cx).inline_assistant_model() {
                let language_name = assist.editor.upgrade().and_then(|editor| {
                    let multibuffer = editor.read(cx).buffer().read(cx);
                    let snapshot = multibuffer.snapshot(cx);
                    let ranges =
                        snapshot.range_to_buffer_ranges(assist.range.start..assist.range.end);
                    ranges
                        .first()
                        .and_then(|(buffer, _, _)| buffer.language())
                        .map(|language| language.name().0.to_string())
                });

                let codegen = assist.codegen.read(cx);
                let session_id = codegen.session_id();
                let message_id = active_alternative.read(cx).message_id.clone();
                let model_telemetry_id = model.model.telemetry_id();
                let model_provider_id = model.model.provider_id().to_string();

                let (phase, event_type, anthropic_event_type) = if undo {
                    (
                        "rejected",
                        "Assistant Response Rejected",
                        AnthropicEventType::Reject,
                    )
                } else {
                    (
                        "accepted",
                        "Assistant Response Accepted",
                        AnthropicEventType::Accept,
                    )
                };

                telemetry::event!(
                    event_type,
                    phase,
                    session_id = session_id.to_string(),
                    kind = "inline",
                    model = model_telemetry_id,
                    model_provider = model_provider_id,
                    language_name = language_name,
                    message_id = message_id.as_deref(),
                );

                report_anthropic_event(
                    &model.model,
                    AnthropicEventData {
                        completion_type: AnthropicCompletionType::Editor,
                        event: anthropic_event_type,
                        language_name,
                        message_id,
                    },
                    cx,
                );
            }

            if undo {
                assist.codegen.update(cx, |codegen, cx| codegen.undo(cx));
            } else {
                self.confirmed_assists.insert(assist_id, active_alternative);
            }
        }
    }

    pub(super) fn dismiss_assist(
        &mut self,
        assist_id: InlineAssistId,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        let Some(assist) = self.assists.get_mut(&assist_id) else {
            return false;
        };
        let Some(editor) = assist.editor.upgrade() else {
            return false;
        };
        let Some(decorations) = assist.decorations.take() else {
            return false;
        };

        editor.update(cx, |editor, cx| {
            let mut to_remove = decorations.removed_line_block_ids;
            to_remove.insert(decorations.prompt_block_id);
            to_remove.insert(decorations.end_block_id);
            if let Some(tool_description_block_id) = decorations.model_explanation {
                to_remove.insert(tool_description_block_id);
            }
            editor.remove_blocks(to_remove, None, cx);
        });

        if decorations
            .prompt_editor
            .focus_handle(cx)
            .contains_focused(window, cx)
        {
            self.focus_next_assist(assist_id, window, cx);
        }

        if let Some(editor_assists) = self.assists_by_editor.get_mut(&editor.downgrade()) {
            if editor_assists
                .scroll_lock
                .as_ref()
                .is_some_and(|lock| lock.assist_id == assist_id)
            {
                editor_assists.scroll_lock = None;
            }
            editor_assists.highlight_updates.send(()).ok();
        }

        true
    }

    pub(super) fn focus_next_assist(
        &mut self,
        assist_id: InlineAssistId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(assist) = self.assists.get(&assist_id) else {
            return;
        };

        let assist_group = &self.assist_groups[&assist.group_id];
        let assist_ix = assist_group
            .assist_ids
            .iter()
            .position(|id| *id == assist_id)
            .unwrap();
        let assist_ids = assist_group
            .assist_ids
            .iter()
            .skip(assist_ix + 1)
            .chain(assist_group.assist_ids.iter().take(assist_ix));

        for assist_id in assist_ids {
            let assist = &self.assists[assist_id];
            if assist.decorations.is_some() {
                self.focus_assist(*assist_id, window, cx);
                return;
            }
        }

        assist
            .editor
            .update(cx, |editor, cx| window.focus(&editor.focus_handle(cx), cx))
            .ok();
    }

    pub(super) fn focus_assist(
        &mut self,
        assist_id: InlineAssistId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(assist) = self.assists.get(&assist_id) else {
            return;
        };

        if let Some(decorations) = assist.decorations.as_ref() {
            decorations.prompt_editor.update(cx, |prompt_editor, cx| {
                prompt_editor.editor.update(cx, |editor, cx| {
                    window.focus(&editor.focus_handle(cx), cx);
                    editor.select_all(&SelectAll, window, cx);
                })
            });
        }

        self.scroll_to_assist(assist_id, window, cx);
    }

    pub fn scroll_to_assist(
        &mut self,
        assist_id: InlineAssistId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(assist) = self.assists.get(&assist_id) else {
            return;
        };
        let Some(editor) = assist.editor.upgrade() else {
            return;
        };

        let position = assist.range.start;
        editor.update(cx, |editor, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                selections.select_anchor_ranges([position..position])
            });

            let mut scroll_target_range = None;
            if let Some(decorations) = assist.decorations.as_ref() {
                scroll_target_range = maybe!({
                    let top = editor.row_for_block(decorations.prompt_block_id, cx)?.0 as f64;
                    let bottom = editor.row_for_block(decorations.end_block_id, cx)?.0 as f64;
                    Some((top, bottom))
                });
                if scroll_target_range.is_none() {
                    log::error!("bug: failed to find blocks for scrolling to inline assist");
                }
            }
            let scroll_target_range = scroll_target_range.unwrap_or_else(|| {
                let snapshot = editor.snapshot(window, cx);
                let start_row = assist
                    .range
                    .start
                    .to_display_point(&snapshot.display_snapshot)
                    .row();
                let top = start_row.0 as ScrollOffset;
                let bottom = top + 1.0;
                (top, bottom)
            });
            let height_in_lines = editor.visible_line_count().unwrap_or(0.);
            let vertical_scroll_margin = editor.vertical_scroll_margin() as ScrollOffset;
            let scroll_target_top = (scroll_target_range.0 - vertical_scroll_margin)
                // Don't scroll up too far in the case of a large vertical_scroll_margin.
                .max(scroll_target_range.0 - height_in_lines / 2.0);
            let scroll_target_bottom = (scroll_target_range.1 + vertical_scroll_margin)
                // Don't scroll down past where the top would still be visible.
                .min(scroll_target_top + height_in_lines);

            let scroll_top = editor.scroll_position(cx).y;
            let scroll_bottom = scroll_top + height_in_lines;

            if scroll_target_top < scroll_top {
                editor.set_scroll_position(point(0., scroll_target_top), window, cx);
            } else if scroll_target_bottom > scroll_bottom {
                editor.set_scroll_position(
                    point(0., scroll_target_bottom - height_in_lines),
                    window,
                    cx,
                );
            }
        });
    }

    pub(super) fn unlink_assist_group(
        &mut self,
        assist_group_id: InlineAssistGroupId,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<InlineAssistId> {
        let assist_group = self.assist_groups.get_mut(&assist_group_id).unwrap();
        assist_group.linked = false;

        for assist_id in &assist_group.assist_ids {
            let assist = self.assists.get_mut(assist_id).unwrap();
            if let Some(editor_decorations) = assist.decorations.as_ref() {
                editor_decorations
                    .prompt_editor
                    .update(cx, |prompt_editor, cx| prompt_editor.unlink(window, cx));
            }
        }
        assist_group.assist_ids.clone()
    }

    pub fn start_assist(&mut self, assist_id: InlineAssistId, window: &mut Window, cx: &mut App) {
        let assist = if let Some(assist) = self.assists.get_mut(&assist_id) {
            assist
        } else {
            return;
        };

        let assist_group_id = assist.group_id;
        if self.assist_groups[&assist_group_id].linked {
            for assist_id in self.unlink_assist_group(assist_group_id, window, cx) {
                self.start_assist(assist_id, window, cx);
            }
            return;
        }

        let Some((user_prompt, mention_set)) = assist.user_prompt(cx).zip(assist.mention_set(cx))
        else {
            return;
        };

        self.prompt_history.retain(|prompt| *prompt != user_prompt);
        self.prompt_history.push_back(user_prompt.clone());
        if self.prompt_history.len() > PROMPT_HISTORY_MAX_LEN {
            self.prompt_history.pop_front();
        }

        let Some(ConfiguredModel { model, .. }) =
            LanguageModelRegistry::read_global(cx).inline_assistant_model()
        else {
            return;
        };

        let context_task = load_context(&mention_set, cx).shared();
        assist
            .codegen
            .update(cx, |codegen, cx| {
                codegen.start(model, user_prompt, context_task, cx)
            })
            .log_err();
    }

    pub fn stop_assist(&mut self, assist_id: InlineAssistId, cx: &mut App) {
        let assist = if let Some(assist) = self.assists.get_mut(&assist_id) {
            assist
        } else {
            return;
        };

        assist.codegen.update(cx, |codegen, cx| codegen.stop(cx));
    }
}
