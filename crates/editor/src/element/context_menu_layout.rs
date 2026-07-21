use super::*;

impl EditorElement {
    pub(super) fn layout_gutter_menu(
        &self,
        line_height: Pixels,
        text_hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        right_margin: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        gutter_overshoot: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let editor = self.editor.read(cx);
        if !editor.context_menu_visible() {
            return;
        }
        let Some(crate::ContextMenuOrigin::GutterIndicator(gutter_row)) =
            editor.context_menu_origin()
        else {
            return;
        };
        // Context menu was spawned via a click on a gutter. Ensure it's a bit closer to the
        // indicator than just a plain first column of the text field.
        let target_position = content_origin
            + gpui::Point {
                x: -gutter_overshoot,
                y: Pixels::from(
                    gutter_row.next_row().as_f64() * ScrollPixelOffset::from(line_height)
                        - scroll_pixel_position.y,
                ),
            };

        let (min_height_in_lines, max_height_in_lines) = editor
            .context_menu_options
            .as_ref()
            .map_or((3, 12), |options| {
                (options.min_entries_visible, options.max_entries_visible)
            });

        let min_height = line_height * min_height_in_lines as f32 + POPOVER_Y_PADDING;
        let max_height = line_height * max_height_in_lines as f32 + POPOVER_Y_PADDING;
        let viewport_bounds =
            Bounds::new(Default::default(), window.viewport_size()).extend(Edges {
                right: -right_margin - MENU_GAP,
                ..Default::default()
            });
        self.layout_popovers_above_or_below_line(
            target_position,
            line_height,
            min_height,
            max_height,
            editor
                .context_menu_options
                .as_ref()
                .and_then(|options| options.placement.clone()),
            text_hitbox,
            viewport_bounds,
            window,
            cx,
            move |height, _max_width_for_stable_x, _, window, cx| {
                let mut element = self
                    .render_context_menu(line_height, height, window, cx)
                    .expect("Visible context menu should always render.");
                let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
                vec![(CursorPopoverType::CodeContextMenu, element, size)]
            },
        );
    }

    pub(super) fn layout_popovers_above_or_below_line(
        &self,
        target_position: gpui::Point<Pixels>,
        line_height: Pixels,
        min_height: Pixels,
        max_height: Pixels,
        placement: Option<ContextMenuPlacement>,
        text_hitbox: &Hitbox,
        viewport_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
        make_sized_popovers: impl FnOnce(
            Pixels,
            Pixels,
            bool,
            &mut Window,
            &mut App,
        ) -> Vec<(CursorPopoverType, AnyElement, Size<Pixels>)>,
    ) -> Option<(Vec<(CursorPopoverType, Bounds<Pixels>)>, bool)> {
        let text_style = TextStyleRefinement {
            line_height: Some(DefiniteLength::Fraction(
                BufferLineHeight::Comfortable.value(),
            )),
            ..Default::default()
        };
        window.with_text_style(Some(text_style), |window| {
            // If the max height won't fit below and there is more space above, put it above the line.
            let bottom_y_when_flipped = target_position.y - line_height;
            let available_above = bottom_y_when_flipped - text_hitbox.top();
            let available_below = text_hitbox.bottom() - target_position.y;
            let y_overflows_below = max_height > available_below;
            let mut y_flipped = match placement {
                Some(ContextMenuPlacement::Above) => true,
                Some(ContextMenuPlacement::Below) => false,
                None => y_overflows_below && available_above > available_below,
            };
            let mut height = cmp::min(
                max_height,
                if y_flipped {
                    available_above
                } else {
                    available_below
                },
            );

            // If the min height doesn't fit within text bounds, instead fit within the window.
            if height < min_height {
                let available_above = bottom_y_when_flipped;
                let available_below = viewport_bounds.bottom() - target_position.y;
                let (y_flipped_override, height_override) = match placement {
                    Some(ContextMenuPlacement::Above) => {
                        (true, cmp::min(available_above, min_height))
                    }
                    Some(ContextMenuPlacement::Below) => {
                        (false, cmp::min(available_below, min_height))
                    }
                    None => {
                        if available_below > min_height {
                            (false, min_height)
                        } else if available_above > min_height {
                            (true, min_height)
                        } else if available_above > available_below {
                            (true, available_above)
                        } else {
                            (false, available_below)
                        }
                    }
                };
                y_flipped = y_flipped_override;
                height = height_override;
            }

            let max_width_for_stable_x = viewport_bounds.right() - target_position.x;

            // TODO: Use viewport_bounds.width as a max width so that it doesn't get clipped on the left
            // for very narrow windows.
            let popovers =
                make_sized_popovers(height, max_width_for_stable_x, y_flipped, window, cx);
            if popovers.is_empty() {
                return None;
            }

            let max_width = popovers
                .iter()
                .map(|(_, _, size)| size.width)
                .max()
                .unwrap_or_default();

            let mut current_position = gpui::Point {
                // Snap the right edge of the list to the right edge of the window if its horizontal bounds
                // overflow. Include space for the scrollbar.
                x: target_position
                    .x
                    .min((viewport_bounds.right() - max_width).max(Pixels::ZERO)),
                y: if y_flipped {
                    bottom_y_when_flipped
                } else {
                    target_position.y
                },
            };

            let mut laid_out_popovers = popovers
                .into_iter()
                .map(|(popover_type, element, size)| {
                    if y_flipped {
                        current_position.y -= size.height;
                    }
                    let position = current_position;
                    window.defer_draw(element, current_position, 1, None);
                    if !y_flipped {
                        current_position.y += size.height + MENU_GAP;
                    } else {
                        current_position.y -= MENU_GAP;
                    }
                    (popover_type, Bounds::new(position, size))
                })
                .collect::<Vec<_>>();

            if y_flipped {
                laid_out_popovers.reverse();
            }

            Some((laid_out_popovers, y_flipped))
        })
    }

