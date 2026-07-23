use super::*;

impl MessageEditor {
    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };

        if self.editor.read(cx).read_only(cx) {
            let editor = self.editor.read(cx);
            let cursor_offset = editor
                .selections
                .newest_anchor()
                .head()
                .to_offset(&editor.buffer().read(cx).snapshot(cx))
                .0;
            cx.emit(MessageEditorEvent::InputAttempted {
                attempt: InputAttempt::Paste(clipboard),
                cursor_offset,
            });
            cx.stop_propagation();
            return;
        }

        cx.stop_propagation();
        self.paste_item(&clipboard, window, cx);
    }

    pub fn paste_item(
        &mut self,
        clipboard: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let editor_clipboard_selections =
            clipboard.entries().iter().find_map(|entry| match entry {
                ClipboardEntry::String(text) => {
                    text.metadata_json::<Vec<editor::ClipboardSelection>>()
                }
                _ => None,
            });

        // Insert creases for pasted clipboard selections that:
        // 1. Contain exactly one selection
        // 2. Have an associated file path
        // 3. Span multiple lines (not single-line selections)
        // 4. Belong to a file that exists in the current project
        let should_insert_creases = util::maybe!({
            let selections = editor_clipboard_selections.as_ref()?;
            if selections.len() > 1 {
                return Some(false);
            }
            let selection = selections.first()?;
            let file_path = selection.file_path.as_ref()?;
            let line_range = selection.line_range.as_ref()?;

            if line_range.start() == line_range.end() {
                return Some(false);
            }

            Some(
                workspace
                    .read(cx)
                    .project()
                    .read(cx)
                    .project_path_for_absolute_path(file_path, cx)
                    .is_some(),
            )
        })
        .unwrap_or(false);

        if should_insert_creases && let Some(selections) = editor_clipboard_selections {
            let snapshot = self.editor.read(cx).buffer().read(cx).snapshot(cx);
            let (insertion_target, _) = snapshot
                .anchor_to_buffer_anchor(self.editor.read(cx).selections.newest_anchor().start)
                .unwrap();

            let project = workspace.read(cx).project().clone();
            for selection in selections {
                if let (Some(file_path), Some(line_range)) =
                    (selection.file_path, selection.line_range)
                {
                    let crease_text =
                        acp_thread::selection_name(Some(file_path.as_ref()), &line_range);

                    let mention_uri = MentionUri::Selection {
                        abs_path: Some(file_path.clone()),
                        line_range: line_range.clone(),
                        column: None,
                    };

                    let mention_text = mention_uri.as_link().to_string();
                    let (text_anchor, content_len) = self.editor.update(cx, |editor, cx| {
                        let buffer = editor.buffer().read(cx);
                        let snapshot = buffer.snapshot(cx);
                        let buffer_snapshot = snapshot.as_singleton().unwrap();
                        let text_anchor = insertion_target.bias_left(&buffer_snapshot);

                        editor.insert(&mention_text, window, cx);
                        editor.insert(" ", window, cx);

                        (text_anchor, mention_text.len())
                    });

                    let Some((crease_id, tx, crease_entity)) = insert_crease_for_mention(
                        text_anchor,
                        content_len,
                        crease_text.into(),
                        mention_uri.icon_path(cx),
                        mention_uri.tooltip_text(),
                        Some(mention_uri.clone()),
                        Some(self.workspace.clone()),
                        None,
                        self.editor.clone(),
                        window,
                        cx,
                    ) else {
                        continue;
                    };
                    drop(tx);

                    let mention_task = cx
                        .spawn({
                            let project = project.clone();
                            async move |_, cx| {
                                let project_path = project
                                    .update(cx, |project, cx| {
                                        project.project_path_for_absolute_path(&file_path, cx)
                                    })
                                    .ok_or_else(|| "project path not found".to_string())?;

                                let buffer = project
                                    .update(cx, |project, cx| project.open_buffer(project_path, cx))
                                    .await
                                    .map_err(|e| e.to_string())?;

                                Ok(buffer.update(cx, |buffer, cx| {
                                    let start =
                                        Point::new(*line_range.start(), 0).min(buffer.max_point());
                                    let end = Point::new(*line_range.end() + 1, 0)
                                        .min(buffer.max_point());
                                    let content = buffer.text_for_range(start..end).collect();
                                    Mention::Text {
                                        content,
                                        tracked_buffers: vec![cx.entity()],
                                    }
                                }))
                            }
                        })
                        .shared();

                    self.mention_set.update(cx, |mention_set, cx| {
                        mention_set.insert_mention(
                            crease_id,
                            mention_uri.clone(),
                            mention_task,
                            crease_entity,
                            cx,
                        )
                    });
                }
            }
            return;
        }
        // Handle text paste with potential markdown mention links before
        // clipboard context entries so markdown text still pastes as text.
        let clipboard_text = clipboard.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(text) => Some(text.text().to_string()),
            _ => None,
        });
        if let Some(clipboard_text) = clipboard_text.as_deref() {
            if clipboard_text.contains("[@") {
                let selections_before = self.editor.update(cx, |editor, cx| {
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    editor
                        .selections
                        .disjoint_anchors()
                        .iter()
                        .map(|selection| {
                            (
                                selection.start.bias_left(&snapshot),
                                selection.end.bias_right(&snapshot),
                            )
                        })
                        .collect::<Vec<_>>()
                });

                self.editor.update(cx, |editor, cx| {
                    editor.insert(clipboard_text, window, cx);
                });

                let snapshot = self.editor.read(cx).buffer().read(cx).snapshot(cx);
                let path_style = workspace.read(cx).project().read(cx).path_style(cx);

                let mut all_mentions = Vec::new();
                for (start_anchor, end_anchor) in selections_before {
                    let start_offset = start_anchor.to_offset(&snapshot);
                    let end_offset = end_anchor.to_offset(&snapshot);

                    // Get the actual inserted text from the buffer (may differ due to auto-indent)
                    let inserted_text: String =
                        snapshot.text_for_range(start_offset..end_offset).collect();

                    let parsed_mentions = parse_mention_links(&inserted_text, path_style);
                    for (range, mention_uri) in parsed_mentions {
                        let mention_start_offset = MultiBufferOffset(start_offset.0 + range.start);
                        let anchor = snapshot.anchor_before(mention_start_offset);
                        let content_len = range.end - range.start;
                        all_mentions.push((anchor, content_len, mention_uri));
                    }
                }

                if !all_mentions.is_empty() {
                    let supports_images = self.session_capabilities.read().supports_images();
                    let http_client = workspace.read(cx).client().http_client();

                    for (anchor, content_len, mention_uri) in all_mentions {
                        let Some((crease_id, tx, crease_entity)) = insert_crease_for_mention(
                            snapshot.anchor_to_buffer_anchor(anchor).unwrap().0,
                            content_len,
                            mention_uri.name().into(),
                            mention_uri.icon_path(cx),
                            mention_uri.tooltip_text(),
                            Some(mention_uri.clone()),
                            Some(self.workspace.clone()),
                            None,
                            self.editor.clone(),
                            window,
                            cx,
                        ) else {
                            continue;
                        };

                        // Create the confirmation task based on the mention URI type.
                        // This properly loads file content, fetches URLs, etc.
                        let task = self.mention_set.update(cx, |mention_set, cx| {
                            mention_set.confirm_mention_for_uri(
                                mention_uri.clone(),
                                supports_images,
                                http_client.clone(),
                                cx,
                            )
                        });
                        let task = cx
                            .spawn(async move |_, _| task.await.map_err(|e| e.to_string()))
                            .shared();

                        self.mention_set.update(cx, |mention_set, cx| {
                            mention_set.insert_mention(
                                crease_id,
                                mention_uri.clone(),
                                task.clone(),
                                crease_entity,
                                cx,
                            )
                        });

                        // Drop the tx after inserting to signal the crease is ready
                        drop(tx);
                    }
                    return;
                }
            }
        }

        if self.handle_pasted_context(clipboard, window, cx) {
            return;
        }

        self.editor.update(cx, |editor, cx| {
            editor.paste_item(clipboard, window, cx);
        });
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        let Some((text, _)) = self.serialize_selection_with_mentions(false, cx) else {
            cx.propagate();
            return;
        };

        cx.stop_propagation();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let Some((text, ranges)) = self.serialize_selection_with_mentions(true, cx) else {
            cx.propagate();
            return;
        };

        cx.stop_propagation();
        self.editor.update(cx, |editor, cx| {
            editor.transact(window, cx, |editor, window, cx| {
                editor.change_selections(Default::default(), window, cx, |selections| {
                    selections.select_ranges(ranges);
                });
                editor.insert("", window, cx);
            });
        });
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn paste_raw(&mut self, _: &PasteRaw, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.editor.clone();
        window.defer(cx, move |window, cx| {
            editor.update(cx, |editor, cx| editor.paste(&Paste, window, cx));
        });
    }

    fn handle_pasted_context(
        &mut self,
        clipboard: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if matches!(
            clipboard.entries().first(),
            Some(ClipboardEntry::String(_)) | None
        ) {
            return false;
        }

        let Some(workspace) = self.workspace.upgrade() else {
            return false;
        };
        let project = workspace.read(cx).project().clone();
        let project_is_local = project.read(cx).is_local();
        let supports_images = self.session_capabilities.read().supports_images();
        if !project_is_local && !supports_images {
            return false;
        }
        let editor = self.editor.clone();
        let mention_set = self.mention_set.clone();
        let workspace = self.workspace.clone();
        let entries = clipboard.clone().into_entries().collect::<Vec<_>>();

        window
            .spawn(cx, async move |mut cx| {
                let (items, added_worktrees) = resolve_pasted_context_items(
                    project,
                    project_is_local,
                    supports_images,
                    entries,
                    &mut cx,
                )
                .await;
                insert_resolved_pasted_context_items(
                    items,
                    added_worktrees,
                    editor,
                    mention_set,
                    workspace,
                    supports_images,
                    &mut cx,
                )
                .await;
                Ok::<(), anyhow::Error>(())
            })
            .detach_and_log_err(cx);

        true
    }
}
