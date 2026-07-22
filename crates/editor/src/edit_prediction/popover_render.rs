use super::*;

impl Editor {
    fn render_edit_prediction_modifier_jump_popover(
        &mut self,
        text_bounds: &Bounds<Pixels>,
        content_origin: gpui::Point<Pixels>,
        visible_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        newest_selection_head: Option<DisplayPoint>,
        target_display_point: DisplayPoint,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        let scrolled_content_origin =
            content_origin - gpui::Point::new(scroll_pixel_position.x.into(), Pixels::ZERO);

        const SCROLL_PADDING_Y: Pixels = px(12.);

        if target_display_point.row() < visible_row_range.start {
            return self.render_edit_prediction_scroll_popover(
                &|_| SCROLL_PADDING_Y,
                IconName::ArrowUp,
                visible_row_range,
                line_layouts,
                newest_selection_head,
                scrolled_content_origin,
                window,
                cx,
            );
        } else if target_display_point.row() >= visible_row_range.end {
            return self.render_edit_prediction_scroll_popover(
                &|size| text_bounds.size.height - size.height - SCROLL_PADDING_Y,
                IconName::ArrowDown,
                visible_row_range,
                line_layouts,
                newest_selection_head,
                scrolled_content_origin,
                window,
                cx,
            );
        }

        const POLE_WIDTH: Pixels = px(2.);

        let line_layout =
            line_layouts.get(target_display_point.row().minus(visible_row_range.start) as usize)?;
        let target_column = target_display_point.column() as usize;

        let target_x = line_layout.x_for_index(target_column);
        let target_y = (target_display_point.row().as_f64() * f64::from(line_height))
            - scroll_pixel_position.y;

        let flag_on_right = target_x < text_bounds.size.width / 2.;

        let mut border_color = Self::edit_prediction_callout_popover_border_color(cx);
        border_color.l += 0.001;

        let mut element = v_flex()
            .items_end()
            .when(flag_on_right, |el| el.items_start())
            .child(if flag_on_right {
                self.render_edit_prediction_line_popover("Jump", None, window, cx)
                    .rounded_bl(px(0.))
                    .rounded_tl(px(0.))
                    .border_l_2()
                    .border_color(border_color)
            } else {
                self.render_edit_prediction_line_popover("Jump", None, window, cx)
                    .rounded_br(px(0.))
                    .rounded_tr(px(0.))
                    .border_r_2()
                    .border_color(border_color)
            })
            .child(div().w(POLE_WIDTH).bg(border_color).h(line_height))
            .into_any();

        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

        let mut origin = scrolled_content_origin + point(target_x, target_y.into())
            - point(
                if flag_on_right {
                    POLE_WIDTH
                } else {
                    size.width - POLE_WIDTH
                },
                size.height - line_height,
            );

        origin.x = origin.x.max(content_origin.x);

        element.prepaint_at(origin, window, cx);

        Some((element, origin))
    }

    fn render_edit_prediction_scroll_popover(
        &mut self,
        to_y: &dyn Fn(Size<Pixels>) -> Pixels,
        scroll_icon: IconName,
        visible_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        newest_selection_head: Option<DisplayPoint>,
        scrolled_content_origin: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        let mut element = self
            .render_edit_prediction_line_popover("Scroll", Some(scroll_icon), window, cx)
            .into_any();

        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

        let cursor = newest_selection_head?;
        let cursor_row_layout =
            line_layouts.get(cursor.row().minus(visible_row_range.start) as usize)?;
        let cursor_column = cursor.column() as usize;

        let cursor_character_x = cursor_row_layout.x_for_index(cursor_column);

        let origin = scrolled_content_origin + point(cursor_character_x, to_y(size));

        element.prepaint_at(origin, window, cx);
        Some((element, origin))
    }