    pub(super) fn layout_context_menu_aside(
        &self,
        y_flipped: bool,
        menu_bounds: Bounds<Pixels>,
        target_bounds: Bounds<Pixels>,
        max_target_bounds: Bounds<Pixels>,
        max_height: Pixels,
        must_place_above_or_below: bool,
        text_hitbox: &Hitbox,
        viewport_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let available_within_viewport = target_bounds.space_within(&viewport_bounds);
        let positioned_aside = if available_within_viewport.right >= MENU_ASIDE_MIN_WIDTH
            && !must_place_above_or_below
        {
            let max_width = cmp::min(
                available_within_viewport.right - px(1.),
                MENU_ASIDE_MAX_WIDTH,
            );
            let mut aside = self.render_context_menu_aside(
                size(max_width, max_height - POPOVER_Y_PADDING),
                window,
                cx,
            )?;
            let size = aside.layout_as_root(AvailableSpace::min_size(), window, cx);
            let right_position = point(target_bounds.right(), menu_bounds.origin.y);
            Some((aside, right_position, size))
        } else {
            let max_size = size(
                // TODO(mgsloan): Once the menu is bounded by viewport width the bound on viewport
                // won't be needed here.
                cmp::min(
                    cmp::max(menu_bounds.size.width - px(2.), MENU_ASIDE_MIN_WIDTH),
                    viewport_bounds.right(),
                ),
                cmp::min(
                    max_height,
                    cmp::max(
                        available_within_viewport.top,
                        available_within_viewport.bottom,
                    ),
                ) - POPOVER_Y_PADDING,
            );
            let mut aside = self.render_context_menu_aside(max_size, window, cx)?;
            let actual_size = aside.layout_as_root(AvailableSpace::min_size(), window, cx);

            let top_position = point(
                menu_bounds.origin.x,
                target_bounds.top() - actual_size.height,
            );
            let bottom_position = point(menu_bounds.origin.x, target_bounds.bottom());

            let fit_within = |available: Edges<Pixels>, wanted: Size<Pixels>| {
                // Prefer to fit on the same side of the line as the menu, then on the other side of
                // the line.
                if !y_flipped && wanted.height < available.bottom {
                    Some(bottom_position)
                } else if !y_flipped && wanted.height < available.top {
                    Some(top_position)
                } else if y_flipped && wanted.height < available.top {
                    Some(top_position)
                } else if y_flipped && wanted.height < available.bottom {
                    Some(bottom_position)
                } else {
                    None
                }
            };

            // Prefer choosing a direction using max sizes rather than actual size for stability.
            let available_within_text = max_target_bounds.space_within(&text_hitbox.bounds);
            let wanted = size(MENU_ASIDE_MAX_WIDTH, max_height);
            let aside_position = fit_within(available_within_text, wanted)
                // Fallback: fit max size in window.
                .or_else(|| fit_within(max_target_bounds.space_within(&viewport_bounds), wanted))
                // Fallback: fit actual size in window.
                .or_else(|| fit_within(available_within_viewport, actual_size));

            aside_position.map(|position| (aside, position, actual_size))
        };

        // Skip drawing if it doesn't fit anywhere.
        if let Some((aside, position, size)) = positioned_aside {
            let aside_bounds = Bounds::new(position, size);
            window.defer_draw(aside, position, 2, None);
            return Some(aside_bounds);
        }

        None
    }

    pub(super) fn render_context_menu(
        &self,
        line_height: Pixels,
        height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let max_height_in_lines = ((height - POPOVER_Y_PADDING) / line_height).floor() as u32;
        self.editor.update(cx, |editor, cx| {
            editor.render_context_menu(max_height_in_lines, window, cx)
        })
    }

    pub(super) fn render_context_menu_aside(
        &self,
        max_size: Size<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if max_size.width < px(100.) || max_size.height < px(12.) {
            None
        } else {
            self.editor.update(cx, |editor, cx| {
                editor.render_context_menu_aside(max_size, window, cx)
            })
        }
    }
}
