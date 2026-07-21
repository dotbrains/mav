use super::*;

pub(super) struct PreScrollLayouts {
    pub(super) scrollbar_layout_information: ScrollbarLayoutInformation,
    pub(super) blocks: Vec<BlockLayout>,
    pub(super) spacer_blocks: Vec<BlockLayout>,
    pub(super) row_block_types: HashMap<DisplayRow, bool>,
    pub(super) sticky_header_excerpt_id: Option<BufferId>,
    pub(super) sticky_buffer_header: Option<AnyElement>,
    pub(super) scroll_position: gpui::Point<ScrollOffset>,
    pub(super) scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
    pub(super) scroll_max: gpui::Point<ScrollPixelOffset>,
    pub(super) sticky_headers: Option<header::StickyHeaders>,
    pub(super) indent_guides: Option<Vec<IndentGuideLayout>>,
}

impl EditorElement {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn layout_pre_scroll_phase(
        &self,
        is_minimap: bool,
        is_singleton: bool,
        max_row: DisplayRow,
        visible_row_range: Range<DisplayRow>,
        start_anchor: Anchor,
        end_anchor: Anchor,
        current_selection_head: Option<DisplayRow>,
        scroll_position: gpui::Point<ScrollOffset>,
        max_scroll_top: f64,
        glyph_grid_cell: Size<Pixels>,
        em_advance: Pixels,
        em_layout_width: Pixels,
        line_height: Pixels,
        content_origin: gpui::Point<Pixels>,
        text_hitbox: &Hitbox,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        editor_width: Pixels,
        editor_margins: &EditorMargins,
        em_width: Pixels,
        gutter_dimensions: GutterDimensions,
        gutter_hitbox: &Hitbox,
        right_margin: Pixels,
        line_layouts: &mut Vec<LineWithInvisibles>,
        local_selections: &[Selection<Point>],
        selected_buffer_ids: &Vec<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        scroll_beyond_last_line: ScrollBeyondLastLine,
        needs_horizontal_autoscroll: NeedsHorizontalAutoscroll,
        autoscroll_request: Option<(Autoscroll, bool)>,
        request_layout: &mut EditorRequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<PreScrollLayouts> {
        let scrollbar_layout_information = self.layout_scrollbar_information(
            snapshot,
            text_hitbox.bounds,
            glyph_grid_cell,
            max_row,
            line_height,
            em_advance,
            editor_width,
            is_row_soft_wrapped,
            scroll_beyond_last_line,
            &self.style,
            window,
            cx,
        );

        let mut scroll_width = scrollbar_layout_information.scroll_range.width;

        let layout_data::BlockRenderPhase {
            blocks_output,
            sticky_header_excerpt_id,
            start_buffer_row,
            end_buffer_row,
            preliminary_scroll_pixel_position,
            indent_guides,
        } = self.layout_block_render_phase(
            is_minimap,
            visible_row_range.clone(),
            start_anchor,
            end_anchor,
            scroll_position,
            em_layout_width,
            line_height,
            content_origin,
            text_hitbox,
            snapshot,
            hitbox,
            editor_width,
            &mut scroll_width,
            editor_margins,
            em_width,
            gutter_dimensions.full_width(),
            line_layouts,
            local_selections,
            selected_buffer_ids,
            latest_selection_anchors,
            is_row_soft_wrapped,
            window,
            cx,
        );
        let RenderBlocksOutput {
            non_spacer_blocks: blocks,
            spacer_blocks,
            row_block_types,
            resized_blocks,
        } = blocks_output;
        if let Some(resized_blocks) = resized_blocks {
            if request_layout.has_remaining_prepaint_depth() {
                self.editor.update(cx, |editor, cx| {
                    editor.resize_blocks(
                        resized_blocks,
                        autoscroll_request.map(|(autoscroll, _)| autoscroll),
                        cx,
                    )
                });
                return None;
            } else {
                debug_panic!(
                    "dropping block resize because prepaint depth \
                     limit was reached"
                );
            }
        }

        let sticky_buffer_header = self.layout_sticky_buffer_header_phase(
            sticky_header_excerpt_id,
            scroll_position,
            line_height,
            right_margin,
            snapshot,
            hitbox,
            selected_buffer_ids,
            &blocks,
            latest_selection_anchors,
            window,
            cx,
        );

        let layout_data::ScrollPositionLayout {
            scroll_position,
            scroll_pixel_position,
            scroll_max,
        } = self.layout_scroll_position(
            scroll_position,
            max_scroll_top,
            visible_row_range.start,
            editor_width,
            scroll_width,
            em_advance,
            em_layout_width,
            line_height,
            line_layouts,
            needs_horizontal_autoscroll,
            autoscroll_request,
            window,
            cx,
        );
        let layout_data::StickyHeaderLayouts {
            sticky_headers,
            indent_guides,
        } = self.layout_sticky_headers_and_guides(
            is_minimap,
            is_singleton,
            snapshot,
            editor_width,
            is_row_soft_wrapped,
            line_height,
            scroll_pixel_position,
            preliminary_scroll_pixel_position,
            content_origin,
            &gutter_dimensions,
            gutter_hitbox,
            text_hitbox,
            current_selection_head,
            start_buffer_row..end_buffer_row,
            indent_guides,
            window,
            cx,
        );

        Some(PreScrollLayouts {
            scrollbar_layout_information,
            blocks,
            spacer_blocks,
            row_block_types,
            sticky_header_excerpt_id,
            sticky_buffer_header,
            scroll_position,
            scroll_pixel_position,
            scroll_max,
            sticky_headers,
            indent_guides,
        })
    }
}
