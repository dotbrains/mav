use super::*;

impl Editor {
    pub fn delete_line(&mut self, _: &DeleteLine, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let selections = self.selections.all::<Point>(&display_map);

        let mut new_cursors = Vec::new();
        let mut edit_ranges = Vec::new();
        let mut selections = selections.iter().peekable();
        while let Some(selection) = selections.next() {
            let mut rows = selection.spanned_rows(false, &display_map);

            while let Some(next_selection) = selections.peek() {
                let next_rows = next_selection.spanned_rows(false, &display_map);
                if next_rows.start <= rows.end {
                    rows.end = next_rows.end;
                    selections.next().unwrap();
                } else {
                    break;
                }
            }

            let buffer = display_map.buffer_snapshot();
            let mut edit_start = ToOffset::to_offset(&Point::new(rows.start.0, 0), buffer);
            let (edit_end, target_row) = if buffer.max_point().row >= rows.end.0 {
                (
                    ToOffset::to_offset(&Point::new(rows.end.0, 0), buffer),
                    rows.end,
                )
            } else {
                edit_start = edit_start.saturating_sub_usize(1);
                (buffer.len(), rows.start.previous_row())
            };

            let text_layout_details = self.text_layout_details(window, cx);
            let x = display_map.x_for_display_point(
                selection.head().to_display_point(&display_map),
                &text_layout_details,
            );
            let row = Point::new(target_row.0, 0)
                .to_display_point(&display_map)
                .row();
            let column = display_map.display_column_for_x(row, x, &text_layout_details);

            new_cursors.push((
                selection.id,
                buffer.anchor_after(DisplayPoint::new(row, column).to_point(&display_map)),
                SelectionGoal::None,
            ));
            edit_ranges.push(edit_start..edit_end);
        }

        self.transact(window, cx, |this, window, cx| {
            let buffer = this.buffer.update(cx, |buffer, cx| {
                let empty_str: Arc<str> = Arc::default();
                buffer.edit(
                    edit_ranges
                        .into_iter()
                        .map(|range| (range, empty_str.clone())),
                    None,
                    cx,
                );
                buffer.snapshot(cx)
            });
            let new_selections = new_cursors
                .into_iter()
                .map(|(id, cursor, goal)| {
                    let cursor = cursor.to_point(&buffer);
                    Selection {
                        id,
                        start: cursor,
                        end: cursor,
                        reversed: false,
                        goal,
                    }
                })
                .collect();

            this.change_selections(Default::default(), window, cx, |s| {
                s.select(new_selections);
            });
        });
    }

    pub fn join_lines_impl(
        &mut self,
        insert_whitespace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }
        let mut row_ranges = Vec::<Range<MultiBufferRow>>::new();
        for selection in self.selections.all::<Point>(&self.display_snapshot(cx)) {
            let start = MultiBufferRow(selection.start.row);
            let end = if selection.start.row == selection.end.row {
                MultiBufferRow(selection.start.row + 1)
            } else if selection.end.column == 0 {
                if selection.start.row + 1 == selection.end.row {
                    MultiBufferRow(selection.end.row)
                } else {
                    MultiBufferRow(selection.end.row - 1)
                }
            } else {
                MultiBufferRow(selection.end.row)
            };

            if let Some(last_row_range) = row_ranges.last_mut()
                && start <= last_row_range.end
            {
                last_row_range.end = end;
                continue;
            }
            row_ranges.push(start..end);
        }

        let snapshot = self.buffer.read(cx).snapshot(cx);
        let mut cursor_positions = Vec::new();
        for row_range in &row_ranges {
            let anchor = snapshot.anchor_before(Point::new(
                row_range.end.previous_row().0,
                snapshot.line_len(row_range.end.previous_row()),
            ));
            cursor_positions.push(anchor..anchor);
        }

        self.transact(window, cx, |this, window, cx| {
            for row_range in row_ranges.into_iter().rev() {
                for row in row_range.iter_rows().rev() {
                    let end_of_line = Point::new(row.0, snapshot.line_len(row));
                    let next_line_row = row.next_row();
                    let indent = snapshot.indent_size_for_line(next_line_row);
                    let mut join_start_column = indent.len;

                    if let Some(language_scope) =
                        snapshot.language_scope_at(Point::new(next_line_row.0, indent.len))
                    {
                        let line_end =
                            Point::new(next_line_row.0, snapshot.line_len(next_line_row));
                        let line_text_after_indent = snapshot
                            .text_for_range(Point::new(next_line_row.0, indent.len)..line_end)
                            .collect::<String>();

                        if !line_text_after_indent.is_empty() {
                            let block_prefix = language_scope
                                .block_comment()
                                .map(|c| c.prefix.as_ref())
                                .filter(|p| !p.is_empty());
                            let doc_prefix = language_scope
                                .documentation_comment()
                                .map(|c| c.prefix.as_ref())
                                .filter(|p| !p.is_empty());
                            let all_prefixes = language_scope
                                .line_comment_prefixes()
                                .iter()
                                .map(|p| p.as_ref())
                                .chain(block_prefix)
                                .chain(doc_prefix)
                                .chain(language_scope.unordered_list().iter().map(|p| p.as_ref()));

                            let mut longest_prefix_len = None;
                            for prefix in all_prefixes {
                                let trimmed = prefix.trim_end();
                                if line_text_after_indent.starts_with(trimmed) {
                                    let candidate_len =
                                        if line_text_after_indent.starts_with(prefix) {
                                            prefix.len()
                                        } else {
                                            trimmed.len()
                                        };
                                    if longest_prefix_len.map_or(true, |len| candidate_len > len) {
                                        longest_prefix_len = Some(candidate_len);
                                    }
                                }
                            }

                            if let Some(prefix_len) = longest_prefix_len {
                                join_start_column =
                                    join_start_column.saturating_add(prefix_len as u32);
                            }
                        }
                    }

                    let start_of_next_line = Point::new(next_line_row.0, join_start_column);
                    let replace = if snapshot.line_len(next_line_row) > join_start_column
                        && insert_whitespace
                    {
                        " "
                    } else {
                        ""
                    };

                    this.buffer.update(cx, |buffer, cx| {
                        buffer.edit([(end_of_line..start_of_next_line, replace)], None, cx)
                    });
                }
            }

            this.change_selections(Default::default(), window, cx, |s| {
                s.select_anchor_ranges(cursor_positions)
            });
        });
    }

    pub fn join_lines(&mut self, _: &JoinLines, window: &mut Window, cx: &mut Context<Self>) {
        self.join_lines_impl(true, window, cx);
    }
}