    fn render_edit_prediction_eager_jump_popover(
        &mut self,
        text_bounds: &Bounds<Pixels>,
        content_origin: gpui::Point<Pixels>,
        editor_snapshot: &EditorSnapshot,
        visible_row_range: Range<DisplayRow>,
        scroll_top: ScrollOffset,
        scroll_bottom: ScrollOffset,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        target_display_point: DisplayPoint,
        editor_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        if target_display_point.row().as_f64() < scroll_top {
            let mut element = self
                .render_edit_prediction_line_popover(
                    "Jump to Edit",
                    Some(IconName::ArrowUp),
                    window,
                    cx,
                )
                .into_any();

            let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
            let offset = point(
                (text_bounds.size.width - size.width) / 2.,
                Self::EDIT_PREDICTION_POPOVER_PADDING_Y,
            );

            let origin = text_bounds.origin + offset;
            element.prepaint_at(origin, window, cx);
            Some((element, origin))
        } else if (target_display_point.row().as_f64() + 1.) > scroll_bottom {
            let mut element = self
                .render_edit_prediction_line_popover(
                    "Jump to Edit",
                    Some(IconName::ArrowDown),
                    window,
                    cx,
                )
                .into_any();

            let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
            let offset = point(
                (text_bounds.size.width - size.width) / 2.,
                text_bounds.size.height - size.height - Self::EDIT_PREDICTION_POPOVER_PADDING_Y,
            );

            let origin = text_bounds.origin + offset;
            element.prepaint_at(origin, window, cx);
            Some((element, origin))
        } else {
            self.render_edit_prediction_end_of_line_popover(
                "Jump to Edit",
                editor_snapshot,
                visible_row_range,
                target_display_point,
                line_height,
                scroll_pixel_position,
                content_origin,
                editor_width,
                window,
                cx,
            )
        }
    }

    fn render_edit_prediction_end_of_line_popover(
        self: &mut Editor,
        label: &'static str,
        editor_snapshot: &EditorSnapshot,
        visible_row_range: Range<DisplayRow>,
        target_display_point: DisplayPoint,
        line_height: Pixels,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        content_origin: gpui::Point<Pixels>,
        editor_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        let target_line_end = DisplayPoint::new(
            target_display_point.row(),
            editor_snapshot.line_len(target_display_point.row()),
        );

        let mut element = self
            .render_edit_prediction_line_popover(label, None, window, cx)
            .into_any();

        let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

        let line_origin =
            self.display_to_pixel_point(target_line_end, editor_snapshot, window, cx)?;

        let start_point = content_origin - point(scroll_pixel_position.x.into(), Pixels::ZERO);
        let mut origin = start_point
            + line_origin
            + point(Self::EDIT_PREDICTION_POPOVER_PADDING_X, Pixels::ZERO);
        origin.x = origin.x.max(content_origin.x);

        let max_x = content_origin.x + editor_width - size.width;

        if origin.x > max_x {
            let offset = line_height + Self::EDIT_PREDICTION_POPOVER_PADDING_Y;

            let icon = if visible_row_range.contains(&(target_display_point.row() + 2)) {
                origin.y += offset;
                IconName::ArrowUp
            } else {
                origin.y -= offset;
                IconName::ArrowDown
            };

            element = self
                .render_edit_prediction_line_popover(label, Some(icon), window, cx)
                .into_any();

            let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);

            origin.x = content_origin.x + editor_width - size.width - px(2.);
        }

