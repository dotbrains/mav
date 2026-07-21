use super::*;

impl EditorElement {
    pub(super) fn layout_minimap(
        &self,
        snapshot: &EditorSnapshot,
        minimap_width: Pixels,
        scroll_position: gpui::Point<f64>,
        scrollbar_layout_information: &ScrollbarLayoutInformation,
        scrollbar_layout: Option<&EditorScrollbars>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<MinimapLayout> {
        let minimap_editor = self.editor.read(cx).minimap().cloned()?;

        let minimap_settings = EditorSettings::get_global(cx).minimap;

        if minimap_settings.on_active_editor() {
            let active_editor = self.editor.read(cx).workspace().and_then(|ws| {
                ws.read(cx)
                    .active_pane()
                    .read(cx)
                    .active_item()
                    .and_then(|i| i.act_as::<Editor>(cx))
            });
            if active_editor.is_some_and(|e| e != self.editor) {
                return None;
            }
        }

        if !snapshot.mode.is_full()
            || minimap_width.is_zero()
            || matches!(
                minimap_settings.show,
                ShowMinimap::Auto if scrollbar_layout.is_none_or(|layout| !layout.visible)
            )
        {
            return None;
        }

        const MINIMAP_AXIS: ScrollbarAxis = ScrollbarAxis::Vertical;

        let ScrollbarLayoutInformation {
            editor_bounds,
            scroll_range,
            glyph_grid_cell,
        } = scrollbar_layout_information;

        let line_height = glyph_grid_cell.height;
        let scroll_position = scroll_position.along(MINIMAP_AXIS);

        let top_right_anchor = scrollbar_layout
            .and_then(|layout| layout.vertical.as_ref())
            .map(|vertical_scrollbar| vertical_scrollbar.hitbox.origin)
            .unwrap_or_else(|| editor_bounds.top_right());

        let thumb_state = self
            .editor
            .read_with(cx, |editor, _| editor.scroll_manager.minimap_thumb_state());

        let show_thumb = match minimap_settings.thumb {
            MinimapThumb::Always => true,
            MinimapThumb::Hover => thumb_state.is_some(),
        };

        let minimap_bounds = Bounds::from_anchor_and_size(
            gpui::Anchor::TopRight,
            top_right_anchor,
            size(minimap_width, editor_bounds.size.height),
        );
        let minimap_line_height = self.get_minimap_line_height(
            minimap_editor
                .read(cx)
                .text_style_refinement
                .as_ref()
                .and_then(|refinement| refinement.font_size)
                .unwrap_or(MINIMAP_FONT_SIZE),
            window,
            cx,
        );
        let minimap_height = minimap_bounds.size.height;

        let visible_editor_lines = (editor_bounds.size.height / line_height) as f64;
        let total_editor_lines = (scroll_range.height / line_height) as f64;
        let minimap_lines = (minimap_height / minimap_line_height) as f64;

        let minimap_scroll_top = MinimapLayout::calculate_minimap_top_offset(
            total_editor_lines,
            visible_editor_lines,
            minimap_lines,
            scroll_position,
        );

        let layout = ScrollbarLayout::for_minimap(
            window.insert_hitbox(minimap_bounds, HitboxBehavior::Normal),
            visible_editor_lines,
            total_editor_lines,
            minimap_line_height,
            scroll_position,
            minimap_scroll_top,
            show_thumb,
        )
        .with_thumb_state(thumb_state);

        minimap_editor.update(cx, |editor, cx| {
            editor.set_scroll_position(point(0., minimap_scroll_top), window, cx)
        });

        // Required for the drop shadow to be visible.
        const PADDING_OFFSET: Pixels = px(4.);

        let mut minimap = div()
            .size_full()
            .shadow_xs()
            .px(PADDING_OFFSET)
            .child(minimap_editor)
            .into_any_element();

        let extended_bounds = minimap_bounds.extend(Edges {
            right: PADDING_OFFSET,
            left: PADDING_OFFSET,
            ..Default::default()
        });
        minimap.layout_as_root(extended_bounds.size.into(), window, cx);
        window.with_absolute_element_offset(extended_bounds.origin, |window| {
            minimap.prepaint(window, cx)
        });

        Some(MinimapLayout {
            minimap,
            thumb_layout: layout,
            thumb_border_style: minimap_settings.thumb_border,
            minimap_line_height,
            minimap_scroll_top,
            max_scroll_top: total_editor_lines,
        })
    }

    pub(super) fn get_minimap_line_height(
        &self,
        font_size: AbsoluteLength,
        window: &mut Window,
        cx: &mut App,
    ) -> Pixels {
        let rem_size = self.rem_size(cx).unwrap_or(window.rem_size());
        let mut text_style = self.style.text.clone();
        text_style.font_size = font_size;
        text_style.line_height_in_pixels(rem_size)
    }

    pub(super) fn get_minimap_width(
        &self,
        minimap_settings: &Minimap,
        scrollbars_shown: bool,
        text_width: Pixels,
        em_width: Pixels,
        font_size: Pixels,
        rem_size: Pixels,
        cx: &App,
    ) -> Option<Pixels> {
        if minimap_settings.show == ShowMinimap::Auto && !scrollbars_shown {
            return None;
        }

        let minimap_font_size = self.editor.read_with(cx, |editor, cx| {
            editor.minimap().map(|minimap_editor| {
                minimap_editor
                    .read(cx)
                    .text_style_refinement
                    .as_ref()
                    .and_then(|refinement| refinement.font_size)
                    .unwrap_or(MINIMAP_FONT_SIZE)
            })
        })?;

        let minimap_em_width = em_width * (minimap_font_size.to_pixels(rem_size) / font_size);

        let minimap_width = (text_width * MinimapLayout::MINIMAP_WIDTH_PCT)
            .min(minimap_em_width * minimap_settings.max_width_columns.get() as f32);

        (minimap_width >= minimap_em_width * MinimapLayout::MINIMAP_MIN_WIDTH_COLUMNS)
            .then_some(minimap_width)
    }

    pub(super) fn paint_minimap(
        &self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(mut layout) = layout.minimap.take() {
            let minimap_hitbox = layout.thumb_layout.hitbox.clone();
            let dragging_minimap = self.editor.read(cx).scroll_manager.is_dragging_minimap();

            window.paint_layer(layout.thumb_layout.hitbox.bounds, |window| {
                window.with_element_namespace("minimap", |window| {
                    layout.minimap.paint(window, cx);
                    if let Some(thumb_bounds) = layout.thumb_layout.thumb_bounds {
                        let minimap_thumb_color = match layout.thumb_layout.thumb_state {
                            ScrollbarThumbState::Idle => {
                                cx.theme().colors().minimap_thumb_background
                            }
                            ScrollbarThumbState::Hovered => {
                                cx.theme().colors().minimap_thumb_hover_background
                            }
                            ScrollbarThumbState::Dragging => {
                                cx.theme().colors().minimap_thumb_active_background
                            }
                        };
                        let minimap_thumb_border = match layout.thumb_border_style {
                            MinimapThumbBorder::Full => Edges::all(ScrollbarLayout::BORDER_WIDTH),
                            MinimapThumbBorder::LeftOnly => Edges {
                                left: ScrollbarLayout::BORDER_WIDTH,
                                ..Default::default()
                            },
                            MinimapThumbBorder::LeftOpen => Edges {
                                right: ScrollbarLayout::BORDER_WIDTH,
                                top: ScrollbarLayout::BORDER_WIDTH,
                                bottom: ScrollbarLayout::BORDER_WIDTH,
                                ..Default::default()
                            },
                            MinimapThumbBorder::RightOpen => Edges {
                                left: ScrollbarLayout::BORDER_WIDTH,
                                top: ScrollbarLayout::BORDER_WIDTH,
                                bottom: ScrollbarLayout::BORDER_WIDTH,
                                ..Default::default()
                            },
                            MinimapThumbBorder::None => Default::default(),
                        };

                        window.paint_layer(minimap_hitbox.bounds, |window| {
                            window.paint_quad(quad(
                                thumb_bounds,
                                Corners::default(),
                                minimap_thumb_color,
                                minimap_thumb_border,
                                cx.theme().colors().minimap_thumb_border,
                                BorderStyle::Solid,
                            ));
                        });
                    }
                });
            });

            if dragging_minimap {
                window.set_window_cursor_style(CursorStyle::Arrow);
            } else {
                window.set_cursor_style(CursorStyle::Arrow, &minimap_hitbox);
            }

            let minimap_axis = ScrollbarAxis::Vertical;
            let pixels_per_line = Pixels::from(
                ScrollPixelOffset::from(minimap_hitbox.size.height) / layout.max_scroll_top,
            )
            .min(layout.minimap_line_height);

            let mut mouse_position = window.mouse_position();

            window.on_mouse_event({
                let editor = self.editor.clone();

                let minimap_hitbox = minimap_hitbox.clone();

                move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        return;
                    }

                    editor.update(cx, |editor, cx| {
                        if event.pressed_button == Some(MouseButton::Left)
                            && editor.scroll_manager.is_dragging_minimap()
                        {
                            let old_position = mouse_position.along(minimap_axis);
                            let new_position = event.position.along(minimap_axis);
                            if (minimap_hitbox.origin.along(minimap_axis)
                                ..minimap_hitbox.bottom_right().along(minimap_axis))
                                .contains(&old_position)
                            {
                                let position =
                                    editor.scroll_position(cx).apply_along(minimap_axis, |p| {
                                        (p + ScrollPixelOffset::from(
                                            (new_position - old_position) / pixels_per_line,
                                        ))
                                        .max(0.)
                                    });

                                editor.set_scroll_position(position, window, cx);
                            }
                            cx.stop_propagation();
                        } else if minimap_hitbox.is_hovered(window) {
                            editor.scroll_manager.set_is_hovering_minimap_thumb(
                                !event.dragging()
                                    && layout
                                        .thumb_layout
                                        .thumb_bounds
                                        .is_some_and(|bounds| bounds.contains(&event.position)),
                                cx,
                            );

                            // Stop hover events from propagating to the
                            // underlying editor if the minimap hitbox is hovered.
                            if !event.dragging() {
                                cx.stop_propagation();
                            }
                        } else {
                            editor.scroll_manager.hide_minimap_thumb(cx);
                        }
                        mouse_position = event.position;
                    });
                }
            });

            if dragging_minimap {
                window.on_mouse_event({
                    let editor = self.editor.clone();
                    move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            return;
                        }

                        editor.update(cx, |editor, cx| {
                            if minimap_hitbox.is_hovered(window) {
                                editor.scroll_manager.set_is_hovering_minimap_thumb(
                                    layout
                                        .thumb_layout
                                        .thumb_bounds
                                        .is_some_and(|bounds| bounds.contains(&event.position)),
                                    cx,
                                );
                            } else {
                                editor.scroll_manager.hide_minimap_thumb(cx);
                            }
                            cx.stop_propagation();
                        });
                    }
                });
            } else {
                window.on_mouse_event({
                    let editor = self.editor.clone();

                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture || !minimap_hitbox.is_hovered(window) {
                            return;
                        }

                        let event_position = event.position;

                        let Some(thumb_bounds) = layout.thumb_layout.thumb_bounds else {
                            return;
                        };

                        editor.update(cx, |editor, cx| {
                            if !thumb_bounds.contains(&event_position) {
                                let click_position =
                                    event_position.relative_to(&minimap_hitbox.origin).y;

                                let top_position = (click_position
                                    - thumb_bounds.size.along(minimap_axis) / 2.0)
                                    .max(Pixels::ZERO);

                                let scroll_offset = (layout.minimap_scroll_top
                                    + ScrollPixelOffset::from(
                                        top_position / layout.minimap_line_height,
                                    ))
                                .min(layout.max_scroll_top);

                                let scroll_position = editor
                                    .scroll_position(cx)
                                    .apply_along(minimap_axis, |_| scroll_offset);
                                editor.set_scroll_position(scroll_position, window, cx);
                            }

                            editor.scroll_manager.set_is_dragging_minimap(cx);
                            cx.stop_propagation();
                        });
                    }
                });
            }
        }
    }
}
