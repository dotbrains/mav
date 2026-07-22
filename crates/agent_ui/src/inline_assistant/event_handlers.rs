use super::*;

impl InlineAssistant {
    pub(super) fn handle_prompt_editor_focus_in(
        &mut self,
        assist_id: InlineAssistId,
        cx: &mut App,
    ) {
        let assist = &self.assists[&assist_id];
        let Some(decorations) = assist.decorations.as_ref() else {
            return;
        };
        let assist_group = self.assist_groups.get_mut(&assist.group_id).unwrap();
        let editor_assists = self.assists_by_editor.get_mut(&assist.editor).unwrap();

        assist_group.active_assist_id = Some(assist_id);
        if assist_group.linked {
            for assist_id in &assist_group.assist_ids {
                if let Some(decorations) = self.assists[assist_id].decorations.as_ref() {
                    decorations.prompt_editor.update(cx, |prompt_editor, cx| {
                        prompt_editor.set_show_cursor_when_unfocused(true, cx)
                    });
                }
            }
        }

        assist
            .editor
            .update(cx, |editor, cx| {
                let scroll_top = editor.scroll_position(cx).y;
                let scroll_bottom = scroll_top + editor.visible_line_count().unwrap_or(0.);
                editor_assists.scroll_lock = editor
                    .row_for_block(decorations.prompt_block_id, cx)
                    .map(|row| row.as_f64())
                    .filter(|prompt_row| (scroll_top..scroll_bottom).contains(&prompt_row))
                    .map(|prompt_row| InlineAssistScrollLock {
                        assist_id,
                        distance_from_top: prompt_row - scroll_top,
                    });
            })
            .ok();
    }

    pub(super) fn handle_prompt_editor_focus_out(
        &mut self,
        assist_id: InlineAssistId,
        cx: &mut App,
    ) {
        let assist = &self.assists[&assist_id];
        let assist_group = self.assist_groups.get_mut(&assist.group_id).unwrap();
        if assist_group.active_assist_id == Some(assist_id) {
            assist_group.active_assist_id = None;
            if assist_group.linked {
                for assist_id in &assist_group.assist_ids {
                    if let Some(decorations) = self.assists[assist_id].decorations.as_ref() {
                        decorations.prompt_editor.update(cx, |prompt_editor, cx| {
                            prompt_editor.set_show_cursor_when_unfocused(false, cx)
                        });
                    }
                }
            }
        }
    }

    pub(super) fn handle_prompt_editor_event(
        &mut self,
        prompt_editor: Entity<PromptEditor<BufferCodegen>>,
        event: &PromptEditorEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let assist_id = prompt_editor.read(cx).id();
        match event {
            PromptEditorEvent::StartRequested => {
                self.start_assist(assist_id, window, cx);
            }
            PromptEditorEvent::StopRequested => {
                self.stop_assist(assist_id, cx);
            }
            PromptEditorEvent::ConfirmRequested { execute: _ } => {
                self.finish_assist(assist_id, false, window, cx);
            }
            PromptEditorEvent::CancelRequested => {
                self.finish_assist(assist_id, true, window, cx);
            }
            PromptEditorEvent::Resized { .. } => {
                // This only matters for the terminal inline assistant
            }
        }
    }

    pub(super) fn handle_editor_newline(
        &mut self,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(editor_assists) = self.assists_by_editor.get(&editor.downgrade()) else {
            return;
        };

        if editor.read(cx).selections.count() == 1 {
            let (selection, buffer) = editor.update(cx, |editor, cx| {
                (
                    editor
                        .selections
                        .newest::<MultiBufferOffset>(&editor.display_snapshot(cx)),
                    editor.buffer().read(cx).snapshot(cx),
                )
            });
            for assist_id in &editor_assists.assist_ids {
                let assist = &self.assists[assist_id];
                let assist_range = assist.range.to_offset(&buffer);
                if assist_range.contains(&selection.start) && assist_range.contains(&selection.end)
                {
                    if matches!(assist.codegen.read(cx).status(cx), CodegenStatus::Pending) {
                        self.dismiss_assist(*assist_id, window, cx);
                    } else {
                        self.finish_assist(*assist_id, false, window, cx);
                    }

                    return;
                }
            }
        }

        cx.propagate();
    }

