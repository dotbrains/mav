use super::*;
use language::ToOffset as _;

impl Editor {
    pub fn rename(
        &mut self,
        _: &Rename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if self.read_only(cx) {
            return None;
        }
        let provider = self.semantics_provider.clone()?;
        let selection = self.selections.newest_anchor().clone();
        let (cursor_buffer, cursor_buffer_position) = self
            .buffer
            .read(cx)
            .text_anchor_for_position(selection.head(), cx)?;
        let (tail_buffer, cursor_buffer_position_end) = self
            .buffer
            .read(cx)
            .text_anchor_for_position(selection.tail(), cx)?;
        if tail_buffer != cursor_buffer {
            return None;
        }

        let snapshot = cursor_buffer.read(cx).snapshot();
        let cursor_buffer_offset = cursor_buffer_position.to_offset(&snapshot);
        let cursor_buffer_offset_end = cursor_buffer_position_end.to_offset(&snapshot);
        let prepare_rename = provider.range_for_rename(&cursor_buffer, cursor_buffer_position, cx);
        drop(snapshot);

        Some(cx.spawn_in(window, async move |this, cx| {
            let rename_range = prepare_rename.await?;
            if let Some(rename_range) = rename_range {
                this.update_in(cx, |this, window, cx| {
                    let snapshot = cursor_buffer.read(cx).snapshot();
                    let rename_buffer_range = rename_range.to_offset(&snapshot);
                    let cursor_offset_in_rename_range =
                        cursor_buffer_offset.saturating_sub(rename_buffer_range.start);
                    let cursor_offset_in_rename_range_end =
                        cursor_buffer_offset_end.saturating_sub(rename_buffer_range.start);

                    this.take_rename(false, window, cx);
                    let buffer = this.buffer.read(cx).read(cx);
                    let cursor_offset = selection.head().to_offset(&buffer);
                    let rename_start =
                        cursor_offset.saturating_sub_usize(cursor_offset_in_rename_range);
                    let rename_end = rename_start + rename_buffer_range.len();
                    let range = buffer.anchor_before(rename_start)..buffer.anchor_after(rename_end);
                    let mut old_highlight_id = None;
                    let old_name: Arc<str> = buffer
                        .chunks(
                            rename_start..rename_end,
                            LanguageAwareStyling {
                                tree_sitter: true,
                                diagnostics: true,
                            },
                        )
                        .map(|chunk| {
                            if old_highlight_id.is_none() {
                                old_highlight_id = chunk.syntax_highlight_id;
                            }
                            chunk.text
                        })
                        .collect::<String>()
                        .into();

                    drop(buffer);

                    // Position the selection in the rename editor so that it matches the current selection.
                    this.show_local_selections = false;
                    let rename_editor = cx.new(|cx| {
                        let mut editor = Editor::single_line(window, cx);
                        editor.buffer.update(cx, |buffer, cx| {
                            buffer.edit(
                                [(MultiBufferOffset(0)..MultiBufferOffset(0), old_name.clone())],
                                None,
                                cx,
                            )
                        });
                        let cursor_offset_in_rename_range =
                            MultiBufferOffset(cursor_offset_in_rename_range);
                        let cursor_offset_in_rename_range_end =
                            MultiBufferOffset(cursor_offset_in_rename_range_end);
                        let rename_selection_range = match cursor_offset_in_rename_range
                            .cmp(&cursor_offset_in_rename_range_end)
                        {
                            Ordering::Equal => {
                                editor.select_all(&SelectAll, window, cx);
                                return editor;
                            }
                            Ordering::Less => {
                                cursor_offset_in_rename_range..cursor_offset_in_rename_range_end
                            }
                            Ordering::Greater => {
                                cursor_offset_in_rename_range_end..cursor_offset_in_rename_range
                            }
                        };
                        if rename_selection_range.end.0 > old_name.len() {
                            editor.select_all(&SelectAll, window, cx);
                        } else {
                            editor.change_selections(Default::default(), window, cx, |s| {
                                s.select_ranges([rename_selection_range]);
                            });
                        }
                        editor
                    });
                    cx.subscribe(&rename_editor, |_, _, e: &EditorEvent, cx| {
                        if e == &EditorEvent::Focused {
                            cx.emit(EditorEvent::FocusedIn)
                        }
                    })
                    .detach();

                    let write_highlights =
                        this.clear_background_highlights(HighlightKey::DocumentHighlightWrite, cx);
                    let read_highlights =
                        this.clear_background_highlights(HighlightKey::DocumentHighlightRead, cx);
                    let ranges = write_highlights
                        .iter()
                        .flat_map(|(_, ranges)| ranges.iter())
                        .chain(read_highlights.iter().flat_map(|(_, ranges)| ranges.iter()))
                        .cloned()
                        .collect();

                    this.highlight_text(
                        HighlightKey::Rename,
                        ranges,
                        HighlightStyle {
                            fade_out: Some(0.6),
                            ..Default::default()
                        },
                        cx,
                    );
                    let rename_focus_handle = rename_editor.focus_handle(cx);
                    window.focus(&rename_focus_handle, cx);
                    let block_id = this.insert_blocks(
                        [BlockProperties {
                            style: BlockStyle::Flex,
                            placement: BlockPlacement::Below(range.start),
                            height: Some(1),
                            render: Arc::new({
                                let rename_editor = rename_editor.clone();
                                move |cx: &mut BlockContext| {
                                    let mut text_style = cx.editor_style.text.clone();
                                    if let Some(highlight_style) = old_highlight_id
                                        .and_then(|h| cx.editor_style.syntax.get(h).cloned())
                                    {
                                        text_style = text_style.highlight(highlight_style);
                                    }
                                    div()
                                        .block_mouse_except_scroll()
                                        .pl(cx.anchor_x)
                                        .child(EditorElement::new(
                                            &rename_editor,
                                            EditorStyle {
                                                background: cx.theme().system().transparent,
                                                local_player: cx.editor_style.local_player,
                                                text: text_style,
                                                scrollbar_width: cx.editor_style.scrollbar_width,
                                                syntax: cx.editor_style.syntax.clone(),
                                                status: cx.editor_style.status.clone(),
                                                inlay_hints_style: HighlightStyle {
                                                    font_weight: Some(FontWeight::BOLD),
                                                    ..make_inlay_hints_style(cx.app)
                                                },
                                                edit_prediction_styles: make_suggestion_styles(
                                                    cx.app,
                                                ),
                                                ..EditorStyle::default()
                                            },
                                        ))
                                        .into_any_element()
                                }
                            }),
                            priority: 0,
                        }],
                        Some(Autoscroll::fit()),
                        cx,
                    )[0];
                    this.pending_rename = Some(RenameState {
                        range,
                        old_name,
                        editor: rename_editor,
                        block_id,
                    });
                })?;
            }

            Ok(())
        }))
    }

