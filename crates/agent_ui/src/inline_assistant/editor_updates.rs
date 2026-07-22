use super::*;

impl InlineAssistant {
    pub(super) fn update_editor_highlights(&self, editor: &Entity<Editor>, cx: &mut App) {
        let mut gutter_pending_ranges = Vec::new();
        let mut gutter_transformed_ranges = Vec::new();
        let mut foreground_ranges = Vec::new();
        let mut inserted_row_ranges = Vec::new();
        let empty_assist_ids = Vec::new();
        let assist_ids = self
            .assists_by_editor
            .get(&editor.downgrade())
            .map_or(&empty_assist_ids, |editor_assists| {
                &editor_assists.assist_ids
            });

        for assist_id in assist_ids {
            if let Some(assist) = self.assists.get(assist_id) {
                let codegen = assist.codegen.read(cx);
                let buffer = codegen.buffer(cx).read(cx).read(cx);
                foreground_ranges.extend(codegen.last_equal_ranges(cx).iter().cloned());

                let pending_range =
                    codegen.edit_position(cx).unwrap_or(assist.range.start)..assist.range.end;
                if pending_range.end.to_offset(&buffer) > pending_range.start.to_offset(&buffer) {
                    gutter_pending_ranges.push(pending_range);
                }

                if let Some(edit_position) = codegen.edit_position(cx) {
                    let edited_range = assist.range.start..edit_position;
                    if edited_range.end.to_offset(&buffer) > edited_range.start.to_offset(&buffer) {
                        gutter_transformed_ranges.push(edited_range);
                    }
                }

                if assist.decorations.is_some() {
                    inserted_row_ranges
                        .extend(codegen.diff(cx).inserted_row_ranges.iter().cloned());
                }
            }
        }

        let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
        merge_ranges(&mut foreground_ranges, &snapshot);
        merge_ranges(&mut gutter_pending_ranges, &snapshot);
        merge_ranges(&mut gutter_transformed_ranges, &snapshot);
        editor.update(cx, |editor, cx| {
            enum GutterPendingRange {}
            if gutter_pending_ranges.is_empty() {
                editor.clear_gutter_highlights::<GutterPendingRange>(cx);
            } else {
                editor.highlight_gutter::<GutterPendingRange>(
                    gutter_pending_ranges,
                    |cx| cx.theme().status().info_background,
                    cx,
                )
            }

            enum GutterTransformedRange {}
            if gutter_transformed_ranges.is_empty() {
                editor.clear_gutter_highlights::<GutterTransformedRange>(cx);
            } else {
                editor.highlight_gutter::<GutterTransformedRange>(
                    gutter_transformed_ranges,
                    |cx| cx.theme().status().info,
                    cx,
                )
            }

            if foreground_ranges.is_empty() {
                editor.clear_highlights(HighlightKey::InlineAssist, cx);
            } else {
                editor.highlight_text(
                    HighlightKey::InlineAssist,
                    foreground_ranges,
                    HighlightStyle {
                        fade_out: Some(0.6),
                        ..Default::default()
                    },
                    cx,
                );
            }

            editor.clear_row_highlights::<InlineAssist>();
            for row_range in inserted_row_ranges {
                editor.highlight_rows::<InlineAssist>(
                    row_range,
                    |cx| cx.theme().status().info_background,
                    Default::default(),
                    cx,
                );
            }
        });
    }

