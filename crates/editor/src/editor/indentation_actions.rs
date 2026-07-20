use super::*;

impl Editor {
    pub fn backtab(&mut self, _: &Backtab, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode.is_single_line() {
            cx.propagate();
            return;
        }

        if self.move_to_prev_snippet_tabstop(window, cx) {
            return;
        }
        self.outdent(&Outdent, window, cx);
    }

    pub fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode.is_single_line() {
            cx.propagate();
            return;
        }

        if self.move_to_next_snippet_tabstop(window, cx) {
            return;
        }
        if self.read_only(cx) {
            return;
        }
        let mut selections = self.selections.all_adjusted(&self.display_snapshot(cx));
        let buffer = self.buffer.read(cx);
        let snapshot = buffer.snapshot(cx);
        let rows_iter = selections.iter().map(|s| s.head().row);
        let suggested_indents = snapshot.suggested_indents(rows_iter, cx);

        let has_some_cursor_in_whitespace = selections
            .iter()
            .filter(|selection| selection.is_empty())
            .any(|selection| {
                let cursor = selection.head();
                let current_indent = snapshot.indent_size_for_line(MultiBufferRow(cursor.row));
                cursor.column < current_indent.len
            });

        let mut edits = Vec::new();
        let mut prev_edited_row = 0;
        let mut row_delta = 0;
        for selection in &mut selections {
            if selection.start.row != prev_edited_row {
                row_delta = 0;
            }
            prev_edited_row = selection.end.row;

            if selection.is_empty() {
                let cursor = selection.head();
                let settings = buffer.language_settings_at(cursor, cx);
                if settings.indent_list_on_tab {
                    if let Some(language) = snapshot.language_scope_at(Point::new(cursor.row, 0)) {
                        if input::is_list_prefix_row(
                            MultiBufferRow(cursor.row),
                            &snapshot,
                            &language,
                        ) {
                            row_delta = Self::indent_selection(
                                buffer, &snapshot, selection, &mut edits, row_delta, cx,
                            );
                            continue;
                        }
                    }
                }
            }

            if !selection.is_empty() {
                row_delta =
                    Self::indent_selection(buffer, &snapshot, selection, &mut edits, row_delta, cx);
                continue;
            }

            let cursor = selection.head();
            let current_indent = snapshot.indent_size_for_line(MultiBufferRow(cursor.row));
            if let Some(suggested_indent) =
                suggested_indents.get(&MultiBufferRow(cursor.row)).copied()
            {
                if has_some_cursor_in_whitespace
                    && cursor.column == current_indent.len
                    && current_indent.len == suggested_indent.len
                {
                    continue;
                }

                if cursor.column < suggested_indent.len
                    && cursor.column <= current_indent.len
                    && current_indent.len <= suggested_indent.len
                {
                    selection.start = Point::new(cursor.row, suggested_indent.len);
                    selection.end = selection.start;
                    if row_delta == 0 {
                        edits.extend(Buffer::edit_for_indent_size_adjustment(
                            cursor.row,
                            current_indent,
                            suggested_indent,
                        ));
                        row_delta = suggested_indent.len - current_indent.len;
                    }
                    continue;
                }

                if cursor.column < current_indent.len && current_indent.len > suggested_indent.len {
                    selection.start = Point::new(cursor.row, current_indent.len);
                    selection.end = selection.start;
                    continue;
                }
            }

            let settings = buffer.language_settings_at(cursor, cx);
            let tab_size = if settings.hard_tabs {
                IndentSize::tab()
            } else {
                let tab_size = settings.tab_size.get();
                let indent_remainder = snapshot
                    .text_for_range(Point::new(cursor.row, 0)..cursor)
                    .flat_map(str::chars)
                    .fold(row_delta % tab_size, |counter: u32, c| {
                        if c == '\t' {
                            0
                        } else {
                            (counter + 1) % tab_size
                        }
                    });

                let chars_to_next_tab_stop = tab_size - indent_remainder;
                IndentSize::spaces(chars_to_next_tab_stop)
            };
            selection.start = Point::new(cursor.row, cursor.column + row_delta + tab_size.len);
            selection.end = selection.start;
            edits.push((cursor..cursor, tab_size.chars().collect::<String>()));
            row_delta += tab_size.len;
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |b, cx| b.edit(edits, None, cx));
            this.change_selections(Default::default(), window, cx, |s| s.select(selections));
            this.refresh_edit_prediction(
                true,
                false,
                EditPredictionRequestTrigger::BufferEdit,
                window,
                cx,
            );
        });
    }

    pub fn indent(&mut self, _: &Indent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        if self.mode.is_single_line() {
            cx.propagate();
            return;
        }

        let mut selections = self.selections.all::<Point>(&self.display_snapshot(cx));
        let mut prev_edited_row = 0;
        let mut row_delta = 0;
        let mut edits = Vec::new();
        let buffer = self.buffer.read(cx);
        let snapshot = buffer.snapshot(cx);
        for selection in &mut selections {
            if selection.start.row != prev_edited_row {
                row_delta = 0;
            }
            prev_edited_row = selection.end.row;

            row_delta =
                Self::indent_selection(buffer, &snapshot, selection, &mut edits, row_delta, cx);
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |b, cx| b.edit(edits, None, cx));
            this.change_selections(Default::default(), window, cx, |s| s.select(selections));
        });
    }

    fn indent_selection(
        buffer: &MultiBuffer,
        snapshot: &MultiBufferSnapshot,
        selection: &mut Selection<Point>,
        edits: &mut Vec<(Range<Point>, String)>,
        delta_for_start_row: u32,
        cx: &App,
    ) -> u32 {
        let settings = buffer.language_settings_at(selection.start, cx);
        let tab_size = settings.tab_size.get();
        let indent_kind = if settings.hard_tabs {
            IndentKind::Tab
        } else {
            IndentKind::Space
        };
        let mut start_row = selection.start.row;
        let mut end_row = selection.end.row + 1;

        if selection.end.column == 0 && selection.end.row > selection.start.row {
            end_row -= 1;
        }

        if delta_for_start_row > 0 {
            start_row += 1;
            selection.start.column += delta_for_start_row;
            if selection.end.row == selection.start.row {
                selection.end.column += delta_for_start_row;
            }
        }

        let mut delta_for_end_row = 0;
        let has_multiple_rows = start_row + 1 != end_row;
        for row in start_row..end_row {
            let current_indent = snapshot.indent_size_for_line(MultiBufferRow(row));
            let indent_delta = match (current_indent.kind, indent_kind) {
                (IndentKind::Space, IndentKind::Space) => {
                    let columns_to_next_tab_stop = tab_size - (current_indent.len % tab_size);
                    IndentSize::spaces(columns_to_next_tab_stop)
                }
                (IndentKind::Tab, IndentKind::Space) => IndentSize::spaces(tab_size),
                (_, IndentKind::Tab) => IndentSize::tab(),
            };

            let start = if has_multiple_rows || current_indent.len < selection.start.column {
                0
            } else {
                selection.start.column
            };
            let row_start = Point::new(row, start);
            edits.push((
                row_start..row_start,
                indent_delta.chars().collect::<String>(),
            ));

            if row == selection.start.row {
                selection.start.column += indent_delta.len;
            }
            if row == selection.end.row {
                selection.end.column += indent_delta.len;
                delta_for_end_row = indent_delta.len;
            }
        }

        if selection.start.row == selection.end.row {
            delta_for_start_row + delta_for_end_row
        } else {
            delta_for_end_row
        }
    }

    pub fn outdent(&mut self, _: &Outdent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        if self.mode.is_single_line() {
            cx.propagate();
            return;
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let selections = self.selections.all::<Point>(&display_map);
        let mut deletion_ranges = Vec::new();
        let mut last_outdent = None;
        {
            let buffer = self.buffer.read(cx);
            let snapshot = buffer.snapshot(cx);
            for selection in &selections {
                let settings = buffer.language_settings_at(selection.start, cx);
                let tab_size = settings.tab_size;
                let mut rows = selection.spanned_rows(false, &display_map);

                if let Some(last_row) = last_outdent
                    && last_row == rows.start
                {
                    rows.start = rows.start.next_row();
                }
                let has_multiple_rows = rows.len() > 1;
                for row in rows.iter_rows() {
                    let indent_size = snapshot.indent_size_for_line(row);
                    if indent_size.len > 0 {
                        let deletion_len = indent_size.outdent_len(tab_size);
                        let start = if has_multiple_rows
                            || deletion_len > selection.start.column
                            || indent_size.len < selection.start.column
                        {
                            0
                        } else {
                            selection.start.column - deletion_len
                        };
                        deletion_ranges.push(
                            Point::new(row.0, start)..Point::new(row.0, start + deletion_len),
                        );
                        last_outdent = Some(row);
                    }
                }
            }
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                let empty_str: Arc<str> = Arc::default();
                buffer.edit(
                    deletion_ranges
                        .into_iter()
                        .map(|range| (range, empty_str.clone())),
                    None,
                    cx,
                );
            });
            let selections = this
                .selections
                .all::<MultiBufferOffset>(&this.display_snapshot(cx));
            this.change_selections(Default::default(), window, cx, |s| s.select(selections));
        });
    }

    pub fn autoindent(&mut self, _: &AutoIndent, window: &mut Window, cx: &mut Context<Self>) {
        if self.read_only(cx) {
            return;
        }
        if self.mode.is_single_line() {
            cx.propagate();
            return;
        }

        let selections = self
            .selections
            .all::<MultiBufferOffset>(&self.display_snapshot(cx))
            .into_iter()
            .map(|s| s.range());

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.autoindent_ranges(selections, cx);
            });
            let selections = this
                .selections
                .all::<MultiBufferOffset>(&this.display_snapshot(cx));
            this.change_selections(Default::default(), window, cx, |s| s.select(selections));
        });
    }
}
