use super::*;

impl EditorElement {
    pub(super) fn layout_block_render_phase(
        &self,
        is_minimap: bool,
        rows: Range<DisplayRow>,
        start_anchor: Anchor,
        end_anchor: Anchor,
        scroll_position: gpui::Point<ScrollOffset>,
        em_layout_width: Pixels,
        line_height: Pixels,
        content_origin: gpui::Point<Pixels>,
        text_hitbox: &Hitbox,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        editor_width: Pixels,
        scroll_width: &mut Pixels,
        editor_margins: &EditorMargins,
        em_width: Pixels,
        gutter_full_width: Pixels,
        line_layouts: &mut [LineWithInvisibles],
        local_selections: &[Selection<Point>],
        selected_buffer_ids: &Vec<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::BlockRenderPhase {
        let sticky_header_excerpt_id = if snapshot.buffer_snapshot().show_headers() {
            snapshot
                .sticky_header_excerpt(scroll_position.y)
                .as_ref()
                .map(|top| top.excerpt.buffer_id())
        } else {
            None
        };

        let buffer = snapshot.buffer_snapshot();
        let start_buffer_row = MultiBufferRow(start_anchor.to_point(&buffer).row);
        let end_buffer_row = MultiBufferRow(end_anchor.to_point(&buffer).row);

        let preliminary_scroll_pixel_position = point(
            scroll_position.x * f64::from(em_layout_width),
            scroll_position.y * f64::from(line_height),
        );
        let indent_guides = self.layout_indent_guides(
            content_origin,
            text_hitbox.origin,
            start_buffer_row..end_buffer_row,
            preliminary_scroll_pixel_position,
            line_height,
            snapshot,
            window,
            cx,
        );
        let indent_guides_for_spacers = indent_guides.clone();

        let blocks_output = (!is_minimap)
            .then(|| {
                window.with_element_namespace("blocks", |window| {
                    self.render_blocks(
                        rows,
                        snapshot,
                        hitbox,
                        text_hitbox,
                        editor_width,
                        scroll_width,
                        editor_margins,
                        em_width,
                        gutter_full_width,
                        line_height,
                        line_layouts,
                        local_selections,
                        selected_buffer_ids,
                        latest_selection_anchors,
                        is_row_soft_wrapped,
                        sticky_header_excerpt_id,
                        &indent_guides_for_spacers,
                        window,
                        cx,
                    )
                })
            })
            .unwrap_or_default();

        layout_data::BlockRenderPhase {
            blocks_output,
            sticky_header_excerpt_id,
            start_buffer_row,
            end_buffer_row,
            preliminary_scroll_pixel_position,
            indent_guides,
        }
    }

    pub(super) fn layout_sticky_buffer_header_phase(
        &self,
        sticky_header_excerpt_id: Option<BufferId>,
        scroll_position: gpui::Point<ScrollOffset>,
        line_height: Pixels,
        right_margin: Pixels,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        selected_buffer_ids: &Vec<BufferId>,
        blocks: &[BlockLayout],
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if !self.should_show_buffer_headers() || sticky_header_excerpt_id.is_none() {
            return None;
        }

        snapshot
            .sticky_header_excerpt(scroll_position.y)
            .map(|sticky_header_excerpt| {
                window.with_element_namespace("blocks", |window| {
                    self.layout_sticky_buffer_header(
                        sticky_header_excerpt,
                        scroll_position,
                        line_height,
                        right_margin,
                        snapshot,
                        hitbox,
                        selected_buffer_ids,
                        blocks,
                        latest_selection_anchors,
                        window,
                        cx,
                    )
                })
            })
    }

    pub(super) fn spacer_pattern_period(line_height: f32, target_height: f32) -> f32 {
        let k_approx = line_height / (2.0 * target_height);
        let k_floor = (k_approx.floor() as u32).max(1);
        let k_ceil = (k_approx.ceil() as u32).max(1);

        let size_floor = line_height / (2 * k_floor) as f32;
        let size_ceil = line_height / (2 * k_ceil) as f32;

        if (size_floor - target_height).abs() <= (size_ceil - target_height).abs() {
            size_floor
        } else {
            size_ceil
        }
    }

    pub fn render_spacer_block(
        block_id: BlockId,
        block_height: u32,
        line_height: Pixels,
        indent_guide_padding: Pixels,
        window: &mut Window,
        cx: &App,
    ) -> AnyElement {
        let target_size = 16.0;
        let scale = window.scale_factor();
        let pattern_size =
            Self::spacer_pattern_period(f32::from(line_height) * scale, target_size * scale);
        let color = cx.theme().colors().panel_background;
        let background = pattern_slash(color, 2.0, pattern_size - 2.0);

        div()
            .id(block_id)
            .cursor(CursorStyle::Arrow)
            .w_full()
            .h((block_height as f32) * line_height)
            .flex()
            .flex_row()
            .child(div().flex_shrink_0().w(indent_guide_padding).h_full())
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .relative()
                    .overflow_x_hidden()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .right_0()
                            .left(-indent_guide_padding)
                            .bg(background),
                    ),
            )
            .into_any()
    }

    pub(super) fn render_blocks(
        &self,
        rows: Range<DisplayRow>,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        text_hitbox: &Hitbox,
        editor_width: Pixels,
        scroll_width: &mut Pixels,
        editor_margins: &EditorMargins,
        em_width: Pixels,
        text_x: Pixels,
        line_height: Pixels,
        line_layouts: &mut [LineWithInvisibles],
        selections: &[Selection<Point>],
        selected_buffer_ids: &Vec<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        sticky_header_excerpt_id: Option<BufferId>,
        indent_guides: &Option<Vec<IndentGuideLayout>>,
        window: &mut Window,
        cx: &mut App,
    ) -> RenderBlocksOutput {
        let (fixed_blocks, non_fixed_blocks) = snapshot
            .blocks_in_range(rows.clone())
            .partition::<Vec<_>, _>(|(_, block)| block.style() == BlockStyle::Fixed);

        let mut focused_block = self
            .editor
            .update(cx, |editor, _| editor.take_focused_block());
        let mut fixed_block_max_width = Pixels::ZERO;
        let mut blocks = Vec::new();
        let mut spacer_blocks = Vec::new();
        let mut resized_blocks = HashMap::default();
        let mut row_block_types = HashMap::default();
        let mut block_resize_offset: i32 = 0;

        for (row, block) in fixed_blocks {
            let block_id = block.id();

            if focused_block.as_ref().is_some_and(|b| b.id == block_id) {
                focused_block = None;
            }

            if let Some((element, element_size, row, x_offset)) = self.render_block(
                block,
                AvailableSpace::MinContent,
                block_id,
                row,
                snapshot,
                text_x,
                &rows,
                line_layouts,
                editor_margins,
                line_height,
                em_width,
                text_hitbox,
                editor_width,
                scroll_width,
                &mut resized_blocks,
                &mut row_block_types,
                selections,
                selected_buffer_ids,
                latest_selection_anchors,
                is_row_soft_wrapped,
                sticky_header_excerpt_id,
                indent_guides,
                &mut block_resize_offset,
                window,
                cx,
            ) {
                fixed_block_max_width = fixed_block_max_width.max(element_size.width + em_width);
                blocks.push(BlockLayout {
                    id: block_id,
                    x_offset,
                    row: Some(row),
                    element,
                    available_space: size(AvailableSpace::MinContent, element_size.height.into()),
                    style: BlockStyle::Fixed,
                    overlaps_gutter: true,
                    is_buffer_header: block.is_buffer_header(),
                });
            }
        }

        for (row, block) in non_fixed_blocks {
            let style = block.style();
            let width = match (style, block.place_near()) {
                (_, true) => AvailableSpace::MinContent,
                (BlockStyle::Sticky, _) => hitbox.size.width.into(),
                (BlockStyle::Flex, _) => hitbox
                    .size
                    .width
                    .max(fixed_block_max_width)
                    .max(
                        editor_margins.gutter.width + *scroll_width + editor_margins.extended_right,
                    )
                    .into(),
                (BlockStyle::Spacer, _) => hitbox
                    .size
                    .width
                    .max(fixed_block_max_width)
                    .max(*scroll_width + editor_margins.extended_right)
                    .into(),
                (BlockStyle::Fixed, _) => unreachable!(),
            };
            let block_id = block.id();

            if focused_block.as_ref().is_some_and(|b| b.id == block_id) {
                focused_block = None;
            }

            if let Some((element, element_size, row, x_offset)) = self.render_block(
                block,
                width,
                block_id,
                row,
                snapshot,
                text_x,
                &rows,
                line_layouts,
                editor_margins,
                line_height,
                em_width,
                text_hitbox,
                editor_width,
                scroll_width,
                &mut resized_blocks,
                &mut row_block_types,
                selections,
                selected_buffer_ids,
                latest_selection_anchors,
                is_row_soft_wrapped,
                sticky_header_excerpt_id,
                indent_guides,
                &mut block_resize_offset,
                window,
                cx,
            ) {
                let layout = BlockLayout {
                    id: block_id,
                    x_offset,
                    row: Some(row),
                    element,
                    available_space: size(width, element_size.height.into()),
                    style,
                    overlaps_gutter: !block.place_near() && style != BlockStyle::Spacer,
                    is_buffer_header: block.is_buffer_header(),
                };
                if style == BlockStyle::Spacer {
                    spacer_blocks.push(layout);
                } else {
                    blocks.push(layout);
                }
            }
        }

        if let Some(focused_block) = focused_block
            && let Some(focus_handle) = focused_block.focus_handle.upgrade()
            && focus_handle.is_focused(window)
            && let Some(block) = snapshot.block_for_id(focused_block.id)
        {
            let style = block.style();
            let width = match style {
                BlockStyle::Fixed => AvailableSpace::MinContent,
                BlockStyle::Flex => {
                    AvailableSpace::Definite(hitbox.size.width.max(fixed_block_max_width).max(
                        editor_margins.gutter.width + *scroll_width + editor_margins.extended_right,
                    ))
                }
                BlockStyle::Spacer => AvailableSpace::Definite(
                    hitbox
                        .size
                        .width
                        .max(fixed_block_max_width)
                        .max(*scroll_width + editor_margins.extended_right),
                ),
                BlockStyle::Sticky => AvailableSpace::Definite(hitbox.size.width),
            };

            if let Some((element, element_size, _, x_offset)) = self.render_block(
                &block,
                width,
                focused_block.id,
                rows.end,
                snapshot,
                text_x,
                &rows,
                line_layouts,
                editor_margins,
                line_height,
                em_width,
                text_hitbox,
                editor_width,
                scroll_width,
                &mut resized_blocks,
                &mut row_block_types,
                selections,
                selected_buffer_ids,
                latest_selection_anchors,
                is_row_soft_wrapped,
                sticky_header_excerpt_id,
                indent_guides,
                &mut block_resize_offset,
                window,
                cx,
            ) {
                blocks.push(BlockLayout {
                    id: block.id(),
                    x_offset,
                    row: None,
                    element,
                    available_space: size(width, element_size.height.into()),
                    style,
                    overlaps_gutter: true,
                    is_buffer_header: block.is_buffer_header(),
                });
            }
        }

        if resized_blocks.is_empty() {
            *scroll_width =
                (*scroll_width).max(fixed_block_max_width - editor_margins.gutter.width);
        }

        RenderBlocksOutput {
            non_spacer_blocks: blocks,
            spacer_blocks,
            row_block_types,
            resized_blocks: (!resized_blocks.is_empty()).then_some(resized_blocks),
        }
    }

    pub(super) fn layout_blocks(
        &self,
        blocks: &mut Vec<BlockLayout>,
        hitbox: &Hitbox,
        gutter_hitbox: &Hitbox,
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        editor_margins: &EditorMargins,
        window: &mut Window,
        cx: &mut App,
    ) {
        for block in blocks {
            let mut origin = if let Some(row) = block.row {
                hitbox.origin
                    + point(
                        block.x_offset,
                        Pixels::from(
                            (row.as_f64() - scroll_position.y)
                                * ScrollPixelOffset::from(line_height),
                        ),
                    )
            } else {
                // Position the block outside the visible area
                hitbox.origin + point(Pixels::ZERO, hitbox.size.height)
            };

            if block.style == BlockStyle::Spacer {
                origin += point(
                    gutter_hitbox.size.width + editor_margins.gutter.margin,
                    Pixels::ZERO,
                );
            }

            if !matches!(block.style, BlockStyle::Sticky) {
                origin += point(Pixels::from(-scroll_pixel_position.x), Pixels::ZERO);
            }

            let focus_handle =
                block
                    .element
                    .prepaint_as_root(origin, block.available_space, window, cx);

            if let Some(focus_handle) = focus_handle {
                self.editor.update(cx, |editor, _cx| {
                    editor.set_focused_block(FocusedBlock {
                        id: block.id,
                        focus_handle: focus_handle.downgrade(),
                    });
                });
            }
        }
    }
}
