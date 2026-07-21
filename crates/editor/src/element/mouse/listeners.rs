use super::*;

impl EditorElement {
    pub(crate) fn layout_mouse_context_menu(
        &self,
        editor_snapshot: &EditorSnapshot,
        visible_range: Range<DisplayRow>,
        content_origin: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let position = self.editor.update(cx, |editor, cx| {
            let visible_start_point = editor.display_to_pixel_point(
                DisplayPoint::new(visible_range.start, 0),
                editor_snapshot,
                window,
                cx,
            )?;
            let visible_end_point = editor.display_to_pixel_point(
                DisplayPoint::new(visible_range.end, 0),
                editor_snapshot,
                window,
                cx,
            )?;

            let mouse_context_menu = editor.mouse_context_menu.as_ref()?;
            let (source_display_point, position) = match mouse_context_menu.position {
                mouse_context_menu::MenuPosition::PinnedToScreen(point) => (None, point),
                mouse_context_menu::MenuPosition::PinnedToEditor { source, offset } => {
                    let source_display_point = source.to_display_point(editor_snapshot);
                    let source_point =
                        editor.to_pixel_point(source, editor_snapshot, window, cx)?;
                    let position = content_origin + source_point + offset;
                    (Some(source_display_point), position)
                }
            };

            let source_included = source_display_point.is_none_or(|source_display_point| {
                visible_range
                    .to_inclusive()
                    .contains(&source_display_point.row())
            });
            let position_included =
                visible_start_point.y <= position.y && position.y <= visible_end_point.y;
            if !source_included && !position_included {
                None
            } else {
                Some(position)
            }
        })?;

        let text_style = TextStyleRefinement {
            line_height: Some(DefiniteLength::Fraction(
                BufferLineHeight::Comfortable.value(),
            )),
            ..Default::default()
        };
        window.with_text_style(Some(text_style), |window| {
            let mut element = self.editor.read_with(cx, |editor, _| {
                let mouse_context_menu = editor.mouse_context_menu.as_ref()?;
                let context_menu = mouse_context_menu.context_menu.clone();

                Some(
                    deferred(
                        anchored()
                            .position(position)
                            .child(context_menu)
                            .anchor(gpui::Anchor::TopLeft)
                            .snap_to_window_with_margin(px(8.)),
                    )
                    .with_priority(1)
                    .into_any(),
                )
            })?;

            element.prepaint_as_root(position, AvailableSpace::min_size(), window, cx);
            Some(element)
        })
    }

