use super::*;

pub(super) struct PostScrollPrepaintLayouts {
    pub(super) crease_trailers: Vec<Option<CreaseTrailerLayout>>,
    pub(super) edit_prediction_popover: Option<AnyElement>,
    pub(super) inline_diagnostics: HashMap<DisplayRow, AnyElement>,
    pub(super) inline_blame_layout: Option<InlineBlameLayout>,
    pub(super) inline_code_actions: Option<AnyElement>,
    pub(super) blamed_display_rows: Option<Vec<AnyElement>>,
    pub(super) line_elements: SmallVec<[AnyElement; 1]>,
    pub(super) blocks: Vec<BlockLayout>,
    pub(super) spacer_blocks: Vec<BlockLayout>,
    pub(super) line_layouts: Vec<LineWithInvisibles>,
}

impl EditorElement {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_post_scroll_prepaint(
        &self,
        crease_trailers: Vec<Option<AnyElement>>,
        mut blocks: Vec<BlockLayout>,
        mut spacer_blocks: Vec<BlockLayout>,
        mut line_layouts: Vec<LineWithInvisibles>,
        row_block_types: &HashMap<DisplayRow, bool>,
        row_infos: &[RowInfo],
        content_origin: gpui::Point<Pixels>,
        text_hitbox: &Hitbox,
        right_margin: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        newest_selection_head: Option<DisplayPoint>,
        start_row: DisplayRow,
        end_row: DisplayRow,
        height_in_lines: f64,
        line_height: Pixels,
        em_width: Pixels,
        style: &EditorStyle,
        snapshot: &EditorSnapshot,
        editor_width: Pixels,
        gutter_hitbox: &Hitbox,
        git_blame_entries_width: Option<Pixels>,
        hitbox: &Hitbox,
        editor_margins: &EditorMargins,
        window: &mut Window,
        cx: &mut App,
    ) -> PostScrollPrepaintLayouts {
        let crease_trailers = window.with_element_namespace("crease_trailers", |window| {
            self.prepaint_crease_trailers(
                crease_trailers,
                &line_layouts,
                line_height,
                content_origin,
                scroll_pixel_position,
                scroll_position,
                start_row,
                em_width,
                window,
                cx,
            )
        });

        let (edit_prediction_popover, edit_prediction_popover_origin) = self
            .editor
            .update(cx, |editor, cx| {
                editor.render_edit_prediction_popover(
                    &text_hitbox.bounds,
                    content_origin,
                    right_margin,
                    snapshot,
                    start_row..end_row,
                    scroll_position.y,
                    scroll_position.y + height_in_lines,
                    &line_layouts,
                    line_height,
                    scroll_position,
                    scroll_pixel_position,
                    newest_selection_head,
                    editor_width,
                    style,
                    window,
                    cx,
                )
            })
            .unzip();

        let layout_data::InlineDecorationLayouts {
            inline_diagnostics,
            inline_blame_layout,
            inline_code_actions,
        } = self.layout_inline_decorations(
            &line_layouts,
            &crease_trailers,
            row_block_types,
            row_infos,
            content_origin,
            scroll_position,
            scroll_pixel_position,
            edit_prediction_popover_origin,
            newest_selection_head,
            start_row,
            end_row,
            line_height,
            em_width,
            style,
            snapshot,
            window,
            cx,
        );

        let blamed_display_rows = self.layout_blame_entries(
            row_infos,
            em_width,
            scroll_position,
            start_row,
            line_height,
            gutter_hitbox,
            git_blame_entries_width,
            window,
            cx,
        );

        let line_elements = self.prepaint_lines(
            start_row,
            &mut line_layouts,
            line_height,
            scroll_position,
            scroll_pixel_position,
            content_origin,
            window,
            cx,
        );

        window.with_element_namespace("blocks", |window| {
            self.layout_blocks(
                &mut blocks,
                hitbox,
                gutter_hitbox,
                line_height,
                scroll_position,
                scroll_pixel_position,
                editor_margins,
                window,
                cx,
            );
            self.layout_blocks(
                &mut spacer_blocks,
                hitbox,
                gutter_hitbox,
                line_height,
                scroll_position,
                scroll_pixel_position,
                editor_margins,
                window,
                cx,
            );
        });

        PostScrollPrepaintLayouts {
            crease_trailers,
            edit_prediction_popover,
            inline_diagnostics,
            inline_blame_layout,
            inline_code_actions,
            blamed_display_rows,
            line_elements,
            blocks,
            spacer_blocks,
            line_layouts,
        }
    }
}
