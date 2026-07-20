use super::*;

impl Editor {
    pub fn duplicate(
        &mut self,
        upwards: bool,
        whole_lines: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }

        let display_map = self.display_map.update(cx, |map, cx| map.snapshot(cx));
        let buffer = display_map.buffer_snapshot();
        let selections = self.selections.all::<Point>(&display_map);

        let mut edits = Vec::new();
        let mut selections_iter = selections.iter().peekable();
        while let Some(selection) = selections_iter.next() {
            let mut rows = selection.spanned_rows(false, &display_map);
            // duplicate line-wise
            if whole_lines || selection.start == selection.end {
                // Avoid duplicating the same lines twice.
                while let Some(next_selection) = selections_iter.peek() {
                    let next_rows = next_selection.spanned_rows(false, &display_map);
                    if next_rows.start < rows.end {
                        rows.end = next_rows.end;
                        selections_iter.next().unwrap();
                    } else {
                        break;
                    }
                }

                // Copy the text from the selected row region and splice it either at the start
                // or end of the region.
                let start = Point::new(rows.start.0, 0);
                let end = Point::new(
                    rows.end.previous_row().0,
                    buffer.line_len(rows.end.previous_row()),
                );

                let mut text = buffer.text_for_range(start..end).collect::<String>();

                let insert_location = if upwards {
                    // When duplicating upward, we need to insert before the current line.
                    // If we're on the last line and it doesn't end with a newline,
                    // we need to add a newline before the duplicated content.
                    let needs_leading_newline = rows.end.0 >= buffer.max_point().row
                        && buffer.max_point().column > 0
                        && !text.ends_with('\n');

                    if needs_leading_newline {
                        text.insert(0, '\n');
                        end
                    } else {
                        text.push('\n');
                        Point::new(rows.start.0, 0)
                    }
                } else {
                    text.push('\n');
                    start
                };
                edits.push((insert_location..insert_location, text));
            } else {
                // duplicate character-wise
                let start = selection.start;
                let end = selection.end;
                let text = buffer.text_for_range(start..end).collect::<String>();
                edits.push((selection.end..selection.end, text));
            }
        }

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });

            // When duplicating upward with whole lines, move the cursor to the duplicated line
            if upwards && whole_lines {
                let display_map = this.display_map.update(cx, |map, cx| map.snapshot(cx));

                this.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                    let mut new_ranges = Vec::new();
                    let selections = s.all::<Point>(&display_map);
                    let mut selections_iter = selections.iter().peekable();

                    while let Some(first_selection) = selections_iter.next() {
                        // Group contiguous selections together to find the total row span
                        let mut group_selections = vec![first_selection];
                        let mut rows = first_selection.spanned_rows(false, &display_map);

                        while let Some(next_selection) = selections_iter.peek() {
                            let next_rows = next_selection.spanned_rows(false, &display_map);
                            if next_rows.start < rows.end {
                                rows.end = next_rows.end;
                                group_selections.push(selections_iter.next().unwrap());
                            } else {
                                break;
                            }
                        }

                        let row_count = rows.end.0 - rows.start.0;

                        // Move all selections in this group up by the total number of duplicated rows
                        for selection in group_selections {
                            let new_start = Point::new(
                                selection.start.row.saturating_sub(row_count),
                                selection.start.column,
                            );

                            let new_end = Point::new(
                                selection.end.row.saturating_sub(row_count),
                                selection.end.column,
                            );

                            new_ranges.push(new_start..new_end);
                        }
                    }

                    s.select_ranges(new_ranges);
                });
            }

            this.request_autoscroll(Autoscroll::fit(), cx);
        });
    }

    pub fn duplicate_line_up(
        &mut self,
        _: &DuplicateLineUp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate(true, true, window, cx);
    }

    pub fn duplicate_line_down(
        &mut self,
        _: &DuplicateLineDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate(false, true, window, cx);
    }

    pub fn duplicate_selection(
        &mut self,
        _: &DuplicateSelection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate(false, false, window, cx);
    }
}
