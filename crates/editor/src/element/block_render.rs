use super::*;

impl EditorElement {
    pub(super) fn render_block(
        &self,
        block: &Block,
        available_width: AvailableSpace,
        block_id: BlockId,
        block_row_start: DisplayRow,
        snapshot: &EditorSnapshot,
        text_x: Pixels,
        rows: &Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        editor_margins: &EditorMargins,
        line_height: Pixels,
        em_width: Pixels,
        text_hitbox: &Hitbox,
        editor_width: Pixels,
        scroll_width: &mut Pixels,
        resized_blocks: &mut HashMap<CustomBlockId, u32>,
        row_block_types: &mut HashMap<DisplayRow, bool>,
        selections: &[Selection<Point>],
        selected_buffer_ids: &Vec<BufferId>,
        latest_selection_anchors: &HashMap<BufferId, Anchor>,
        is_row_soft_wrapped: impl Copy + Fn(usize) -> bool,
        sticky_header_excerpt_id: Option<BufferId>,
        indent_guides: &Option<Vec<IndentGuideLayout>>,
        block_resize_offset: &mut i32,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, Size<Pixels>, DisplayRow, Pixels)> {
        let mut x_position = None;
        let mut element = match block {
            Block::Custom(custom) => {
                let block_start = custom.start().to_point(&snapshot.buffer_snapshot());
                let block_end = custom.end().to_point(&snapshot.buffer_snapshot());
                if block.place_near() && snapshot.is_line_folded(MultiBufferRow(block_start.row)) {
                    return None;
                }
                let align_to = block_start.to_display_point(snapshot);
                let x_and_width = |layout: &LineWithInvisibles| {
                    Some((
                        text_x + layout.x_for_index(align_to.column() as usize),
                        text_x + layout.width,
                    ))
                };
                let line_ix = align_to.row().0.checked_sub(rows.start.0);
                x_position =
                    if let Some(layout) = line_ix.and_then(|ix| line_layouts.get(ix as usize)) {
                        x_and_width(layout)
                    } else {
                        x_and_width(&layout_line(
                            align_to.row(),
                            snapshot,
                            &self.style,
                            editor_width,
                            is_row_soft_wrapped,
                            window,
                            cx,
                        ))
                    };

                let anchor_x = x_position.unwrap().0;

                let selected = selections
                    .binary_search_by(|selection| {
                        if selection.end <= block_start {
                            Ordering::Less
                        } else if selection.start >= block_end {
                            Ordering::Greater
                        } else {
                            Ordering::Equal
                        }
                    })
                    .is_ok();

                div()
                    .size_full()
                    .child(
                        custom.render(&mut BlockContext {
                            window,
                            app: cx,
                            anchor_x,
                            margins: editor_margins,
                            line_height,
                            em_width,
                            block_id,
                            height: custom.height.unwrap_or(1),
                            selected,
                            max_width: text_hitbox.size.width.max(*scroll_width),
                            editor_style: &self.style,
                            indent_guide_padding: indent_guides
                                .as_ref()
                                .map(|guides| {
                                    Self::depth_zero_indent_guide_padding_for_row(
                                        guides,
                                        block_row_start,
                                    )
                                })
                                .unwrap_or(px(0.0)),
                        }),
                    )
                    .into_any()
            }

            Block::FoldedBuffer {
                first_excerpt,
                height,
                ..
            } => {
                let mut result = v_flex().id(block_id).w_full().pr(editor_margins.right);

                if self.should_show_buffer_headers() {
                    let selected = selected_buffer_ids.contains(&first_excerpt.buffer_id());
                    let jump_data = header::header_jump_data(
                        snapshot,
                        block_row_start,
                        *height,
                        first_excerpt,
                        latest_selection_anchors,
                    );
                    result = result.child(header::render_buffer_header(
                        &self.editor,
                        first_excerpt,
                        true,
                        selected,
                        false,
                        jump_data,
                        window,
                        cx,
                    ));
                } else {
                    result =
                        result.child(div().h(FILE_HEADER_HEIGHT as f32 * window.line_height()));
                }

                result.into_any_element()
            }

            Block::ExcerptBoundary { .. } => {
                let color = cx.theme().colors().clone();
                let mut result = v_flex().id(block_id).w_full();

                result = result.child(
                    h_flex().relative().child(
                        div()
                            .top(line_height / 2.)
                            .absolute()
                            .w_full()
                            .h_px()
                            .bg(color.border_variant),
                    ),
                );

                result.into_any()
            }

            Block::BufferHeader { excerpt, height } => {
                let mut result = v_flex().id(block_id).w_full();

                if self.should_show_buffer_headers() {
                    let jump_data = header::header_jump_data(
                        snapshot,
                        block_row_start,
                        *height,
                        excerpt,
                        latest_selection_anchors,
                    );

                    if sticky_header_excerpt_id != Some(excerpt.buffer_id()) {
                        let selected = selected_buffer_ids.contains(&excerpt.buffer_id());

                        result = result.child(div().pr(editor_margins.right).child(
                            header::render_buffer_header(
                                &self.editor,
                                excerpt,
                                false,
                                selected,
                                false,
                                jump_data,
                                window,
                                cx,
                            ),
                        ));
                    } else {
                        result =
                            result.child(div().h(FILE_HEADER_HEIGHT as f32 * window.line_height()));
                    }
                } else {
                    result =
                        result.child(div().h(FILE_HEADER_HEIGHT as f32 * window.line_height()));
                }

                result.into_any()
            }

            Block::Spacer { height, .. } => {
                let indent_guide_padding = indent_guides
                    .as_ref()
                    .map(|guides| {
                        Self::depth_zero_indent_guide_padding_for_row(guides, block_row_start)
                    })
                    .unwrap_or(px(0.0));
                Self::render_spacer_block(
                    block_id,
                    *height,
                    line_height,
                    indent_guide_padding,
                    window,
                    cx,
                )
            }
        };

        // Discover the element's content height, then round up to the nearest multiple of line height.
        let preliminary_size = element.layout_as_root(
            size(available_width, AvailableSpace::MinContent),
            window,
            cx,
        );
        let quantized_height = (preliminary_size.height / line_height).ceil() * line_height;
        let final_size = if preliminary_size.height == quantized_height {
            preliminary_size
        } else {
            element.layout_as_root(size(available_width, quantized_height.into()), window, cx)
        };
        let mut element_height_in_lines = ((final_size.height / line_height).ceil() as u32).max(1);

        let effective_row_start = block_row_start.0 as i32 + *block_resize_offset;
        debug_assert!(effective_row_start >= 0);
        let mut row = DisplayRow(effective_row_start.max(0) as u32);

        let mut x_offset = px(0.);
        let mut is_block = true;

        if let BlockId::Custom(custom_block_id) = block_id
            && block.has_height()
        {
            if block.place_near()
                && let Some((x_target, line_width)) = x_position
            {
                let margin = em_width * 2;
                if line_width + final_size.width + margin
                    < editor_width + editor_margins.gutter.full_width()
                    && !row_block_types.contains_key(&(row - 1))
                    && element_height_in_lines == 1
                {
                    // Render inline at end of line (for diagnostic blocks that fit)
                    x_offset = line_width + margin;
                    row = row - 1;
                    is_block = false;
                    element_height_in_lines = 0;
                    row_block_types.insert(row, is_block);
                } else {
                    let max_offset =
                        editor_width + editor_margins.gutter.full_width() - final_size.width;
                    let min_offset = (x_target + em_width - final_size.width)
                        .max(editor_margins.gutter.full_width());
                    x_offset = x_target.min(max_offset).max(min_offset);
                }
            };
            if element_height_in_lines != block.height() {
                *block_resize_offset += element_height_in_lines as i32 - block.height() as i32;
                resized_blocks.insert(custom_block_id, element_height_in_lines);
            }
        }
        for i in 0..element_height_in_lines {
            row_block_types.insert(row + i, is_block);
        }

        Some((element, final_size, row, x_offset))
    }
}
