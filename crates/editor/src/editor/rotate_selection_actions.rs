use super::*;

impl Editor {
    pub fn rotate_selections_forward(
        &mut self,
        _: &RotateSelectionsForward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rotate_selections(window, cx, false)
    }

    pub fn rotate_selections_backward(
        &mut self,
        _: &RotateSelectionsBackward,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rotate_selections(window, cx, true)
    }

    fn rotate_selections(&mut self, window: &mut Window, cx: &mut Context<Self>, reverse: bool) {
        if self.read_only(cx) {
            return;
        }
        let display_snapshot = self.display_snapshot(cx);
        let selections = self.selections.all::<MultiBufferOffset>(&display_snapshot);

        if selections.len() < 2 {
            return;
        }

        let (edits, new_selections) = {
            let buffer = self.buffer.read(cx).read(cx);
            let has_selections = selections.iter().any(|s| !s.is_empty());
            if has_selections {
                let mut selected_texts: Vec<String> = selections
                    .iter()
                    .map(|selection| {
                        buffer
                            .text_for_range(selection.start..selection.end)
                            .collect()
                    })
                    .collect();

                if reverse {
                    selected_texts.rotate_left(1);
                } else {
                    selected_texts.rotate_right(1);
                }

                let mut offset_delta: i64 = 0;
                let mut new_selections = Vec::new();
                let edits: Vec<_> = selections
                    .iter()
                    .zip(selected_texts.iter())
                    .map(|(selection, new_text)| {
                        let old_len = (selection.end.0 - selection.start.0) as i64;
                        let new_len = new_text.len() as i64;
                        let adjusted_start =
                            MultiBufferOffset((selection.start.0 as i64 + offset_delta) as usize);
                        let adjusted_end =
                            MultiBufferOffset((adjusted_start.0 as i64 + new_len) as usize);

                        new_selections.push(Selection {
                            id: selection.id,
                            start: adjusted_start,
                            end: adjusted_end,
                            reversed: selection.reversed,
                            goal: selection.goal,
                        });

                        offset_delta += new_len - old_len;
                        (selection.start..selection.end, new_text.clone())
                    })
                    .collect();
                (edits, new_selections)
            } else {
                let mut all_rows: Vec<u32> = selections
                    .iter()
                    .map(|selection| buffer.offset_to_point(selection.start).row)
                    .collect();
                all_rows.sort_unstable();
                all_rows.dedup();

                if all_rows.len() < 2 {
                    return;
                }

                let line_ranges: Vec<Range<MultiBufferOffset>> = all_rows
                    .iter()
                    .map(|&row| {
                        let start = Point::new(row, 0);
                        let end = Point::new(row, buffer.line_len(MultiBufferRow(row)));
                        buffer.point_to_offset(start)..buffer.point_to_offset(end)
                    })
                    .collect();

                let mut line_texts: Vec<String> = line_ranges
                    .iter()
                    .map(|range| buffer.text_for_range(range.clone()).collect())
                    .collect();

                if reverse {
                    line_texts.rotate_left(1);
                } else {
                    line_texts.rotate_right(1);
                }

                let edits = line_ranges
                    .iter()
                    .zip(line_texts.iter())
                    .map(|(range, new_text)| (range.clone(), new_text.clone()))
                    .collect();

                let num_rows = all_rows.len();
                let row_to_index: std::collections::HashMap<u32, usize> = all_rows
                    .iter()
                    .enumerate()
                    .map(|(i, &row)| (row, i))
                    .collect();

                // Compute new line start offsets after rotation (handles CRLF)
                let newline_len = line_ranges[1].start.0 - line_ranges[0].end.0;
                let first_line_start = line_ranges[0].start.0;
                let mut new_line_starts: Vec<usize> = vec![first_line_start];
                for text in line_texts.iter().take(num_rows - 1) {
                    let prev_start = *new_line_starts.last().unwrap();
                    new_line_starts.push(prev_start + text.len() + newline_len);
                }

                let new_selections = selections
                    .iter()
                    .map(|selection| {
                        let point = buffer.offset_to_point(selection.start);
                        let old_index = row_to_index[&point.row];
                        let new_index = if reverse {
                            (old_index + num_rows - 1) % num_rows
                        } else {
                            (old_index + 1) % num_rows
                        };
                        let new_offset =
                            MultiBufferOffset(new_line_starts[new_index] + point.column as usize);
                        Selection {
                            id: selection.id,
                            start: new_offset,
                            end: new_offset,
                            reversed: selection.reversed,
                            goal: selection.goal,
                        }
                    })
                    .collect();

                (edits, new_selections)
            }
        };

        self.transact(window, cx, |this, window, cx| {
            this.buffer.update(cx, |buffer, cx| {
                buffer.edit(edits, None, cx);
            });
            this.change_selections(Default::default(), window, cx, |s| {
                s.select(new_selections);
            });
        });
    }
}
