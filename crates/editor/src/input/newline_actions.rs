use super::*;

impl Editor {
    pub fn newline(&mut self, _: &Newline, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }

        self.transact(window, cx, |this, window, cx| {
            let (edits_with_flags, selection_info): (Vec<_>, Vec<_>) = {
                let selections = this
                    .selections
                    .all::<MultiBufferOffset>(&this.display_snapshot(cx));
                let multi_buffer = this.buffer.read(cx);
                let buffer = multi_buffer.snapshot(cx);
                selections
                    .iter()
                    .map(|selection| {
                        let start_point = selection.start.to_point(&buffer);
                        let mut existing_indent =
                            buffer.indent_size_for_line(MultiBufferRow(start_point.row));
                        let full_indent_len = existing_indent.len;
                        existing_indent.len = cmp::min(existing_indent.len, start_point.column);
                        let mut start = selection.start;
                        let end = selection.end;
                        let selection_is_empty = start == end;
                        let language_scope = buffer.language_scope_at(start);
                        let (delimiter, newline_config) = if let Some(language) = &language_scope {
                            let needs_extra_newline = NewlineConfig::insert_extra_newline_brackets(
                                &buffer,
                                start..end,
                                language,
                            )
                                || NewlineConfig::insert_extra_newline_tree_sitter(
                                    &buffer,
                                    start..end,
                                );

                            let mut newline_config = NewlineConfig::Newline {
                                additional_indent: IndentSize::spaces(0),
                                extra_line_additional_indent: if needs_extra_newline {
                                    Some(IndentSize::spaces(0))
                                } else {
                                    None
                                },
                                prevent_auto_indent: false,
                            };
                            let mut delimiter = None;

                            if let Some(comment_delimiter) = maybe!({
                                if !selection_is_empty {
                                    return None;
                                }

                                if !multi_buffer.language_settings(cx).extend_comment_on_newline {
                                    return None;
                                }

                                return comment_delimiter_for_newline(
                                    &start_point,
                                    &buffer,
                                    language,
                                );
                            }) {
                                delimiter = Some(comment_delimiter);
                                if let NewlineConfig::Newline {
                                    extra_line_additional_indent,
                                    ..
                                } = &mut newline_config
                                {
                                    *extra_line_additional_indent = None;
                                }
                            } else if let Some(doc_delimiter) = maybe!({
                                if !selection_is_empty {
                                    return None;
                                }

                                if !multi_buffer.language_settings(cx).extend_comment_on_newline {
                                    return None;
                                }

                                return documentation_delimiter_for_newline(
                                    &start_point,
                                    &buffer,
                                    language,
                                    &mut newline_config,
                                );
                            }) {
                                delimiter = Some(doc_delimiter);
                            } else if let Some(list_delimiter) = maybe!({
                                if !selection_is_empty {
                                    return None;
                                }

                                if !multi_buffer.language_settings(cx).extend_list_on_newline {
                                    return None;
                                }

                                return list_delimiter_for_newline(
                                    &start_point,
                                    &buffer,
                                    language,
                                    &mut newline_config,
                                );
                            }) {
                                delimiter = Some(list_delimiter);
                            }

                            (delimiter, newline_config)
                        } else {
                            (
                                None,
                                NewlineConfig::Newline {
                                    additional_indent: IndentSize::spaces(0),
                                    extra_line_additional_indent: None,
                                    prevent_auto_indent: false,
                                },
                            )
                        };

                        let (edit_start, new_text, prevent_auto_indent) = match &newline_config {
                            NewlineConfig::ClearCurrentLine => {
                                let row_start =
                                    buffer.point_to_offset(Point::new(start_point.row, 0));
                                (row_start, String::new(), false)
                            }
                            NewlineConfig::UnindentCurrentLine { continuation } => {
                                let row_start =
                                    buffer.point_to_offset(Point::new(start_point.row, 0));
                                let tab_size = buffer.language_settings_at(start, cx).tab_size;
                                existing_indent.len = existing_indent
                                    .len
                                    .saturating_sub(existing_indent.outdent_len(tab_size));
                                let mut new_text = String::new();
                                new_text.extend(existing_indent.chars());
                                new_text.push_str(continuation);
                                (row_start, new_text, true)
                            }
                            NewlineConfig::Newline {
                                additional_indent,
                                extra_line_additional_indent,
                                prevent_auto_indent,
                            } => {
                                let auto_indent_mode =
                                    buffer.language_settings_at(start, cx).auto_indent;
                                let preserve_indent =
                                    auto_indent_mode != language::AutoIndentMode::None;
                                let apply_syntax_indent =
                                    auto_indent_mode == language::AutoIndentMode::SyntaxAware;
                                let capacity_for_delimiter =
                                    delimiter.as_deref().map(str::len).unwrap_or_default();
                                let existing_indent_len = if preserve_indent {
                                    existing_indent.len as usize
                                } else {
                                    0
                                };
                                let extra_line_len = extra_line_additional_indent
                                    .map(|i| 1 + existing_indent_len + i.len as usize)
                                    .unwrap_or(0);
                                let mut new_text = String::with_capacity(
                                    1 + capacity_for_delimiter
                                        + existing_indent_len
                                        + additional_indent.len as usize
                                        + extra_line_len,
                                );
                                new_text.push('\n');
                                if preserve_indent {
                                    new_text.extend(existing_indent.chars());
                                }
                                new_text.extend(additional_indent.chars());
                                if let Some(delimiter) = &delimiter {
                                    new_text.push_str(delimiter);
                                }
                                if let Some(extra_indent) = extra_line_additional_indent {
                                    new_text.push('\n');
                                    if preserve_indent {
                                        new_text.extend(existing_indent.chars());
                                    }
                                    new_text.extend(extra_indent.chars());
                                }
                                // Extend the edit to the beginning of the line
                                // to clear auto-indent whitespace that would
                                // otherwise remain as trailing whitespace. This
                                // applies to blank lines and lines where only
                                // indentation remains before the cursor.
                                if selection_is_empty
                                    && preserve_indent
                                    && full_indent_len > 0
                                    && start_point.column == full_indent_len
                                {
                                    start = buffer.point_to_offset(Point::new(start_point.row, 0));
                                }

                                (
                                    start,
                                    new_text,
                                    *prevent_auto_indent || !apply_syntax_indent,
                                )
                            }
                        };

                        let anchor = buffer.anchor_after(end);
                        let new_selection = selection.map(|_| anchor);
                        (
                            ((edit_start..end, new_text), prevent_auto_indent),
                            (newline_config.has_extra_line(), new_selection),
                        )
                    })
                    .unzip()
            };

            let mut auto_indent_edits = Vec::new();
            let mut edits = Vec::new();
            for (edit, prevent_auto_indent) in edits_with_flags {
                if prevent_auto_indent {
                    edits.push(edit);
                } else {
                    auto_indent_edits.push(edit);
                }
            }
            if !edits.is_empty() {
                this.edit(edits, cx);
            }
            if !auto_indent_edits.is_empty() {
                this.edit_with_autoindent(auto_indent_edits, cx);
            }

            let buffer = this.buffer.read(cx).snapshot(cx);
            let new_selections = selection_info
                .into_iter()
                .map(|(extra_newline_inserted, new_selection)| {
                    let mut cursor = new_selection.end.to_point(&buffer);
                    if extra_newline_inserted {
                        cursor.row -= 1;
                        cursor.column = buffer.line_len(MultiBufferRow(cursor.row));
                    }
                    new_selection.map(|_| cursor)
                })
                .collect();

            this.change_selections(Default::default(), window, cx, |s| s.select(new_selections));
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
            if let Some(task) = this.trigger_on_type_formatting("\n".to_owned(), window, cx) {
                task.detach_and_log_err(cx);
            }
        });
    }

    pub fn newline_above(&mut self, _: &NewlineAbove, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }

        let buffer = self.buffer.read(cx);
        let snapshot = buffer.snapshot(cx);

        let mut edits = Vec::new();
        let mut rows = Vec::new();

        for (rows_inserted, selection) in self
            .selections
            .all_adjusted(&self.display_snapshot(cx))
            .into_iter()
            .enumerate()
        {
            let cursor = selection.head();
            let row = cursor.row;

            let start_of_line = snapshot.clip_point(Point::new(row, 0), Bias::Left);

            let newline = "\n".to_string();
            edits.push((start_of_line..start_of_line, newline));

            rows.push(row + rows_inserted as u32);
        }

        self.transact(window, cx, |editor, window, cx| {
            editor.edit(edits, cx);

            editor.change_selections(Default::default(), window, cx, |s| {
                let mut index = 0;
                s.move_cursors_with(&mut |map, _, _| {
                    let row = rows[index];
                    index += 1;

                    let point = Point::new(row, 0);
                    let boundary = map.next_line_boundary(point).1;
                    let clipped = map.clip_point(boundary, Bias::Left);

                    (clipped, SelectionGoal::None)
                });
            });

            let mut indent_edits = Vec::new();
            let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
            for row in rows {
                let indents = multibuffer_snapshot.suggested_indents(row..row + 1, cx);
                for (row, indent) in indents {
                    if indent.len == 0 {
                        continue;
                    }

                    let text = match indent.kind {
                        IndentKind::Space => " ".repeat(indent.len as usize),
                        IndentKind::Tab => "\t".repeat(indent.len as usize),
                    };
                    let point = Point::new(row.0, 0);
                    indent_edits.push((point..point, text));
                }
            }
            editor.edit(indent_edits, cx);
            if let Some(format) = editor.trigger_on_type_formatting("\n".to_owned(), window, cx) {
                format.detach_and_log_err(cx);
            }
        });
    }

    pub fn newline_below(&mut self, _: &NewlineBelow, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }

        let mut buffer_edits: HashMap<EntityId, (Entity<Buffer>, Vec<Point>)> = HashMap::default();
        let mut rows: Vec<Option<u32>> = Vec::new();
        let mut rows_inserted = 0;

        for selection in self.selections.all_adjusted(&self.display_snapshot(cx)) {
            let cursor = selection.head();
            let row = cursor.row;

            let point = Point::new(row, 0);
            let Some((buffer_handle, buffer_point)) =
                self.buffer.read(cx).point_to_buffer_point(point, cx)
            else {
                rows.push(None);
                continue;
            };

            buffer_edits
                .entry(buffer_handle.entity_id())
                .or_insert_with(|| (buffer_handle, Vec::new()))
                .1
                .push(buffer_point);

            rows_inserted += 1;
            rows.push(Some(row + rows_inserted));
        }

        self.transact(window, cx, |editor, window, cx| {
            for (_, (buffer_handle, points)) in &buffer_edits {
                buffer_handle.update(cx, |buffer, cx| {
                    let edits: Vec<_> = points
                        .iter()
                        .map(|point| {
                            let target = Point::new(point.row + 1, 0);
                            let start_of_line = buffer.point_to_offset(target).min(buffer.len());
                            (start_of_line..start_of_line, "\n")
                        })
                        .collect();
                    buffer.edit(edits, None, cx);
                });
            }

            editor.change_selections(Default::default(), window, cx, |s| {
                let mut index = 0;
                s.maybe_move_cursors_with(&mut |map, _, _| {
                    let row = rows.get(index).copied().flatten();
                    index += 1;

                    let point = Point::new(row?, 0);
                    let boundary = map.next_line_boundary(point).1;
                    let clipped = map.clip_point(boundary, Bias::Left);

                    Some((clipped, SelectionGoal::None))
                });
            });

            let mut indent_edits = Vec::new();
            let multibuffer_snapshot = editor.buffer.read(cx).snapshot(cx);
            for row in rows.into_iter().flatten() {
                let indents = multibuffer_snapshot.suggested_indents(row..row + 1, cx);
                for (row, indent) in indents {
                    if indent.len == 0 {
                        continue;
                    }

                    let text = match indent.kind {
                        IndentKind::Space => " ".repeat(indent.len as usize),
                        IndentKind::Tab => "\t".repeat(indent.len as usize),
                    };
                    let point = Point::new(row.0, 0);
                    indent_edits.push((point..point, text));
                }
            }
            editor.edit(indent_edits, cx);
            if let Some(format) = editor.trigger_on_type_formatting("\n".to_owned(), window, cx) {
                format.detach_and_log_err(cx);
            }
        });
    }
}
