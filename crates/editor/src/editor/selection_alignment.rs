use super::*;

impl Editor {
    pub fn align_selections(
        &mut self,
        _: &crate::actions::AlignSelections,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.read_only(cx) {
            return;
        }

        let display_snapshot = self.display_snapshot(cx);

        struct CursorData {
            anchor: Anchor,
            point: Point,
        }

        let cursor_data: Vec<CursorData> = self
            .selections
            .disjoint_anchors()
            .iter()
            .map(|selection| {
                let anchor = if selection.reversed {
                    selection.head()
                } else {
                    selection.tail()
                };
                CursorData {
                    anchor,
                    point: anchor.to_point(&display_snapshot.buffer_snapshot()),
                }
            })
            .collect();

        let rows_anchors_count: Vec<usize> = cursor_data
            .iter()
            .map(|cursor| cursor.point.row)
            .chunk_by(|&row| row)
            .into_iter()
            .map(|(_, group)| group.count())
            .collect();
        let max_columns = rows_anchors_count.iter().max().copied().unwrap_or(0);
        let mut rows_column_offset = vec![0; rows_anchors_count.len()];
        let mut edits = Vec::new();

        for column_idx in 0..max_columns {
            let mut cursor_index = 0;

            let mut target_column = 0;
            for (row_idx, cursor_count) in rows_anchors_count.iter().enumerate() {
                if column_idx >= *cursor_count {
                    cursor_index += cursor_count;
                    continue;
                }

                let point = &cursor_data[cursor_index + column_idx].point;
                let adjusted_column = point.column + rows_column_offset[row_idx];
                if adjusted_column > target_column {
                    target_column = adjusted_column;
                }
                cursor_index += cursor_count;
            }

            cursor_index = 0;
            for (row_idx, cursor_count) in rows_anchors_count.iter().enumerate() {
                if column_idx >= *cursor_count {
                    cursor_index += *cursor_count;
                    continue;
                }

                let point = &cursor_data[cursor_index + column_idx].point;
                let spaces_needed = target_column - point.column - rows_column_offset[row_idx];
                if spaces_needed > 0 {
                    let anchor = cursor_data[cursor_index + column_idx]
                        .anchor
                        .bias_left(&display_snapshot);
                    edits.push((anchor..anchor, " ".repeat(spaces_needed as usize)));
                }
                rows_column_offset[row_idx] += spaces_needed;

                cursor_index += *cursor_count;
            }
        }

        if !edits.is_empty() {
            self.transact(window, cx, |editor, _window, cx| {
                editor.edit(edits, cx);
            });
        }
    }
}