    pub(super) fn update_editor_blocks(
        &mut self,
        editor: &Entity<Editor>,
        assist_id: InlineAssistId,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(assist) = self.assists.get_mut(&assist_id) else {
            return;
        };
        let Some(decorations) = assist.decorations.as_mut() else {
            return;
        };

        let codegen = assist.codegen.read(cx);
        let old_snapshot = codegen.snapshot(cx);
        let old_buffer = codegen.old_buffer(cx);
        let deleted_row_ranges = codegen.diff(cx).deleted_row_ranges.clone();

        editor.update(cx, |editor, cx| {
            let old_blocks = mem::take(&mut decorations.removed_line_block_ids);
            editor.remove_blocks(old_blocks, None, cx);

            let mut new_blocks = Vec::new();
            for (new_row, old_row_range) in deleted_row_ranges {
                let (_, start) = old_snapshot
                    .point_to_buffer_point(Point::new(*old_row_range.start(), 0))
                    .unwrap();
                let (_, end) = old_snapshot
                    .point_to_buffer_point(Point::new(
                        *old_row_range.end(),
                        old_snapshot.line_len(MultiBufferRow(*old_row_range.end())),
                    ))
                    .unwrap();

                let deleted_lines_editor = cx.new(|cx| {
                    let multi_buffer =
                        cx.new(|_| MultiBuffer::without_headers(language::Capability::ReadOnly));
                    multi_buffer.update(cx, |multi_buffer, cx| {
                        multi_buffer.set_excerpts_for_buffer(
                            old_buffer.clone(),
                            // todo(lw): start and end might come from different snapshots!
                            [start..end],
                            0,
                            cx,
                        );
                    });

                    enum DeletedLines {}
                    let mut editor = Editor::for_multibuffer(multi_buffer, None, window, cx);
                    editor.disable_scrollbars_and_minimap(window, cx);
                    editor.set_soft_wrap_mode(language::language_settings::SoftWrap::None, cx);
                    editor.set_show_wrap_guides(false, cx);
                    editor.set_show_gutter(false, cx);
                    editor.set_offset_content(false, cx);
                    editor.disable_mouse_wheel_zoom();
                    editor.set_forbid_vertical_scroll(true);
                    editor.set_read_only(true);
                    editor.set_show_edit_predictions(Some(false), window, cx);
                    editor.highlight_rows::<DeletedLines>(
                        Anchor::Min..Anchor::Max,
                        |cx| cx.theme().status().deleted_background,
                        Default::default(),
                        cx,
                    );
                    editor
                });

                let height =
                    deleted_lines_editor.update(cx, |editor, cx| editor.max_point(cx).row().0 + 1);
                new_blocks.push(BlockProperties {
                    placement: BlockPlacement::Above(new_row),
                    height: Some(height),
                    style: BlockStyle::Flex,
                    render: Arc::new(move |cx| {
                        div()
                            .block_mouse_except_scroll()
                            .bg(cx.theme().status().deleted_background)
                            .size_full()
                            .h(height as f32 * cx.window.line_height())
                            .pl(cx.margins.gutter.full_width())
                            .child(deleted_lines_editor.clone())
                            .into_any_element()
                    }),
                    priority: 0,
                });
            }

            decorations.removed_line_block_ids = editor
                .insert_blocks(new_blocks, None, cx)
                .into_iter()
                .collect();
        })
    }

    pub(super) fn resolve_inline_assist_target(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineAssistTarget> {
        if let Some(terminal_panel) = workspace.panel::<TerminalPanel>(cx)
            && terminal_panel
                .read(cx)
                .focus_handle(cx)
                .contains_focused(window, cx)
            && let Some(terminal_view) = terminal_panel.read(cx).pane().and_then(|pane| {
                pane.read(cx)
                    .active_item()
                    .and_then(|t| t.downcast::<TerminalView>())
            })
        {
            return Some(InlineAssistTarget::Terminal(terminal_view));
        }

        if let Some(agent_panel) = workspace.panel::<AgentPanel>(cx)
            && let Some(terminal_view) = agent_panel.read(cx).visible_terminal_view().cloned()
            && terminal_view.focus_handle(cx).contains_focused(window, cx)
        {
            return Some(InlineAssistTarget::Terminal(terminal_view));
        }

        if let Some(workspace_editor) = workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
        {
            Some(InlineAssistTarget::Editor(workspace_editor))
        } else {
            workspace
                .active_item(cx)
                .and_then(|item| item.act_as::<TerminalView>(cx))
                .map(InlineAssistTarget::Terminal)
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_completion_receiver(
        &mut self,
        sender: mpsc::UnboundedSender<anyhow::Result<InlineAssistId>>,
    ) {
        self._inline_assistant_completions = Some(sender);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn get_codegen(
        &mut self,
        assist_id: InlineAssistId,
        cx: &mut App,
    ) -> Option<Entity<CodegenAlternative>> {
        self.assists.get(&assist_id).map(|inline_assist| {
            inline_assist
                .codegen
                .update(cx, |codegen, _cx| codegen.active_alternative().clone())
        })
    }
}
