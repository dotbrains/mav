use super::*;

impl Editor {
    pub fn do_paste(
        &mut self,
        text: &String,
        clipboard_selections: Option<Vec<ClipboardSelection>>,
        handle_entire_lines: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }

        self.finalize_last_transaction(cx);

        let clipboard_text = Cow::Borrowed(text.as_str());

        self.transact(window, cx, |this, window, cx| {
            let had_active_edit_prediction = this.has_active_edit_prediction();
            let display_map = this.display_snapshot(cx);
            let old_selections = this.selections.all::<MultiBufferOffset>(&display_map);
            let cursor_offset = this
                .selections
                .last::<MultiBufferOffset>(&display_map)
                .head();

            if let Some(mut clipboard_selections) = clipboard_selections {
                let all_selections_were_entire_line =
                    clipboard_selections.iter().all(|s| s.is_entire_line);
                let first_selection_indent_column =
                    clipboard_selections.first().map(|s| s.first_line_indent);
                if clipboard_selections.len() != old_selections.len() {
                    clipboard_selections.drain(..);
                }
                let mut auto_indent_on_paste = true;

                this.buffer.update(cx, |buffer, cx| {
                    let snapshot = buffer.read(cx);
                    auto_indent_on_paste = snapshot
                        .language_settings_at(cursor_offset, cx)
                        .auto_indent_on_paste;

                    let mut start_offset = 0;
                    let mut edits = Vec::new();
                    let mut original_indent_columns = Vec::new();
                    for (ix, selection) in old_selections.iter().enumerate() {
                        let to_insert;
                        let entire_line;
                        let original_indent_column;
                        if let Some(clipboard_selection) = clipboard_selections.get(ix) {
                            let end_offset = start_offset + clipboard_selection.len;
                            to_insert = &clipboard_text[start_offset..end_offset];
                            entire_line = clipboard_selection.is_entire_line;
                            start_offset = if entire_line {
                                end_offset
                            } else {
                                end_offset + 1
                            };
                            original_indent_column = Some(clipboard_selection.first_line_indent);
                        } else {
                            to_insert = &*clipboard_text;
                            entire_line = all_selections_were_entire_line;
                            original_indent_column = first_selection_indent_column
                        }

                        let (range, to_insert) =
                            if selection.is_empty() && handle_entire_lines && entire_line {
                                let column = selection.start.to_point(&snapshot).column as usize;
                                let line_start = selection.start - column;
                                (line_start..line_start, Cow::Borrowed(to_insert))
                            } else {
                                let language = snapshot.language_at(selection.head());
                                let range = selection.range();
                                if let Some(language) = language
                                    && language.name() == "Markdown"
                                {
                                    edit_for_markdown_paste(
                                        &snapshot,
                                        range,
                                        to_insert,
                                        is_standalone_url(to_insert),
                                    )
                                } else {
                                    (range, Cow::Borrowed(to_insert))
                                }
                            };

                        edits.push((range, to_insert));
                        original_indent_columns.push(original_indent_column);
                    }
                    drop(snapshot);

                    buffer.edit(
                        edits,
                        if auto_indent_on_paste {
                            Some(AutoindentMode::Block {
                                original_indent_columns,
                            })
                        } else {
                            None
                        },
                        cx,
                    );
                });

                let selections = this
                    .selections
                    .all::<MultiBufferOffset>(&this.display_snapshot(cx));
                this.change_selections(Default::default(), window, cx, |s| s.select(selections));
            } else {
                let clipboard_is_url = is_standalone_url(&clipboard_text);

                let auto_indent_mode = if !clipboard_text.is_empty() {
                    Some(AutoindentMode::Block {
                        original_indent_columns: Vec::new(),
                    })
                } else {
                    None
                };

                let selection_anchors = this.buffer.update(cx, |buffer, cx| {
                    let snapshot = buffer.snapshot(cx);

                    let anchors = old_selections
                        .iter()
                        .map(|s| {
                            let anchor = snapshot.anchor_after(s.head());
                            s.map(|_| anchor)
                        })
                        .collect::<Vec<_>>();

                    let mut edits = Vec::new();
                    let lines: Vec<&str> = clipboard_text.split('\n').collect();
                    let distribute_lines =
                        old_selections.len() > 1 && lines.len() == old_selections.len();

                    for (ix, selection) in old_selections.iter().enumerate() {
                        let language = snapshot.language_at(selection.head());
                        let range = selection.range();

                        let text_for_cursor: &str = if distribute_lines {
                            lines[ix]
                        } else {
                            &clipboard_text
                        };

                        let (edit_range, edit_text) = if let Some(language) = language
                            && language.name() == "Markdown"
                        {
                            edit_for_markdown_paste(
                                &snapshot,
                                range,
                                text_for_cursor,
                                clipboard_is_url,
                            )
                        } else {
                            (range, Cow::Borrowed(text_for_cursor))
                        };

                        edits.push((edit_range, edit_text));
                    }

                    drop(snapshot);
                    buffer.edit(edits, auto_indent_mode, cx);

                    anchors
                });

                this.change_selections(Default::default(), window, cx, |s| {
                    s.select_anchors(selection_anchors);
                });
            }

            let trigger_in_words =
                this.show_edit_predictions_in_menu() || !had_active_edit_prediction;

            this.trigger_completion_on_input(text, trigger_in_words, window, cx);
        });
    }

    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(item) = cx.read_from_clipboard() {
            self.paste_item(&item, window, cx);
        }
    }

    pub fn paste_item(
        &mut self,
        item: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        let clipboard_string = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(s) => Some(s),
            _ => None,
        });
        match clipboard_string {
            Some(clipboard_string) => self.do_paste(
                clipboard_string.text(),
                clipboard_string.metadata_json::<Vec<ClipboardSelection>>(),
                true,
                window,
                cx,
            ),
            _ => self.do_paste(&item.text().unwrap_or_default(), None, true, window, cx),
        }
    }
}

fn edit_for_markdown_paste<'a>(
    buffer: &MultiBufferSnapshot,
    range: Range<MultiBufferOffset>,
    to_insert: &'a str,
    to_insert_is_url: bool,
) -> (Range<MultiBufferOffset>, Cow<'a, str>) {
    if !to_insert_is_url {
        return (range, Cow::Borrowed(to_insert));
    };

    let old_text = buffer.text_for_range(range.clone()).collect::<String>();

    let new_text = if range.is_empty() || is_standalone_url(&old_text) {
        Cow::Borrowed(to_insert)
    } else {
        Cow::Owned(format!("[{old_text}]({to_insert})"))
    };
    (range, new_text)
}

fn is_standalone_url(text: &str) -> bool {
    let mut finder = linkify::LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]);
    finder
        .links(text)
        .next()
        .is_some_and(|link| link.start() == 0 && link.end() == text.len())
}
