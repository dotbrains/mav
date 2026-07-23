use super::*;

impl MessageEditor {
    pub fn set_read_only(&mut self, read_only: bool, cx: &mut Context<Self>) {
        self.editor.update(cx, |message_editor, cx| {
            message_editor.set_read_only(read_only);
            cx.notify()
        })
    }

    pub fn set_mode(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            if *editor.mode() != mode {
                editor.set_mode(mode);
                cx.notify()
            }
        });
    }

    pub fn set_message(
        &mut self,
        message: Vec<acp::ContentBlock>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.clear(window, cx);
        self.insert_message_blocks(message, false, window, cx);
    }

    pub fn append_message(
        &mut self,
        message: Vec<acp::ContentBlock>,
        separator: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if message.is_empty() {
            return;
        }

        if let Some(separator) = separator
            && !separator.is_empty()
            && !self.is_empty(cx)
        {
            self.editor.update(cx, |editor, cx| {
                editor.insert(separator, window, cx);
            });
        }

        self.insert_message_blocks(message, true, window, cx);
    }

    fn insert_message_blocks(
        &mut self,
        message: Vec<acp::ContentBlock>,
        append_to_existing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project.upgrade() else {
            return;
        };

        let path_style = project.read(cx).path_style(cx);
        let mut text = String::new();
        let mut mentions = Vec::new();
        let append_normalized = |text: &mut String, mut segment: String| {
            LineEnding::normalize(&mut segment);
            text.push_str(&segment);
        };

        for chunk in message {
            match chunk {
                acp::ContentBlock::Text(text_content) => {
                    append_normalized(&mut text, text_content.text);
                }
                acp::ContentBlock::Resource(acp::EmbeddedResource {
                    resource: acp::EmbeddedResourceResource::TextResourceContents(resource),
                    ..
                }) => {
                    let Some(mention_uri) = MentionUri::parse(&resource.uri, path_style).log_err()
                    else {
                        continue;
                    };
                    let start = text.len();
                    append_normalized(&mut text, mention_uri.as_link().to_string());
                    let end = text.len();
                    mentions.push((
                        start..end,
                        mention_uri,
                        Mention::Text {
                            content: resource.text,
                            tracked_buffers: Vec::new(),
                        },
                    ));
                }
                acp::ContentBlock::ResourceLink(resource) => {
                    if let Some(mention_uri) =
                        MentionUri::parse(&resource.uri, path_style).log_err()
                    {
                        let start = text.len();
                        append_normalized(&mut text, mention_uri.as_link().to_string());
                        let end = text.len();
                        mentions.push((start..end, mention_uri, Mention::Link));
                    }
                }
                acp::ContentBlock::Image(acp::ImageContent {
                    uri,
                    data,
                    mime_type,
                    ..
                }) => {
                    let mention_uri = if let Some(uri) = uri {
                        MentionUri::parse(&uri, path_style)
                    } else {
                        Ok(MentionUri::PastedImage {
                            name: "Image".to_string(),
                        })
                    };
                    let Some(mention_uri) = mention_uri.log_err() else {
                        continue;
                    };
                    let Some(format) = ImageFormat::from_mime_type(&mime_type) else {
                        log::error!("failed to parse MIME type for image: {mime_type:?}");
                        continue;
                    };
                    let start = text.len();
                    append_normalized(&mut text, mention_uri.as_link().to_string());
                    let end = text.len();
                    mentions.push((
                        start..end,
                        mention_uri,
                        Mention::Image(MentionImage {
                            data: data.into(),
                            format,
                        }),
                    ));
                }
                _ => {}
            }
        }

        if text.is_empty() && mentions.is_empty() {
            return;
        }

        let insertion_start = if append_to_existing {
            self.editor.read(cx).text(cx).len()
        } else {
            0
        };

        let snapshot = if append_to_existing {
            self.editor.update(cx, |editor, cx| {
                editor.insert(&text, window, cx);
                editor.buffer().read(cx).snapshot(cx)
            })
        } else {
            self.editor.update(cx, |editor, cx| {
                editor.set_text(text, window, cx);
                editor.buffer().read(cx).snapshot(cx)
            })
        };

        for (range, mention_uri, mention) in mentions {
            let adjusted_start = insertion_start + range.start;
            let anchor = snapshot.anchor_before(MultiBufferOffset(adjusted_start));
            let image_preview = image_preview_task_for_mention(&mention);
            let Some((crease_id, tx, crease_entity)) = insert_crease_for_mention(
                snapshot.anchor_to_buffer_anchor(anchor).unwrap().0,
                range.end - range.start,
                mention_uri.name().into(),
                mention_uri.icon_path(cx),
                mention_uri.tooltip_text(),
                Some(mention_uri.clone()),
                Some(self.workspace.clone()),
                image_preview,
                self.editor.clone(),
                window,
                cx,
            ) else {
                continue;
            };
            drop(tx);

            self.mention_set.update(cx, |mention_set, cx| {
                mention_set.insert_mention(
                    crease_id,
                    mention_uri.clone(),
                    Task::ready(Ok(mention)).shared(),
                    crease_entity,
                    cx,
                )
            });
        }

        cx.notify();
    }

    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    pub fn set_cursor_offset(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            let offset = snapshot.clip_offset(MultiBufferOffset(offset), text::Bias::Left);
            editor.change_selections(Default::default(), window, cx, |selections| {
                selections.select_ranges([offset..offset]);
            });
        });
    }

    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }

        self.editor.update(cx, |editor, cx| {
            editor.insert(text, window, cx);
        });
    }

    pub fn set_placeholder_text(
        &mut self,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.set_placeholder_text(placeholder, window, cx);
        });
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_text(text, window, cx);
        });
    }

    fn serialize_selection_with_mentions(
        &self,
        expand_empty_to_line: bool,
        cx: &mut App,
    ) -> Option<(String, Vec<Range<MultiBufferOffset>>)> {
        if self.mention_set.read(cx).is_empty() {
            return None;
        }

        let display_snapshot = self
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));
        let editor = self.editor.read(cx);
        if !expand_empty_to_line && !editor.has_non_empty_selection(&display_snapshot) {
            return None;
        }

        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let mention_set = self.mention_set.read(cx);
        let mention_ranges = display_snapshot
            .crease_snapshot
            .crease_items_with_offsets(&snapshot)
            .into_iter()
            .filter_map(|(crease_id, range)| {
                mention_set.mention_uri_for_crease(&crease_id).map(|uri| {
                    (
                        range.start.to_offset(&snapshot),
                        range.end.to_offset(&snapshot),
                        uri,
                    )
                })
            })
            .collect::<Vec<_>>();

        let line_mode = editor.selections.line_mode();
        let max_point = snapshot.max_point();
        let point_selections = editor.selections.all::<Point>(&display_snapshot);

        let mut text = String::new();
        let mut ranges = Vec::with_capacity(point_selections.len());
        let mut has_mentions = false;
        let mut is_first = true;
        let mut prev_was_entire_line = false;

        for mut selection in point_selections {
            let is_entire_line = (selection.is_empty() && expand_empty_to_line) || line_mode;
            if is_entire_line {
                selection.start = Point::new(selection.start.row, 0);
                if !selection.is_empty() && selection.end.column == 0 {
                    selection.end = min(max_point, selection.end);
                } else {
                    selection.end = min(max_point, Point::new(selection.end.row + 1, 0));
                }
            }
            let range = selection.start.to_offset(&snapshot)..selection.end.to_offset(&snapshot);

            if is_first {
                is_first = false;
            } else if !prev_was_entire_line {
                text.push('\n');
            }
            prev_was_entire_line = is_entire_line;

            let mut cursor = range.start;
            for (start, end, uri) in mention_ranges
                .iter()
                .filter(|(start, end, _)| *start < range.end && range.start < *end)
            {
                if cursor < *start {
                    text.extend(snapshot.text_for_range(cursor..*start));
                }
                write!(text, "{}", uri.as_link()).unwrap();
                cursor = *end;
                has_mentions = true;
            }
            if cursor < range.end {
                text.extend(snapshot.text_for_range(cursor..range.end));
            }

            ranges.push(range);
        }

        has_mentions.then_some((text, ranges))
    }
}
