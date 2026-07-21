use super::*;

impl EditorElement {
    pub(super) fn layout_line_setup(
        &self,
        gutter: &Gutter,
        active_rows: &BTreeMap<DisplayRow, LineHighlightSpec>,
        current_selection_head: Option<DisplayRow>,
        gutter_hitbox: &Hitbox,
        gutter_dimensions: GutterDimensions,
        em_width: Pixels,
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        rows: Range<DisplayRow>,
        row_infos: &[RowInfo],
        snapshot: &EditorSnapshot,
        highlighted_ranges: &mut Vec<(Range<DisplayPoint>, Hsla)>,
        selections: &[(PlayerColor, Vec<SelectionLayout>)],
        document_colors: Option<&(DocumentColorsRenderMode, Vec<(Range<DisplayPoint>, Hsla)>)>,
        editor_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::LineSetupLayouts {
        let line_numbers =
            self.layout_line_numbers(gutter, active_rows, current_selection_head, window, cx);

        let expand_toggles = window.with_element_namespace("expand_toggles", |window| {
            self.layout_expand_toggles(
                gutter_hitbox,
                gutter_dimensions,
                em_width,
                line_height,
                scroll_position,
                rows.start,
                row_infos,
                window,
                cx,
            )
        });

        let crease_toggles = window.with_element_namespace("crease_toggles", |window| {
            self.layout_crease_toggles(rows.clone(), row_infos, active_rows, snapshot, window, cx)
        });
        let crease_trailers = window.with_element_namespace("crease_trailers", |window| {
            self.layout_crease_trailers(row_infos.iter().cloned(), snapshot, window, cx)
        });

        let display_hunks = self.layout_gutter_diff_hunks(
            line_height,
            gutter_hitbox,
            rows.clone(),
            snapshot,
            scroll_position,
            window,
            cx,
        );

        Self::layout_word_diff_highlights(
            &display_hunks,
            row_infos,
            rows.start,
            snapshot,
            highlighted_ranges,
            cx,
        );

        let bg_segments_per_row = Self::bg_segments_per_row(
            rows.clone(),
            selections,
            highlighted_ranges.iter().cloned().chain(
                document_colors
                    .iter()
                    .flat_map(|(_, colors)| colors.iter().cloned()),
            ),
            self.style.background,
        );

        let line_layouts = Self::layout_lines(
            rows,
            snapshot,
            &self.style,
            editor_width,
            is_row_soft_wrapped,
            &bg_segments_per_row,
            window,
            cx,
        );

        layout_data::LineSetupLayouts {
            line_numbers,
            expand_toggles,
            crease_toggles,
            crease_trailers,
            display_hunks,
            line_layouts,
        }
    }
}
