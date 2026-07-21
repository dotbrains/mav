use super::*;

impl EditorElement {
    pub(super) fn paint_cursors(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for cursor in &mut layout.visible_cursors {
            cursor.paint(layout.content_origin, window, cx);
        }
    }

    pub(super) fn paint_scrollbars(
        &mut self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(scrollbars_layout) = layout.scrollbars_layout.take() else {
            return;
        };
        let any_scrollbar_dragged = self.editor.read(cx).scroll_manager.any_scrollbar_dragged();

        for (scrollbar_layout, axis) in scrollbars_layout.iter_scrollbars() {
            let hitbox = &scrollbar_layout.hitbox;
            if scrollbars_layout.visible {
                let scrollbar_edges = match axis {
                    ScrollbarAxis::Horizontal => Edges {
                        top: Pixels::ZERO,
                        right: Pixels::ZERO,
                        bottom: Pixels::ZERO,
                        left: Pixels::ZERO,
                    },
                    ScrollbarAxis::Vertical => Edges {
                        top: Pixels::ZERO,
                        right: Pixels::ZERO,
                        bottom: Pixels::ZERO,
                        left: ScrollbarLayout::BORDER_WIDTH,
                    },
                };

                window.paint_layer(hitbox.bounds, |window| {
                    window.paint_quad(quad(
                        hitbox.bounds,
                        Corners::default(),
                        cx.theme().colors().scrollbar_track_background,
                        scrollbar_edges,
                        cx.theme().colors().scrollbar_track_border,
                        BorderStyle::Solid,
                    ));

                    if axis == ScrollbarAxis::Vertical {
                        let fast_markers =
                            self.collect_fast_scrollbar_markers(layout, scrollbar_layout, cx);
                        // Refresh slow scrollbar markers in the background. Below, we
                        // paint whatever markers have already been computed.
                        self.refresh_slow_scrollbar_markers(layout, scrollbar_layout, window, cx);

                        let markers = self.editor.read(cx).scrollbar_marker_state.markers.clone();
                        for marker in markers.iter().chain(&fast_markers) {
                            let mut marker = marker.clone();
                            marker.bounds.origin += hitbox.origin;
                            window.paint_quad(marker);
                        }
                    }

                    if let Some(thumb_bounds) = scrollbar_layout.thumb_bounds {
                        let scrollbar_thumb_color = match scrollbar_layout.thumb_state {
                            ScrollbarThumbState::Dragging => {
                                cx.theme().colors().scrollbar_thumb_active_background
                            }
                            ScrollbarThumbState::Hovered => {
                                cx.theme().colors().scrollbar_thumb_hover_background
                            }
                            ScrollbarThumbState::Idle => {
                                cx.theme().colors().scrollbar_thumb_background
                            }
                        };
                        window.paint_quad(quad(
                            thumb_bounds,
                            Corners::default(),
                            scrollbar_thumb_color,
                            scrollbar_edges,
                            cx.theme().colors().scrollbar_thumb_border,
                            BorderStyle::Solid,
                        ));

                        if any_scrollbar_dragged {
                            window.set_window_cursor_style(CursorStyle::Arrow);
                        } else {
                            window.set_cursor_style(CursorStyle::Arrow, hitbox);
                        }
                    }
                })
            }
        }

        window.on_mouse_event({
            let editor = self.editor.clone();
            let scrollbars_layout = scrollbars_layout.clone();

            let mut mouse_position = window.mouse_position();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    return;
                }

                editor.update(cx, |editor, cx| {
                    if let Some((scrollbar_layout, axis)) = event
                        .pressed_button
                        .filter(|button| *button == MouseButton::Left)
                        .and(editor.scroll_manager.dragging_scrollbar_axis())
                        .and_then(|axis| {
                            scrollbars_layout
                                .iter_scrollbars()
                                .find(|(_, a)| *a == axis)
                        })
                    {
                        let ScrollbarLayout {
                            hitbox,
                            text_unit_size,
                            ..
                        } = scrollbar_layout;

                        let old_position = mouse_position.along(axis);
                        let new_position = event.position.along(axis);
                        if (hitbox.origin.along(axis)..hitbox.bottom_right().along(axis))
                            .contains(&old_position)
                        {
                            let position = editor.scroll_position(cx).apply_along(axis, |p| {
                                (p + ScrollOffset::from(
                                    (new_position - old_position) / *text_unit_size,
                                ))
                                .max(0.)
                            });
                            editor.set_scroll_position(position, window, cx);
                        }

                        editor.scroll_manager.show_scrollbars(window, cx);
                        cx.stop_propagation();
                    } else if let Some((layout, axis)) = scrollbars_layout
                        .get_hovered_axis(window)
                        .filter(|_| !event.dragging())
                    {
                        if layout.thumb_hovered(&event.position) {
                            editor
                                .scroll_manager
                                .set_hovered_scroll_thumb_axis(axis, cx);
                        } else {
                            editor.scroll_manager.reset_scrollbar_state(cx);
                        }

                        editor.scroll_manager.show_scrollbars(window, cx);
                    } else {
                        editor.scroll_manager.reset_scrollbar_state(cx);
                    }

                    mouse_position = event.position;
                })
            }
        });

        if any_scrollbar_dragged {
            window.on_mouse_event({
                let editor = self.editor.clone();
                move |_: &MouseUpEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        return;
                    }

                    editor.update(cx, |editor, cx| {
                        if let Some((_, axis)) = scrollbars_layout.get_hovered_axis(window) {
                            editor
                                .scroll_manager
                                .set_hovered_scroll_thumb_axis(axis, cx);
                        } else {
                            editor.scroll_manager.reset_scrollbar_state(cx);
                        }
                        cx.stop_propagation();
                    });
                }
            });
        } else {
            window.on_mouse_event({
                let editor = self.editor.clone();

                move |event: &MouseDownEvent, phase, window, cx| {
                    if phase == DispatchPhase::Capture {
                        return;
                    }
                    let Some((scrollbar_layout, axis)) = scrollbars_layout.get_hovered_axis(window)
                    else {
                        return;
                    };

                    let ScrollbarLayout {
                        hitbox,
                        visible_range,
                        text_unit_size,
                        thumb_bounds,
                        ..
                    } = scrollbar_layout;

                    let Some(thumb_bounds) = thumb_bounds else {
                        return;
                    };

                    editor.update(cx, |editor, cx| {
                        editor
                            .scroll_manager
                            .set_dragged_scroll_thumb_axis(axis, cx);

                        let event_position = event.position.along(axis);

                        if event_position < thumb_bounds.origin.along(axis)
                            || thumb_bounds.bottom_right().along(axis) < event_position
                        {
                            let center_position = ((event_position - hitbox.origin.along(axis))
                                / *text_unit_size)
                                .round() as u32;
                            let start_position = center_position.saturating_sub(
                                (visible_range.end - visible_range.start) as u32 / 2,
                            );

                            let position = editor
                                .scroll_position(cx)
                                .apply_along(axis, |_| start_position as ScrollOffset);

                            editor.set_scroll_position(position, window, cx);
                        } else {
                            editor.scroll_manager.show_scrollbars(window, cx);
                        }

                        cx.stop_propagation();
                    });
                }
            });
        }
    }
}
