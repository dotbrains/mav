use super::*;

impl EditorElement {
    pub(super) fn layout_gutter_indicators(
        &self,
        gutter: &Gutter,
        rows: Range<DisplayRow>,
        row_infos: &[RowInfo],
        snapshot: &EditorSnapshot,
        run_indicator_rows: &HashSet<DisplayRow>,
        breakpoint_rows: &mut HashMap<
            DisplayRow,
            (Anchor, Breakpoint, Option<BreakpointSessionState>),
        >,
        gutter_settings: crate::editor_settings::Gutter,
        gutter_dimensions: GutterDimensions,
        line_height: Pixels,
        em_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::GutterIndicatorLayouts {
        let test_indicators = if gutter_settings.runnables {
            self.layout_run_indicators(gutter, run_indicator_rows, breakpoint_rows, window, cx)
        } else {
            Vec::new()
        };

        let show_bookmarks = snapshot.show_bookmarks.unwrap_or(gutter_settings.bookmarks);
        let bookmark_rows = self.editor.update(cx, |editor, cx| {
            let mut rows = editor.active_bookmarks(rows.clone(), window, cx);
            rows.retain(|k| !run_indicator_rows.contains(k));
            rows.retain(|k| !breakpoint_rows.contains_key(k));
            rows
        });
        let bookmarks = if show_bookmarks {
            self.layout_bookmarks(gutter, &bookmark_rows, window, cx)
        } else {
            Vec::new()
        };

        let show_breakpoints = snapshot
            .show_breakpoints
            .unwrap_or(gutter_settings.breakpoints);
        breakpoint_rows.retain(|k, _| !run_indicator_rows.contains(k));
        let mut breakpoints = if show_breakpoints {
            self.layout_breakpoints(gutter, breakpoint_rows, window, cx)
        } else {
            Vec::new()
        };

        let gutter_hover_button = self
            .editor
            .read(cx)
            .gutter_hover_button
            .0
            .filter(|phantom| phantom.is_active)
            .map(|phantom| phantom.display_row);

        if let Some(row) = gutter_hover_button
            && !breakpoint_rows.contains_key(&row)
            && !run_indicator_rows.contains(&row)
            && !bookmark_rows.contains(&row)
            && (show_bookmarks || show_breakpoints)
        {
            let position = snapshot.display_point_to_anchor(DisplayPoint::new(row, 0), Bias::Right);
            breakpoints.extend(self.layout_gutter_hover_button(gutter, position, row, window, cx));
        }

        let git_gutter_width = Self::gutter_strip_width(line_height)
            + gutter_dimensions
                .git_blame_entries_width
                .unwrap_or_default();
        let available_width = gutter_dimensions.left_padding - git_gutter_width;

        let max_line_number_length = self
            .editor
            .read(cx)
            .buffer()
            .read(cx)
            .snapshot(cx)
            .widest_line_number()
            .ilog10()
            + 1;

        let diff_review_button = self
            .should_render_diff_review_button(rows, row_infos, snapshot, cx)
            .map(|(display_row, buffer_row)| {
                let is_wide = max_line_number_length
                    >= EditorSettings::get_global(cx).gutter.min_line_number_digits as u32
                    && buffer_row
                        .is_some_and(|row| (row + 1).ilog10() + 1 == max_line_number_length)
                    || gutter_dimensions.right_padding == px(0.);

                let button_width = if is_wide {
                    available_width - px(6.)
                } else {
                    available_width + em_width - px(6.)
                };

                let button = self.editor.update(cx, |editor, cx| {
                    editor
                        .render_diff_review_button(display_row, button_width, cx)
                        .into_any_element()
                });
                gutter.prepaint_button(button, display_row, window, cx)
            });

        layout_data::GutterIndicatorLayouts {
            test_indicators,
            bookmarks,
            breakpoints,
            diff_review_button,
        }
    }
}