    pub fn confirm_rename(
        &mut self,
        _: &ConfirmRename,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<Result<()>>> {
        if self.read_only(cx) {
            return None;
        }
        let rename = self.take_rename(false, window, cx)?;
        let workspace = self.workspace()?.downgrade();
        let (buffer, start) = self
            .buffer
            .read(cx)
            .text_anchor_for_position(rename.range.start, cx)?;
        let (end_buffer, _) = self
            .buffer
            .read(cx)
            .text_anchor_for_position(rename.range.end, cx)?;
        if buffer != end_buffer {
            return None;
        }

        let old_name = rename.old_name;
        let new_name = rename.editor.read(cx).text(cx);

        let rename = self.semantics_provider.as_ref()?.perform_rename(
            &buffer,
            start,
            new_name.clone(),
            cx,
        )?;

        Some(cx.spawn_in(window, async move |editor, cx| {
            let project_transaction = rename.await?;
            Self::open_project_transaction(
                &editor,
                workspace,
                project_transaction,
                format!("Rename: {} → {}", old_name, new_name),
                cx,
            )
            .await?;

            editor.update(cx, |editor, cx| {
                editor.refresh_document_highlights(cx);
            })?;
            Ok(())
        }))
    }

    pub(crate) fn take_rename(
        &mut self,
        moving_cursor: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<RenameState> {
        let rename = self.pending_rename.take()?;
        if rename.editor.focus_handle(cx).is_focused(window) {
            window.focus(&self.focus_handle, cx);
        }

        self.remove_blocks(
            [rename.block_id].into_iter().collect(),
            Some(Autoscroll::fit()),
            cx,
        );
        self.clear_highlights(HighlightKey::Rename, cx);
        self.show_local_selections = true;

        if moving_cursor {
            let cursor_in_rename_editor = rename.editor.update(cx, |editor, cx| {
                editor
                    .selections
                    .newest::<MultiBufferOffset>(&editor.display_snapshot(cx))
                    .head()
            });

            // Update the selection to match the position of the selection inside
            // the rename editor.
            let snapshot = self.buffer.read(cx).read(cx);
            let rename_range = rename.range.to_offset(&snapshot);
            let cursor_in_editor = snapshot
                .clip_offset(rename_range.start + cursor_in_rename_editor, Bias::Left)
                .min(rename_range.end);
            drop(snapshot);

            self.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges(vec![cursor_in_editor..cursor_in_editor])
            });
        } else {
            self.refresh_document_highlights(cx);
        }

        Some(rename)
    }

    pub fn pending_rename(&self) -> Option<&RenameState> {
        self.pending_rename.as_ref()
    }
}
