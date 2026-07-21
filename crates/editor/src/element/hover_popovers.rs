use super::*;

impl EditorElement {
    pub(super) fn layout_hover_popovers(
        &self,
        snapshot: &EditorSnapshot,
        hitbox: &Hitbox,
        visible_display_row_range: Range<DisplayRow>,
        content_origin: gpui::Point<Pixels>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_layouts: &[LineWithInvisibles],
        line_height: Pixels,
        em_width: Pixels,
        context_menu_layout: Option<ContextMenuLayout>,
        window: &mut Window,
        cx: &mut App,
    ) {
        struct MeasuredHoverPopover {
            element: AnyElement,
            size: Size<Pixels>,
            horizontal_offset: Pixels,
        }

        let max_size = size(
            (120. * em_width) // Default size
                .min(hitbox.size.width / 2.) // Shrink to half of the editor width
                .max(MIN_POPOVER_CHARACTER_WIDTH * em_width), // Apply minimum width of 20 characters
            (16. * line_height) // Default size
                .min(hitbox.size.height / 2.) // Shrink to half of the editor height
                .max(MIN_POPOVER_LINE_HEIGHT * line_height), // Apply minimum height of 4 lines
        );

        // Don't show hover popovers when context menu is open to avoid overlap
        let has_context_menu = self.editor.read(cx).mouse_context_menu.is_some();
        if has_context_menu {
            return;
        }

        let hover_popovers = self.editor.update(cx, |editor, cx| {
            editor.hover_state.render(
                snapshot,
                visible_display_row_range.clone(),
                max_size,
                &editor.text_layout_details(window, cx),
                window,
                cx,
            )
        });
        let Some((popover_position, hover_popovers)) = hover_popovers else {
            return;
        };

        // This is safe because we check on layout whether the required row is available
        let hovered_row_layout = &line_layouts[popover_position
            .row()
            .minus(visible_display_row_range.start)
            as usize];

        // Compute Hovered Point
        let x = hovered_row_layout.x_for_index(popover_position.column() as usize)
            - Pixels::from(scroll_pixel_position.x);
        let y = Pixels::from(
            popover_position.row().as_f64() * ScrollPixelOffset::from(line_height)
                - scroll_pixel_position.y,
        );
        let hovered_point = content_origin + point(x, y);

        let mut overall_height = Pixels::ZERO;
        let mut measured_hover_popovers = Vec::new();
        for (position, mut hover_popover) in hover_popovers.into_iter().with_position() {
            let size = hover_popover.layout_as_root(AvailableSpace::min_size(), window, cx);
            let horizontal_offset =
                (hitbox.top_right().x - POPOVER_RIGHT_OFFSET - (hovered_point.x + size.width))
                    .min(Pixels::ZERO);
            match position {
                itertools::Position::Middle | itertools::Position::Last => {
                    overall_height += HOVER_POPOVER_GAP
                }
                _ => {}
            }
            overall_height += size.height;
            measured_hover_popovers.push(MeasuredHoverPopover {
                element: hover_popover,
                size,
                horizontal_offset,
            });
        }

        fn draw_occluder(
            width: Pixels,
            origin: gpui::Point<Pixels>,
            window: &mut Window,
            cx: &mut App,
        ) {
            let mut occlusion = div()
                .size_full()
                .occlude()
                .on_mouse_move(|_, _, cx| cx.stop_propagation())
                .into_any_element();
            occlusion.layout_as_root(size(width, HOVER_POPOVER_GAP).into(), window, cx);
            window.defer_draw(occlusion, origin, 2, None);
        }

        fn place_popovers_above(
            hovered_point: gpui::Point<Pixels>,
            measured_hover_popovers: Vec<MeasuredHoverPopover>,
            window: &mut Window,
            cx: &mut App,
        ) {
            let mut current_y = hovered_point.y;
            for (position, popover) in measured_hover_popovers.into_iter().with_position() {
                let size = popover.size;
                let popover_origin = point(
                    hovered_point.x + popover.horizontal_offset,
                    current_y - size.height,
                );

                window.defer_draw(popover.element, popover_origin, 2, None);
                if position != itertools::Position::Last {
                    let origin = point(popover_origin.x, popover_origin.y - HOVER_POPOVER_GAP);
                    draw_occluder(size.width, origin, window, cx);
                }

                current_y = popover_origin.y - HOVER_POPOVER_GAP;
            }
        }

        fn place_popovers_below(
            hovered_point: gpui::Point<Pixels>,
            measured_hover_popovers: Vec<MeasuredHoverPopover>,
            line_height: Pixels,
            window: &mut Window,
            cx: &mut App,
        ) {
            let mut current_y = hovered_point.y + line_height;
            for (position, popover) in measured_hover_popovers.into_iter().with_position() {
                let size = popover.size;
                let popover_origin = point(hovered_point.x + popover.horizontal_offset, current_y);

                window.defer_draw(popover.element, popover_origin, 2, None);
                if position != itertools::Position::Last {
                    let origin = point(popover_origin.x, popover_origin.y + size.height);
                    draw_occluder(size.width, origin, window, cx);
                }

                current_y = popover_origin.y + size.height + HOVER_POPOVER_GAP;
            }
        }

        let intersects_menu = |bounds: Bounds<Pixels>| -> bool {
            context_menu_layout
                .as_ref()
                .is_some_and(|menu| bounds.intersects(&menu.bounds))
        };

        let can_place_above = {
            let mut bounds_above = Vec::new();
            let mut current_y = hovered_point.y;
            for popover in &measured_hover_popovers {
                let size = popover.size;
                let popover_origin = point(
                    hovered_point.x + popover.horizontal_offset,
                    current_y - size.height,
                );
                bounds_above.push(Bounds::new(popover_origin, size));
                current_y = popover_origin.y - HOVER_POPOVER_GAP;
            }
            bounds_above
                .iter()
                .all(|b| b.is_contained_within(hitbox) && !intersects_menu(*b))
        };

        let can_place_below = || {
            let mut bounds_below = Vec::new();
            let mut current_y = hovered_point.y + line_height;
            for popover in &measured_hover_popovers {
                let size = popover.size;
                let popover_origin = point(hovered_point.x + popover.horizontal_offset, current_y);
                bounds_below.push(Bounds::new(popover_origin, size));
                current_y = popover_origin.y + size.height + HOVER_POPOVER_GAP;
            }
            bounds_below
                .iter()
                .all(|b| b.is_contained_within(hitbox) && !intersects_menu(*b))
        };

        if can_place_above {
            // try placing above hovered point
            place_popovers_above(hovered_point, measured_hover_popovers, window, cx);
        } else if can_place_below() {
            // try placing below hovered point
            place_popovers_below(
                hovered_point,
                measured_hover_popovers,
                line_height,
                window,
                cx,
            );
        } else {
            // try to place popovers around the context menu
            let origin_surrounding_menu = context_menu_layout.as_ref().and_then(|menu| {
                let total_width = measured_hover_popovers
                    .iter()
                    .map(|p| p.size.width)
                    .max()
                    .unwrap_or(Pixels::ZERO);
                let y_for_horizontal_positioning = if menu.y_flipped {
                    menu.bounds.bottom() - overall_height
                } else {
                    menu.bounds.top()
                };
                let possible_origins = vec![
                    // left of context menu
                    point(
                        menu.bounds.left() - total_width - HOVER_POPOVER_GAP,
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
                        menu.bounds.top() - overall_height - HOVER_POPOVER_GAP,
                    ),
                    // bottom of context menu
                    point(menu.bounds.left(), menu.bounds.bottom() + HOVER_POPOVER_GAP),
                ];
                possible_origins.into_iter().find(|&origin| {
                    Bounds::new(origin, size(total_width, overall_height))
                        .is_contained_within(hitbox)
                })
            });
            if let Some(origin) = origin_surrounding_menu {
                let mut current_y = origin.y;
                for (position, popover) in measured_hover_popovers.into_iter().with_position() {
                    let size = popover.size;
                    let popover_origin = point(origin.x, current_y);

                    window.defer_draw(popover.element, popover_origin, 2, None);
                    if position != itertools::Position::Last {
                        let origin = point(popover_origin.x, popover_origin.y + size.height);
                        draw_occluder(size.width, origin, window, cx);
                    }

                    current_y = popover_origin.y + size.height + HOVER_POPOVER_GAP;
                }
            } else {
                // fallback to existing above/below cursor logic
                // this might overlap menu or overflow in rare case
                if can_place_above {
                    place_popovers_above(hovered_point, measured_hover_popovers, window, cx);
                } else {
                    place_popovers_below(
                        hovered_point,
                        measured_hover_popovers,
                        line_height,
                        window,
                        cx,
                    );
                }
            }
        }
    }
}
