use super::*;

impl EditorElement {
    pub(super) fn layout_row_activity(
        &self,
        rows: Range<DisplayRow>,
        snapshot: &EditorSnapshot,
        active_rows: &mut BTreeMap<DisplayRow, LineHighlightSpec>,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::RowActivity {
        // Relative rows are based on newest selection, even outside the visible area.
        let current_selection_head = self.editor.update(cx, |editor, cx| {
            (editor.selections.count() != 0).then(|| {
                let newest = editor
                    .selections
                    .newest::<Point>(&editor.display_snapshot(cx));

                SelectionLayout::new(
                    newest,
                    editor.selections.line_mode(),
                    editor.cursor_offset_on_selection,
                    editor.cursor_shape,
                    snapshot,
                    true,
                    true,
                    None,
                )
                .head
                .row()
            })
        });

        let run_indicator_rows = self.editor.update(cx, |editor, cx| {
            editor.active_run_indicators(rows.clone(), window, cx)
        });

        let breakpoint_rows = self
            .editor
            .update(cx, |editor, cx| editor.active_breakpoints(rows, window, cx));

        for (display_row, (_, bp, state)) in &breakpoint_rows {
            if bp.is_enabled() && state.is_none_or(|s| s.verified) {
                active_rows.entry(*display_row).or_default().breakpoint = true;
            }
        }

        layout_data::RowActivity {
            current_selection_head,
            run_indicator_rows,
            breakpoint_rows,
        }
    }
}
