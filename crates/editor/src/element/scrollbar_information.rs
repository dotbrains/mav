use super::*;

impl EditorElement {
    pub(super) fn layout_scrollbar_information(
        &self,
        snapshot: &EditorSnapshot,
        text_bounds: Bounds<Pixels>,
        glyph_grid_cell: Size<Pixels>,
        max_row: DisplayRow,
        line_height: Pixels,
        em_advance: Pixels,
        editor_width: Pixels,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        scroll_beyond_last_line: ScrollBeyondLastLine,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> ScrollbarLayoutInformation {
        let longest_line_blame_width = self
            .editor
            .update(cx, |editor, cx| {
                if !editor.show_git_blame_inline {
                    return None;
                }
                let blame = editor.blame.as_ref()?;
                let (_, blame_entry) = blame
                    .update(cx, |blame, cx| {
                        let row_infos = snapshot.row_infos(snapshot.longest_row()).next()?;
                        blame.blame_for_rows(&[row_infos], cx).next()
                    })
                    .flatten()?;
                let mut element = render_inline_blame_entry(blame_entry, style, cx)?;
                let inline_blame_padding =
                    ProjectSettings::get_global(cx).git.inline_blame.padding as f32 * em_advance;
                Some(
                    element
                        .layout_as_root(AvailableSpace::min_size(), window, cx)
                        .width
                        + inline_blame_padding,
                )
            })
            .unwrap_or(Pixels::ZERO);

        let longest_line_width = layout_line(
            snapshot.longest_row(),
            snapshot,
            style,
            editor_width,
            is_row_soft_wrapped,
            window,
            cx,
        )
        .width;

        ScrollbarLayoutInformation::new(
            text_bounds,
            glyph_grid_cell,
            size(
                longest_line_width,
                Pixels::from(max_row.as_f64() * f64::from(line_height)),
            ),
            longest_line_blame_width,
            EditorSettings::get_global(cx),
            scroll_beyond_last_line,
        )
    }
}
