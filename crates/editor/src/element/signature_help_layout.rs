use super::*;

impl EditorElement {
    pub(super) fn layout_signature_help(
        &self,
        hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        newest_selection_head: Option<DisplayPoint>,
        start_row: DisplayRow,
        line_layouts: &[LineWithInvisibles],
        line_height: Pixels,
        em_width: Pixels,
        context_menu_layout: Option<ContextMenuLayout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.editor.focus_handle(cx).is_focused(window) {
            return;
        }
        let Some(newest_selection_head) = newest_selection_head else {
            return;
        };

        let max_size = size(
            (120. * em_width) // Default size
                .min(hitbox.size.width / 2.) // Shrink to half of the editor width
                .max(MIN_POPOVER_CHARACTER_WIDTH * em_width), // Apply minimum width of 20 characters
            (16. * line_height) // Default size
                .min(hitbox.size.height / 2.) // Shrink to half of the editor height
                .max(MIN_POPOVER_LINE_HEIGHT * line_height), // Apply minimum height of 4 lines
        );

        let maybe_element = self.editor.update(cx, |editor, cx| {
            if let Some(popover) = editor.signature_help_state.popover_mut() {
                let element = popover.render(max_size, window, cx);
                Some(element)
            } else {
                None
            }
        });
        let Some(mut element) = maybe_element else {
            return;
        };

        let selection_row = newest_selection_head.row();
        let Some(cursor_row_layout) = (selection_row >= start_row)
            .then(|| line_layouts.get(selection_row.minus(start_row) as usize))
            .flatten()
        else {
            return;
        };

        let target_x = cursor_row_layout.x_for_index(newest_selection_head.column() as usize)
            - Pixels::from(scroll_pixel_position.x);
        let target_y = Pixels::from(
            selection_row.as_f64() * ScrollPixelOffset::from(line_height) - scroll_pixel_position.y,
        );
        let target_point = content_origin + point(target_x, target_y);

        let actual_size = element.layout_as_root(Size::<AvailableSpace>::default(), window, cx);

        let (popover_bounds_above, popover_bounds_below) = {
            let horizontal_offset = (hitbox.top_right().x
                - POPOVER_RIGHT_OFFSET
                - (target_point.x + actual_size.width))
                .min(Pixels::ZERO);
            let initial_x = target_point.x + horizontal_offset;
            (
                Bounds::new(
                    point(initial_x, target_point.y - actual_size.height),
                    actual_size,
                ),
                Bounds::new(
                    point(initial_x, target_point.y + line_height + HOVER_POPOVER_GAP),
                    actual_size,
                ),
            )
        };

        let intersects_menu = |bounds: Bounds<Pixels>| -> bool {
            context_menu_layout
                .as_ref()
                .is_some_and(|menu| bounds.intersects(&menu.bounds))
        };

        let final_origin = if popover_bounds_above.is_contained_within(hitbox)
            && !intersects_menu(popover_bounds_above)
        {
            // try placing above cursor
            popover_bounds_above.origin
        } else if popover_bounds_below.is_contained_within(hitbox)
            && !intersects_menu(popover_bounds_below)
        {
            // try placing below cursor
            popover_bounds_below.origin
        } else {
            // try surrounding context menu if exists
            let origin_surrounding_menu = context_menu_layout.as_ref().and_then(|menu| {
                let y_for_horizontal_positioning = if menu.y_flipped {
                    menu.bounds.bottom() - actual_size.height
                } else {
                    menu.bounds.top()
                };
                let possible_origins = vec![
                    // left of context menu
                    point(
                        menu.bounds.left() - actual_size.width - HOVER_POPOVER_GAP,
                        y_for_horizontal_positioning,
                    ),
                    // right of context menu
                    point(
                        menu.bounds.right() + HOVER_POPOVER_GAP,
                        y_for_horizontal_positioning,
                    ),
                    // top of context menu
                    point(
                        menu.bounds.left(),
                        menu.bounds.top() - actual_size.height - HOVER_POPOVER_GAP,
                    ),
                    // bottom of context menu
                    point(menu.bounds.left(), menu.bounds.bottom() + HOVER_POPOVER_GAP),
                ];
                possible_origins
                    .into_iter()
                    .find(|&origin| Bounds::new(origin, actual_size).is_contained_within(hitbox))
            });
            origin_surrounding_menu.unwrap_or_else(|| {
                // fallback to existing above/below cursor logic
                // this might overlap menu or overflow in rare case
                if popover_bounds_above.is_contained_within(hitbox) {
                    popover_bounds_above.origin
                } else {
                    popover_bounds_below.origin
                }
            })
        };

        window.defer_draw(element, final_origin, 2, None);
    }
}
