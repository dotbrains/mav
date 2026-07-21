use super::*;

pub(super) struct HighlightInputs {
    pub(super) highlighted_rows: BTreeMap<DisplayRow, LineHighlight>,
    pub(super) highlighted_ranges: Vec<(Range<DisplayPoint>, Hsla)>,
    pub(super) highlighted_gutter_ranges: Vec<(Range<DisplayPoint>, Hsla)>,
    pub(super) document_colors:
        Option<(DocumentColorsRenderMode, Vec<(Range<DisplayPoint>, Hsla)>)>,
    pub(super) redacted_ranges: Vec<Range<DisplayPoint>>,
}

impl EditorElement {
    pub(super) fn collect_highlight_inputs(
        &self,
        start_anchor: Anchor,
        end_anchor: Anchor,
        start_row: DisplayRow,
        end_row: DisplayRow,
        max_row: DisplayRow,
        row_infos: &[RowInfo],
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> HighlightInputs {
        let mut highlighted_rows = self
            .editor
            .update(cx, |editor, cx| editor.highlighted_display_rows(window, cx));

        let highlighted_ranges = self.collect_background_highlights(
            start_anchor,
            end_anchor,
            start_row,
            end_row,
            max_row,
            snapshot,
            window,
            cx,
        );

        self.add_diff_and_drag_highlights(
            &mut highlighted_rows,
            row_infos,
            start_row,
            snapshot,
            cx,
        );

        let highlighted_gutter_ranges = self.editor.read(cx).gutter_highlights_in_range(
            start_anchor..end_anchor,
            &snapshot.display_snapshot,
            cx,
        );

        let document_colors = self
            .editor
            .read(cx)
            .colors
            .as_ref()
            .map(|colors| colors.editor_display_highlights(snapshot));
        let redacted_ranges = self.editor.read(cx).redacted_ranges(
            start_anchor..end_anchor,
            &snapshot.display_snapshot,
            cx,
        );

        HighlightInputs {
            highlighted_rows,
            highlighted_ranges,
            highlighted_gutter_ranges,
            document_colors,
            redacted_ranges,
        }
    }
}