        element.prepaint_at(origin, window, cx);
        Some((element, origin))
    }

    fn render_edit_prediction_diff_popover(
        self: &Editor,
        text_bounds: &Bounds<Pixels>,
        content_origin: gpui::Point<Pixels>,
        right_margin: Pixels,
        editor_snapshot: &EditorSnapshot,
        visible_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        newest_selection_head: Option<DisplayPoint>,
        editor_width: Pixels,
        style: &EditorStyle,
        edits: &Vec<(Range<Anchor>, Arc<str>)>,
        edit_preview: &Option<language::EditPreview>,
        snapshot: &language::BufferSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        let Some((first_edit_range, _)) = edits.first() else {
            return None;
        };
        let Some((last_edit_range, _)) = edits.last() else {
            return None;
        };

        let edit_start = first_edit_range.start.to_display_point(editor_snapshot);
        let edit_end = last_edit_range.end.to_display_point(editor_snapshot);

        let is_visible = visible_row_range.contains(&edit_start.row())
            || visible_row_range.contains(&edit_end.row());
        if !is_visible {
            return None;
        }

        let highlighted_edits = if let Some(edit_preview) = edit_preview.as_ref() {
            edit_prediction_edit_text(
                snapshot,
                edits,
                edit_preview,
                false,
                editor_snapshot.buffer_snapshot(),
                cx,
            )
        } else {
            // Fallback for providers without edit_preview
            edit_prediction_fallback_text(edits, cx)
        };

        let styled_text = highlighted_edits.to_styled_text(&style.text);
        let line_count = highlighted_edits.text.lines().count();

        const BORDER_WIDTH: Pixels = px(1.);

        let keybind = self.render_edit_prediction_keybind(window, cx);
        let has_keybind = keybind.is_some();

        let mut element = h_flex()
            .items_start()
            .debug_selector(|| "edit_prediction_diff_popover".into())
            .child(
                h_flex()
                    .bg(cx.theme().colors().editor_background)
                    .border(BORDER_WIDTH)
                    .shadow_xs()
                    .border_color(cx.theme().colors().border)
                    .rounded_l_lg()
                    .when(line_count > 1, |el| el.rounded_br_lg())
                    .pr_1()
                    .child(styled_text),
            )
            .child(
                h_flex()
                    .h(line_height + BORDER_WIDTH * 2.)
                    .px_1p5()
                    .gap_1()
                    // Workaround: For some reason, there's a gap if we don't do this
                    .ml(-BORDER_WIDTH)
                    .shadow(vec![
                        gpui::BoxShadow::new(px(1.), px(1.), gpui::black().opacity(0.05))
                            .blur_radius(px(2.)),
                    ])
                    .bg(Editor::edit_prediction_line_popover_bg_color(cx))
                    .border(BORDER_WIDTH)
                    .border_color(cx.theme().colors().border)
                    .rounded_r_lg()
                    .id("edit_prediction_diff_popover_keybind")
                    .when(!has_keybind, |el| {
                        let status_colors = cx.theme().status();

                        el.bg(status_colors.error_background)
                            .border_color(status_colors.error.opacity(0.6))
                            .child(Icon::new(IconName::Info).color(Color::Error))
                            .cursor_default()
                            .hoverable_tooltip(move |_window, cx| {
                                cx.new(|_| MissingEditPredictionKeybindingTooltip).into()
                            })
                    })
                    .children(keybind),
            )
            .into_any();

        let longest_row =
            editor_snapshot.longest_row_in_range(edit_start.row()..edit_end.row() + 1);
        let longest_line_width = if visible_row_range.contains(&longest_row) {
            line_layouts[(longest_row.0 - visible_row_range.start.0) as usize].width
        } else {
            layout_line(
                longest_row,
                editor_snapshot,
                style,
                editor_width,
                |_| false,
                window,
                cx,
            )
            .width
        };

        let viewport_bounds =
            Bounds::new(Default::default(), window.viewport_size()).extend(Edges {
                right: -right_margin,
                ..Default::default()
            });
        let popover_right_bound = cmp::min(text_bounds.right(), viewport_bounds.right());

        let x_after_longest = Pixels::from(
            ScrollPixelOffset::from(
                text_bounds.origin.x + longest_line_width + Self::EDIT_PREDICTION_POPOVER_PADDING_X,
            ) - scroll_pixel_position.x,
        );

        let element_bounds = element.layout_as_root(AvailableSpace::min_size(), window, cx);

        let can_position_to_the_right = x_after_longest < text_bounds.right()
            && element_bounds.width <= popover_right_bound - text_bounds.left();

        let mut origin = if can_position_to_the_right {
            point(
                x_after_longest.min(popover_right_bound - element_bounds.width),
                text_bounds.origin.y
                    + Pixels::from(
                        edit_start.row().as_f64() * ScrollPixelOffset::from(line_height)
                            - scroll_pixel_position.y,
                    ),
            )
        } else {
            let cursor_row = newest_selection_head.map(|head| head.row());
            let above_edit = edit_start
                .row()
                .0
                .checked_sub(line_count as u32)
                .map(DisplayRow);
            let below_edit = Some(edit_end.row() + 1);
            let above_cursor =
                cursor_row.and_then(|row| row.0.checked_sub(line_count as u32).map(DisplayRow));
            let below_cursor = cursor_row.map(|cursor_row| cursor_row + 1);

            // Place the edit popover adjacent to the edit if there is a location
            // available that is onscreen and does not obscure the cursor. Otherwise,
            // place it adjacent to the cursor.
            let row_target = [above_edit, below_edit, above_cursor, below_cursor]
                .into_iter()
                .flatten()
                .find(|&start_row| {
                    let end_row = start_row + line_count as u32;
                    visible_row_range.contains(&start_row)
                        && visible_row_range.contains(&end_row)
                        && cursor_row
                            .is_none_or(|cursor_row| !((start_row..end_row).contains(&cursor_row)))
                })?;

            content_origin
                + point(
                    Pixels::from(-scroll_pixel_position.x),
                    Pixels::from(
                        (row_target.as_f64() - scroll_position.y) * f64::from(line_height),
                    ),
                )
        };

        origin.x -= BORDER_WIDTH;

        window.with_content_mask(Some(gpui::ContentMask::new(*text_bounds)), |window| {
            window.defer_draw(element, origin, 1, Some(window.content_mask()));
        });

        // Do not return an element, since it will already be drawn due to defer_draw.
        None
    }
}