    pub(crate) fn paint_mouse_listeners(
        &mut self,
        layout: &EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layout.mode.is_minimap() {
            return;
        }

        self.paint_scroll_wheel_listener(layout, window, cx);

        window.on_mouse_event({
            let position_map = layout.position_map.clone();
            let editor = self.editor.clone();
            let line_numbers = layout.line_numbers.clone();

            move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    match event.button {
                        MouseButton::Left => editor.update(cx, |editor, cx| {
                            let pending_mouse_down = editor
                                .pending_mouse_down
                                .get_or_insert_with(Default::default)
                                .clone();

                            *pending_mouse_down.borrow_mut() = Some(event.clone());

                            Self::mouse_left_down(
                                editor,
                                event,
                                &position_map,
                                line_numbers.as_ref(),
                                window,
                                cx,
                            );
                        }),
                        MouseButton::Right => editor.update(cx, |editor, cx| {
                            Self::mouse_right_down(editor, event, &position_map, window, cx);
                        }),
                        MouseButton::Middle => editor.update(cx, |editor, cx| {
                            Self::mouse_middle_down(editor, event, &position_map, window, cx);
                        }),
                        _ => {}
                    };
                }
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();
            let position_map = layout.position_map.clone();

            move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    editor.update(cx, |editor, cx| {
                        Self::mouse_up(editor, event, &position_map, window, cx)
                    });
                }
            }
        });

        window.on_mouse_event({
            let editor = self.editor.clone();
            let position_map = layout.position_map.clone();
            let mut captured_mouse_down = None;

            move |event: &MouseUpEvent, phase, window, cx| match phase {
                // Clear the pending mouse down during the capture phase,
                // so that it happens even if another event handler stops
                // propagation.
                DispatchPhase::Capture => editor.update(cx, |editor, _cx| {
                    let pending_mouse_down = editor
                        .pending_mouse_down
                        .get_or_insert_with(Default::default)
                        .clone();

                    let mut pending_mouse_down = pending_mouse_down.borrow_mut();
                    if pending_mouse_down.is_some() && position_map.text_hitbox.is_hovered(window) {
                        captured_mouse_down = pending_mouse_down.take();
                        window.refresh();
                    }
                }),
                // Fire click handlers during the bubble phase.
                DispatchPhase::Bubble => editor.update(cx, |editor, cx| {
                    if let Some(mouse_down) = captured_mouse_down.take() {
                        let event = ClickEvent::Mouse(MouseClickEvent {
                            down: mouse_down,
                            up: event.clone(),
                        });
                        Self::click(editor, &event, &position_map, window, cx);
                    }
                }),
            }
        });

        window.on_mouse_event({
            let position_map = layout.position_map.clone();
            let editor = self.editor.clone();

            move |event: &MousePressureEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    editor.update(cx, |editor, cx| {
                        Self::pressure_click(editor, &event, &position_map, window, cx);
                    })
                }
            }
        });

        window.on_mouse_event({
            let position_map = layout.position_map.clone();
            let editor = self.editor.clone();
            let split_side = self.split_side;

            move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    editor.update(cx, |editor, cx| {
                        if editor.hover_state.focused(window, cx) {
                            return;
                        }
                        if event.pressed_button == Some(MouseButton::Left)
                            || event.pressed_button == Some(MouseButton::Middle)
                        {
                            Self::mouse_dragged(editor, event, &position_map, window, cx)
                        }

                        Self::mouse_moved(editor, event, &position_map, split_side, window, cx)
                    });
                }
            }
        });
    }

    fn paint_scroll_wheel_listener(
        &mut self,
        layout: &EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.on_mouse_event({
            let position_map = layout.position_map.clone();
            let editor = self.editor.clone();
            let hitbox = layout.hitbox.clone();
            let mut delta = ScrollDelta::default();

            // Set a minimum scroll_sensitivity of 0.01 to make sure the user doesn't
            // accidentally turn off their scrolling.
            let base_scroll_sensitivity =
                EditorSettings::get_global(cx).scroll_sensitivity.max(0.01);

            // Use a minimum fast_scroll_sensitivity for same reason above
            let fast_scroll_sensitivity = EditorSettings::get_global(cx)
                .fast_scroll_sensitivity
                .max(0.01);

            move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                    delta = delta.coalesce(event.delta);

                    if event.modifiers.secondary()
                        && editor.read(cx).enable_mouse_wheel_zoom
                        && EditorSettings::get_global(cx).mouse_wheel_zoom
                    {
                        let delta_y = match event.delta {
                            ScrollDelta::Pixels(pixels) => pixels.y.into(),
                            ScrollDelta::Lines(lines) => lines.y,
                        };

                        if delta_y > 0.0 {
                            theme_settings::increase_buffer_font_size(cx);
                        } else if delta_y < 0.0 {
                            theme_settings::decrease_buffer_font_size(cx);
                        }

                        cx.stop_propagation();
                    } else {
                        let scroll_sensitivity = {
                            if event.modifiers.alt {
                                fast_scroll_sensitivity
                            } else {
                                base_scroll_sensitivity
                            }
                        };

                        editor.update(cx, |editor, cx| {
                            let line_height = position_map.line_height;
                            let glyph_width = position_map.em_layout_width;
                            let (delta, axis) = match delta {
                                gpui::ScrollDelta::Pixels(mut pixels) => {
                                    //Trackpad
                                    let axis =
                                        position_map.snapshot.ongoing_scroll.filter(&mut pixels);
                                    (pixels, axis)
                                }

                                gpui::ScrollDelta::Lines(lines) => {
                                    //Not trackpad
                                    let pixels =
                                        point(lines.x * glyph_width, lines.y * line_height);
                                    (pixels, None)
                                }
                            };

                            let current_scroll_position = position_map.snapshot.scroll_position();
                            let x = (current_scroll_position.x
                                * ScrollPixelOffset::from(glyph_width)
                                - ScrollPixelOffset::from(delta.x * scroll_sensitivity))
                                / ScrollPixelOffset::from(glyph_width);
                            let y = (current_scroll_position.y
                                * ScrollPixelOffset::from(line_height)
                                - ScrollPixelOffset::from(delta.y * scroll_sensitivity))
                                / ScrollPixelOffset::from(line_height);
                            let mut scroll_position =
                                point(x, y).clamp(&point(0., 0.), &position_map.scroll_max);
                            let forbid_vertical_scroll =
                                editor.scroll_manager.forbid_vertical_scroll();
                            if forbid_vertical_scroll {
                                scroll_position.y = current_scroll_position.y;
                            }

                            if scroll_position != current_scroll_position {
                                editor.scroll(scroll_position, axis, window, cx);
                                cx.stop_propagation();
                            } else if y < 0. && !forbid_vertical_scroll {
                                // Due to clamping, we may fail to detect cases of overscroll to the top;
                                // We want the scroll manager to get an update in such cases and detect the change of direction
                                // on the next frame.
                                if editor.scroll_manager.should_notify_top_overscroll(axis) {
                                    cx.notify();
                                }
                            } else {
                                editor.scroll_manager.reset_top_overscroll_notification();
                            }
                        });
                    }
                }
            }
        });
    }
}
