use super::*;

impl EditorElement {
    pub(super) fn layout_cursor_popovers(
        &self,
        line_height: Pixels,
        text_hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        right_margin: Pixels,
        start_row: DisplayRow,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_layouts: &[LineWithInvisibles],
        cursor: DisplayPoint,
        cursor_point: Point,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<ContextMenuLayout> {
        let mut min_menu_height = Pixels::ZERO;
        let mut max_menu_height = Pixels::ZERO;
        let mut height_above_menu = Pixels::ZERO;
        let height_below_menu = Pixels::ZERO;
        let mut edit_prediction_popover_visible = false;
        let mut context_menu_visible = false;
        let context_menu_placement;

        {
            let editor = self.editor.read(cx);
            if editor.edit_prediction_visible_in_cursor_popover(editor.has_active_edit_prediction())
            {
                height_above_menu +=
                    editor.edit_prediction_cursor_popover_height() + POPOVER_Y_PADDING;
                edit_prediction_popover_visible = true;
            }

            if editor.context_menu_visible()
                && let Some(crate::ContextMenuOrigin::Cursor) = editor.context_menu_origin()
            {
                let (min_height_in_lines, max_height_in_lines) = editor
                    .context_menu_options
                    .as_ref()
                    .map_or((3, 12), |options| {
                        (options.min_entries_visible, options.max_entries_visible)
                    });

                min_menu_height += line_height * min_height_in_lines as f32 + POPOVER_Y_PADDING;
                max_menu_height += line_height * max_height_in_lines as f32 + POPOVER_Y_PADDING;
                context_menu_visible = true;
            }
            context_menu_placement = editor
                .context_menu_options
                .as_ref()
                .and_then(|options| options.placement.clone());
        }

        let visible = edit_prediction_popover_visible || context_menu_visible;
        if !visible {
            return None;
        }

        let cursor_row_layout = &line_layouts[cursor.row().minus(start_row) as usize];
        let target_position = content_origin
            + gpui::Point {
                x: cmp::max(
                    px(0.),
                    Pixels::from(
                        ScrollPixelOffset::from(
                            cursor_row_layout.x_for_index(cursor.column() as usize),
                        ) - scroll_pixel_position.x,
                    ),
                ),
                y: cmp::max(
                    px(0.),
                    Pixels::from(
                        cursor.row().next_row().as_f64() * ScrollPixelOffset::from(line_height)
                            - scroll_pixel_position.y,
                    ),
                ),
            };

        let viewport_bounds =
            Bounds::new(Default::default(), window.viewport_size()).extend(Edges {
                right: -right_margin - MENU_GAP,
                ..Default::default()
            });

        let min_height = height_above_menu + min_menu_height + height_below_menu;
        let max_height = height_above_menu + max_menu_height + height_below_menu;
        let (laid_out_popovers, y_flipped) = self.layout_popovers_above_or_below_line(
            target_position,
            line_height,
            min_height,
            max_height,
            context_menu_placement,
            text_hitbox,
            viewport_bounds,
            window,
            cx,
            |height, max_width_for_stable_x, y_flipped, window, cx| {
                // First layout the menu to get its size - others can be at least this wide.
                let context_menu = if context_menu_visible {
                    let menu_height = if y_flipped {
                        height - height_below_menu
                    } else {
                        height - height_above_menu
                    };
                    let mut element = self
                        .render_context_menu(line_height, menu_height, window, cx)
                        .expect("Visible context menu should always render.");
                    let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
                    Some((CursorPopoverType::CodeContextMenu, element, size))
                } else {
                    None
                };
                let min_width = context_menu
                    .as_ref()
                    .map_or(px(0.), |(_, _, size)| size.width);
                let max_width = max_width_for_stable_x.max(
                    context_menu
                        .as_ref()
                        .map_or(px(0.), |(_, _, size)| size.width),
                );

                let edit_prediction = if edit_prediction_popover_visible {
                    self.editor.update(cx, move |editor, cx| {
                        let mut element = editor.render_edit_prediction_cursor_popover(
                            min_width,
                            max_width,
                            cursor_point,
                            style,
                            window,
                            cx,
                        )?;
                        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
                        Some((CursorPopoverType::EditPrediction, element, size))
                    })
                } else {
                    None
                };
                vec![edit_prediction, context_menu]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
            },
        )?;

        let (menu_ix, (_, menu_bounds)) = laid_out_popovers
            .iter()
            .find_position(|(x, _)| matches!(x, CursorPopoverType::CodeContextMenu))?;
        let last_ix = laid_out_popovers.len() - 1;
        let menu_is_last = menu_ix == last_ix;
        let first_popover_bounds = laid_out_popovers[0].1;
        let last_popover_bounds = laid_out_popovers[last_ix].1;

        // Bounds to layout the aside around. When y_flipped, the aside goes either above or to the
        // right, and otherwise it goes below or to the right.
        let mut target_bounds = Bounds::from_corners(
            first_popover_bounds.origin,
            last_popover_bounds.bottom_right(),
        );
        target_bounds.size.width = menu_bounds.size.width;

        // Like `target_bounds`, but with the max height it could occupy. Choosing an aside position
        // based on this is preferred for layout stability.
        let mut max_target_bounds = target_bounds;
        max_target_bounds.size.height = max_height;
        if y_flipped {
            max_target_bounds.origin.y -= max_height - target_bounds.size.height;
        }

        // Add spacing around `target_bounds` and `max_target_bounds`.
        let mut extend_amount = Edges::all(MENU_GAP);
        if y_flipped {
            extend_amount.bottom = line_height;
        } else {
            extend_amount.top = line_height;
        }
        let target_bounds = target_bounds.extend(extend_amount);
        let max_target_bounds = max_target_bounds.extend(extend_amount);

        let must_place_above_or_below =
            if y_flipped && !menu_is_last && menu_bounds.size.height < max_menu_height {
                laid_out_popovers[menu_ix + 1..]
                    .iter()
                    .any(|(_, popover_bounds)| popover_bounds.size.width > menu_bounds.size.width)
            } else {
                false
            };

        let aside_bounds = self.layout_context_menu_aside(
            y_flipped,
            *menu_bounds,
            target_bounds,
            max_target_bounds,
            max_menu_height,
            must_place_above_or_below,
            text_hitbox,
            viewport_bounds,
            window,
            cx,
        );

        if let Some(menu_bounds) = laid_out_popovers.iter().find_map(|(popover_type, bounds)| {
            if matches!(popover_type, CursorPopoverType::CodeContextMenu) {
                Some(*bounds)
            } else {
                None
            }
        }) {
            let bounds = if let Some(aside_bounds) = aside_bounds {
                menu_bounds.union(&aside_bounds)
            } else {
                menu_bounds
            };
            return Some(ContextMenuLayout { y_flipped, bounds });
        }

        None
    }
}
