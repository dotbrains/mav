use super::*;

impl InlineAssistant {
    pub(super) fn codegen_ranges(
        &mut self,
        editor: &Entity<Editor>,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(Vec<Range<Anchor>>, Selection<Point>)> {
        let (initial_selections, newest_selection) = editor.update(cx, |editor, _| {
            (
                editor.selections.all::<Point>(&snapshot.display_snapshot),
                editor
                    .selections
                    .newest::<Point>(&snapshot.display_snapshot),
            )
        });

        // Check if there is already an inline assistant that contains the
        // newest selection, if there is, focus it
        if let Some(editor_assists) = self.assists_by_editor.get(&editor.downgrade()) {
            for assist_id in &editor_assists.assist_ids {
                let assist = &self.assists[assist_id];
                let range = assist.range.to_point(&snapshot.buffer_snapshot());
                if range.start.row <= newest_selection.start.row
                    && newest_selection.end.row <= range.end.row
                {
                    self.focus_assist(*assist_id, window, cx);
                    return None;
                }
            }
        }

        let mut selections = Vec::<Selection<Point>>::new();
        let mut newest_selection = None;
        for mut selection in initial_selections {
            if selection.end == selection.start
                && let Some(fold) =
                    snapshot.crease_for_buffer_row(MultiBufferRow(selection.end.row))
            {
                selection.start = fold.range().start;
                selection.end = fold.range().end;
                if MultiBufferRow(selection.end.row) < snapshot.buffer_snapshot().max_row() {
                    let chars = snapshot
                        .buffer_snapshot()
                        .chars_at(Point::new(selection.end.row + 1, 0));

                    for c in chars {
                        if c == '\n' {
                            break;
                        }
                        if c.is_whitespace() {
                            continue;
                        }
                        if snapshot
                            .language_at(selection.end)
                            .is_some_and(|language| language.config().brackets.is_closing_brace(c))
                        {
                            selection.end.row += 1;
                            selection.end.column = snapshot
                                .buffer_snapshot()
                                .line_len(MultiBufferRow(selection.end.row));
                        }
                    }
                }
            } else {
                selection.start.column = 0;
                // If the selection ends at the start of the line, we don't want to include it.
                if selection.end.column == 0 && selection.start.row != selection.end.row {
                    selection.end.row -= 1;
                }
                selection.end.column = snapshot
                    .buffer_snapshot()
                    .line_len(MultiBufferRow(selection.end.row));
            }

            if let Some(prev_selection) = selections.last_mut()
                && selection.start <= prev_selection.end
            {
                prev_selection.end = selection.end;
                continue;
            }

            let latest_selection = newest_selection.get_or_insert_with(|| selection.clone());
            if selection.id > latest_selection.id {
                *latest_selection = selection.clone();
            }
            selections.push(selection);
        }
        let snapshot = &snapshot.buffer_snapshot();
        let newest_selection = newest_selection.unwrap();

        let mut codegen_ranges = Vec::new();
        for (buffer, buffer_range, _) in selections
            .iter()
            .flat_map(|selection| snapshot.range_to_buffer_ranges(selection.start..selection.end))
        {
            let (Some(start), Some(end)) = (
                snapshot.anchor_in_buffer(buffer.anchor_before(buffer_range.start)),
                snapshot.anchor_in_buffer(buffer.anchor_after(buffer_range.end)),
            ) else {
                continue;
            };
            let anchor_range = start..end;

            codegen_ranges.push(anchor_range);

            if let Some(model) = LanguageModelRegistry::read_global(cx).inline_assistant_model() {
                telemetry::event!(
                    "Assistant Invoked",
                    kind = "inline",
                    phase = "invoked",
                    model = model.model.telemetry_id(),
                    model_provider = model.provider.id().to_string(),
                    language_name = buffer.language().map(|language| language.name().to_proto())
                );

                report_anthropic_event(
                    &model.model,
                    AnthropicEventData {
                        completion_type: AnthropicCompletionType::Editor,
                        event: AnthropicEventType::Invoked,
                        language_name: buffer.language().map(|language| language.name().to_proto()),
                        message_id: None,
                    },
                    cx,
                );
            }
        }

        Some((codegen_ranges, newest_selection))
    }

    pub(super) fn batch_assist(
        &mut self,
        editor: &Entity<Editor>,
        workspace: WeakEntity<Workspace>,
        project: WeakEntity<Project>,
        thread_store: Entity<ThreadStore>,
        initial_prompt: Option<String>,
        window: &mut Window,
        codegen_ranges: &[Range<Anchor>],
        newest_selection: Option<Selection<Point>>,
        initial_transaction_id: Option<TransactionId>,
        cx: &mut App,
    ) -> Option<InlineAssistId> {
        let snapshot = editor.update(cx, |editor, cx| editor.snapshot(window, cx));

        let assist_group_id = self.next_assist_group_id.post_inc();
        let session_id = Uuid::new_v4();
        let prompt_buffer = cx.new(|cx| {
            MultiBuffer::singleton(
                cx.new(|cx| Buffer::local(initial_prompt.unwrap_or_default(), cx)),
                cx,
            )
        });

        let mut assists = Vec::new();
        let mut assist_to_focus = None;

        for range in codegen_ranges {
            let assist_id = self.next_assist_id.post_inc();
            let codegen = cx.new(|cx| {
                BufferCodegen::new(
                    editor.read(cx).buffer().clone(),
                    range.clone(),
                    initial_transaction_id,
                    session_id,
                    self.prompt_builder.clone(),
                    cx,
                )
            });

            let editor_margins = Arc::new(Mutex::new(EditorMargins::default()));
            let prompt_editor = cx.new(|cx| {
                PromptEditor::new_buffer(
                    assist_id,
                    editor_margins,
                    self.prompt_history.clone(),
                    prompt_buffer.clone(),
                    codegen.clone(),
                    session_id,
                    self.fs.clone(),
                    thread_store.clone(),
                    project.clone(),
                    workspace.clone(),
                    window,
                    cx,
                )
            });

            if let Some(newest_selection) = newest_selection.as_ref()
                && assist_to_focus.is_none()
            {
                let focus_assist = if newest_selection.reversed {
                    range.start.to_point(&snapshot) == newest_selection.start
                } else {
                    range.end.to_point(&snapshot) == newest_selection.end
                };
                if focus_assist {
                    assist_to_focus = Some(assist_id);
                }
            }

            let [prompt_block_id, tool_description_block_id, end_block_id] =
                self.insert_assist_blocks(&editor, &range, &prompt_editor, cx);

            assists.push((
                assist_id,
                range.clone(),
                prompt_editor,
                prompt_block_id,
                tool_description_block_id,
                end_block_id,
            ));
        }

        let editor_assists = self
            .assists_by_editor
            .entry(editor.downgrade())
            .or_insert_with(|| EditorInlineAssists::new(editor, window, cx));

        let assist_to_focus = if let Some(focus_id) = assist_to_focus {
            Some(focus_id)
        } else if assists.len() >= 1 {
            Some(assists[0].0)
        } else {
            None
        };

        let mut assist_group = InlineAssistGroup::new();
        for (
            assist_id,
            range,
            prompt_editor,
            prompt_block_id,
            tool_description_block_id,
            end_block_id,
        ) in assists
        {
            let codegen = prompt_editor.read(cx).codegen().clone();

            self.assists.insert(
                assist_id,
                InlineAssist::new(
                    assist_id,
                    assist_group_id,
                    editor,
                    &prompt_editor,
                    prompt_block_id,
                    tool_description_block_id,
                    end_block_id,
                    range,
                    codegen,
                    workspace.clone(),
                    window,
                    cx,
                ),
            );
            assist_group.assist_ids.push(assist_id);
            editor_assists.assist_ids.push(assist_id);
        }

        self.assist_groups.insert(assist_group_id, assist_group);

        assist_to_focus
    }

    pub fn assist(
        &mut self,
        editor: &Entity<Editor>,
        workspace: WeakEntity<Workspace>,
        project: WeakEntity<Project>,
        thread_store: Entity<ThreadStore>,
        initial_prompt: Option<String>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<InlineAssistId> {
        let snapshot = editor.update(cx, |editor, cx| editor.snapshot(window, cx));

        let Some((codegen_ranges, newest_selection)) =
            self.codegen_ranges(editor, &snapshot, window, cx)
        else {
            return None;
        };

        let assist_to_focus = self.batch_assist(
            editor,
            workspace,
            project,
            thread_store,
            initial_prompt,
            window,
            &codegen_ranges,
            Some(newest_selection),
            None,
            cx,
        );

        if let Some(assist_id) = assist_to_focus {
            self.focus_assist(assist_id, window, cx);
        }

        assist_to_focus
    }

    pub(super) fn insert_assist_blocks(
        &self,
        editor: &Entity<Editor>,
        range: &Range<Anchor>,
        prompt_editor: &Entity<PromptEditor<BufferCodegen>>,
        cx: &mut App,
    ) -> [CustomBlockId; 3] {
        let prompt_editor_height = prompt_editor.update(cx, |prompt_editor, cx| {
            prompt_editor
                .editor
                .update(cx, |editor, cx| editor.max_point(cx).row().0 + 1 + 2)
        });
        let assist_blocks = vec![
            BlockProperties {
                style: BlockStyle::Sticky,
                placement: BlockPlacement::Above(range.start),
                height: Some(prompt_editor_height),
                render: build_assist_editor_renderer(prompt_editor),
                priority: 0,
            },
            // Placeholder for tool description - will be updated dynamically
            BlockProperties {
                style: BlockStyle::Flex,
                placement: BlockPlacement::Below(range.end),
                height: Some(0),
                render: Arc::new(|_cx| div().into_any_element()),
                priority: 0,
            },
            BlockProperties {
                style: BlockStyle::Sticky,
                placement: BlockPlacement::Below(range.end),
                height: None,
                render: Arc::new(|cx| {
                    v_flex()
                        .h_full()
                        .w_full()
                        .border_t_1()
                        .border_color(cx.theme().status().info_border)
                        .into_any_element()
                }),
                priority: 0,
            },
        ];

        editor.update(cx, |editor, cx| {
            let block_ids = editor.insert_blocks(assist_blocks, None, cx);
            [block_ids[0], block_ids[1], block_ids[2]]
        })
    }
}
