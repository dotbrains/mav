use super::*;

struct DiffHunkHighlightColors {
    filled_background: Hsla,
    hollow_background: Hsla,
    hollow_border: Hsla,
}

impl EditorElement {
    pub(super) fn collect_background_highlights(
        &self,
        start_anchor: Anchor,
        end_anchor: Anchor,
        start_row: DisplayRow,
        end_row: DisplayRow,
        max_row: DisplayRow,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<(Range<DisplayPoint>, Hsla)> {
        self.editor_with_selections(cx)
            .map(|editor| {
                if editor == self.editor {
                    editor.read(cx).background_highlights_in_range(
                        start_anchor..end_anchor,
                        &snapshot.display_snapshot,
                        cx.theme(),
                    )
                } else {
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.snapshot(window, cx);
                        let start_anchor = if start_row == Default::default() {
                            Anchor::Min
                        } else {
                            snapshot.buffer_snapshot().anchor_before(
                                DisplayPoint::new(start_row, 0).to_offset(&snapshot, Bias::Left),
                            )
                        };
                        let end_anchor = if end_row > max_row {
                            Anchor::Max
                        } else {
                            snapshot.buffer_snapshot().anchor_before(
                                DisplayPoint::new(end_row, 0).to_offset(&snapshot, Bias::Right),
                            )
                        };

                        editor.background_highlights_in_range(
                            start_anchor..end_anchor,
                            &snapshot.display_snapshot,
                            cx.theme(),
                        )
                    })
                }
            })
            .unwrap_or_default()
    }

    pub(super) fn add_diff_and_drag_highlights(
        &self,
        highlighted_rows: &mut BTreeMap<DisplayRow, LineHighlight>,
        row_infos: &[RowInfo],
        start_row: DisplayRow,
        snapshot: &EditorSnapshot,
        cx: &mut App,
    ) {
        let colors = cx.theme().colors();
        let added_diff_hunk_colors = DiffHunkHighlightColors {
            filled_background: colors.editor_diff_hunk_added_background,
            hollow_background: colors.editor_diff_hunk_added_hollow_background,
            hollow_border: colors.editor_diff_hunk_added_hollow_border,
        };
        let deleted_diff_hunk_colors = DiffHunkHighlightColors {
            filled_background: colors.editor_diff_hunk_deleted_background,
            hollow_background: colors.editor_diff_hunk_deleted_hollow_background,
            hollow_border: colors.editor_diff_hunk_deleted_hollow_border,
        };
        let drag_highlight_color = colors.editor_active_line_background;
        let drag_border_color = colors.border_focused;

        for (ix, row_info) in row_infos.iter().enumerate() {
            let Some(diff_status) = row_info.diff_status else {
                continue;
            };

            let diff_hunk_colors = match diff_status.kind {
                DiffHunkStatusKind::Added => &added_diff_hunk_colors,
                DiffHunkStatusKind::Deleted => &deleted_diff_hunk_colors,
                DiffHunkStatusKind::Modified => {
                    debug_panic!("modified diff status for row info");
                    continue;
                }
            };

            let hollow_highlight = LineHighlight {
                background: diff_hunk_colors.hollow_background.into(),
                border: Some(diff_hunk_colors.hollow_border),
                include_gutter: true,
                type_id: None,
            };

            let filled_highlight = LineHighlight {
                background: solid_background(diff_hunk_colors.filled_background),
                border: None,
                include_gutter: true,
                type_id: None,
            };

            let background = if self.diff_hunk_hollow(diff_status, cx) {
                hollow_highlight
            } else {
                filled_highlight
            };

            let base_display_point = DisplayPoint::new(start_row + DisplayRow(ix as u32), 0);

            highlighted_rows
                .entry(base_display_point.row())
                .or_insert(background);
        }

        let Some(drag_state) = &self.editor.read(cx).diff_review_drag_state else {
            return;
        };

        let range = drag_state.row_range(&snapshot.display_snapshot);
        let drag_highlight = LineHighlight {
            background: solid_background(drag_highlight_color),
            border: Some(drag_border_color),
            include_gutter: true,
            type_id: None,
        };
        for row_num in range.start().0..=range.end().0 {
            highlighted_rows
                .entry(DisplayRow(row_num))
                .or_insert(drag_highlight);
        }
    }
}