    pub(super) fn handle_editor_cancel(
        &mut self,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(editor_assists) = self.assists_by_editor.get(&editor.downgrade()) else {
            return;
        };

        if editor.read(cx).selections.count() == 1 {
            let (selection, buffer) = editor.update(cx, |editor, cx| {
                (
                    editor
                        .selections
                        .newest::<MultiBufferOffset>(&editor.display_snapshot(cx)),
                    editor.buffer().read(cx).snapshot(cx),
                )
            });
            let mut closest_assist_fallback = None;
            for assist_id in &editor_assists.assist_ids {
                let assist = &self.assists[assist_id];
                let assist_range = assist.range.to_offset(&buffer);
                if assist.decorations.is_some() {
                    if assist_range.contains(&selection.start)
                        && assist_range.contains(&selection.end)
                    {
                        self.focus_assist(*assist_id, window, cx);
                        return;
                    } else {
                        let distance_from_selection = assist_range
                            .start
                            .0
                            .abs_diff(selection.start.0)
                            .min(assist_range.start.0.abs_diff(selection.end.0))
                            + assist_range
                                .end
                                .0
                                .abs_diff(selection.start.0)
                                .min(assist_range.end.0.abs_diff(selection.end.0));
                        match closest_assist_fallback {
                            Some((_, old_distance)) => {
                                if distance_from_selection < old_distance {
                                    closest_assist_fallback =
                                        Some((assist_id, distance_from_selection));
                                }
                            }
                            None => {
                                closest_assist_fallback = Some((assist_id, distance_from_selection))
                            }
                        }
                    }
                }
            }

            if let Some((&assist_id, _)) = closest_assist_fallback {
                self.focus_assist(assist_id, window, cx);
            }
        }

        cx.propagate();
    }

    pub(super) fn handle_editor_release(
        &mut self,
        editor: WeakEntity<Editor>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(editor_assists) = self.assists_by_editor.get_mut(&editor) {
            for assist_id in editor_assists.assist_ids.clone() {
                self.finish_assist(assist_id, true, window, cx);
            }
        }
    }

    pub(super) fn handle_editor_change(
        &mut self,
        editor: Entity<Editor>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(editor_assists) = self.assists_by_editor.get(&editor.downgrade()) else {
            return;
        };
        let Some(scroll_lock) = editor_assists.scroll_lock.as_ref() else {
            return;
        };
        let assist = &self.assists[&scroll_lock.assist_id];
        let Some(decorations) = assist.decorations.as_ref() else {
            return;
        };

        editor.update(cx, |editor, cx| {
            let scroll_position = editor.scroll_position(cx);
            let target_scroll_top = editor
                .row_for_block(decorations.prompt_block_id, cx)?
                .as_f64()
                - scroll_lock.distance_from_top;
            if target_scroll_top != scroll_position.y {
                editor.set_scroll_position(point(scroll_position.x, target_scroll_top), window, cx);
            }
            Some(())
        });
    }

    pub(super) fn handle_editor_event(
        &mut self,
        editor: Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(editor_assists) = self.assists_by_editor.get_mut(&editor.downgrade()) else {
            return;
        };

        match event {
            EditorEvent::Edited { transaction_id } => {
                let buffer = editor.read(cx).buffer().read(cx);
                let edited_ranges = buffer.edited_ranges_for_transaction(*transaction_id, cx);
                let snapshot = buffer.snapshot(cx);

                for assist_id in editor_assists.assist_ids.clone() {
                    let assist = &self.assists[&assist_id];
                    if matches!(
                        assist.codegen.read(cx).status(cx),
                        CodegenStatus::Error(_) | CodegenStatus::Done
                    ) {
                        let assist_range = assist.range.to_offset(&snapshot);
                        if edited_ranges
                            .iter()
                            .any(|range| range.overlaps(&assist_range))
                        {
                            self.finish_assist(assist_id, false, window, cx);
                        }
                    }
                }
            }
            EditorEvent::ScrollPositionChanged { .. } => {
                if let Some(scroll_lock) = editor_assists.scroll_lock.as_ref() {
                    let assist = &self.assists[&scroll_lock.assist_id];
                    if let Some(decorations) = assist.decorations.as_ref() {
                        let distance_from_top = editor.update(cx, |editor, cx| {
                            let scroll_top = editor.scroll_position(cx).y;
                            let prompt_row = editor
                                .row_for_block(decorations.prompt_block_id, cx)?
                                .0 as ScrollOffset;
                            Some(prompt_row - scroll_top)
                        });

                        if distance_from_top.is_none_or(|distance_from_top| {
                            distance_from_top != scroll_lock.distance_from_top
                        }) {
                            editor_assists.scroll_lock = None;
                        }
                    }
                }
            }
            EditorEvent::SelectionsChanged { .. } => {
                for assist_id in editor_assists.assist_ids.clone() {
                    let assist = &self.assists[&assist_id];
                    if let Some(decorations) = assist.decorations.as_ref()
                        && decorations
                            .prompt_editor
                            .focus_handle(cx)
                            .is_focused(window)
                    {
                        return;
                    }
                }

                editor_assists.scroll_lock = None;
            }
            _ => {}
        }
    }
}
