use super::*;

impl EditorElement {
    pub(super) fn update_snapshot_layout(
        &self,
        bounds: Bounds<Pixels>,
        snapshot: EditorSnapshot,
        gutter_dimensions: GutterDimensions,
        line_height: Pixels,
        editor_width: Pixels,
        em_advance: Pixels,
        em_layout_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> EditorSnapshot {
        self.editor.update(cx, |editor, cx| {
            editor.last_bounds = Some(bounds);
            editor.gutter_dimensions = gutter_dimensions;
            editor.set_visible_line_count((bounds.size.height / line_height) as f64, window, cx);
            editor.set_visible_column_count(f64::from(editor_width / em_advance));

            if matches!(
                editor.mode,
                EditorMode::AutoHeight { .. } | EditorMode::Minimap { .. }
            ) {
                snapshot
            } else {
                let wrap_width =
                    calculate_wrap_width(editor.soft_wrap_mode(cx), editor_width, em_layout_width);

                if editor.set_wrap_width(wrap_width, cx) {
                    editor.snapshot(window, cx)
                } else {
                    snapshot
                }
            }
        })
    }
}
