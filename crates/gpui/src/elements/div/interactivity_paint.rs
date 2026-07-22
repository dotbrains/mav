use super::*;

impl Interactivity {
    /// Paint this element according to this interactivity state's configured styles
    /// and bind the element's mouse and keyboard events.
    ///
    /// content_size is the size of the content of the element, which may be larger than the
    /// element's bounds if the element is scrollable.
    ///
    /// the final computed style will be passed to the provided function, along
    /// with the current scroll offset
    pub fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        hitbox: Option<&Hitbox>,
        window: &mut Window,
        cx: &mut App,
        f: impl FnOnce(&Style, &mut Window, &mut App),
    ) {
        self.hovered = hitbox.map(|hitbox| hitbox.is_hovered(window));
        window.with_optional_element_state::<InteractiveElementState, _>(
            global_id,
            |element_state, window| {
                let mut element_state =
                    element_state.map(|element_state| element_state.unwrap_or_default());

                let style = self.compute_style_internal(hitbox, element_state.as_mut(), window, cx);

                #[cfg(any(feature = "test-support", test))]
                if let Some(debug_selector) = &self.debug_selector {
                    window
                        .next_frame
                        .debug_bounds
                        .insert(debug_selector.clone(), bounds);
                }

                self.paint_hover_group_handler(window, cx);

                if style.visibility == Visibility::Hidden {
                    return ((), element_state);
                }

                let mut tab_group = None;
                if self.tab_group {
                    tab_group = self.tab_index;
                }
                if let Some(focus_handle) = &self.tracked_focus_handle {
                    window.next_frame.tab_stops.insert(focus_handle);
                }

                window.with_element_opacity(style.opacity, |window| {
                    style.paint(bounds, window, cx, |window: &mut Window, cx: &mut App| {
                        window.with_text_style(style.text_style().cloned(), |window| {
                            window.with_content_mask(
                                style.overflow_mask(bounds, window.rem_size()),
                                |window| {
                                    window.with_tab_group(tab_group, |window| {
                                        if let Some(hitbox) = hitbox {
                                            #[cfg(debug_assertions)]
                                            self.paint_debug_info(
                                                global_id, hitbox, &style, window, cx,
                                            );

                                            if let Some(drag) = cx.active_drag.as_ref() {
                                                if let Some(mouse_cursor) = drag.cursor_style {
                                                    window.set_window_cursor_style(mouse_cursor);
                                                }
                                            } else {
                                                if let Some(mouse_cursor) = style.mouse_cursor {
                                                    window.set_cursor_style(mouse_cursor, hitbox);
                                                }
                                            }

                                            if let Some(group) = self.group.clone() {
                                                GroupHitboxes::push(group, hitbox.id, cx);
                                            }

                                            if let Some(area) = self.window_control {
                                                window.insert_window_control_hitbox(
                                                    area,
                                                    hitbox.clone(),
                                                );
                                            }

                                            self.paint_mouse_listeners(
                                                hitbox,
                                                element_state.as_mut(),
                                                window,
                                                cx,
                                            );
                                            self.paint_scroll_listener(hitbox, &style, window, cx);
                                        }

                                        self.paint_keyboard_listeners(window, cx);

                                        if window.a11y.is_active() {
                                            if let Some(global_id) = global_id {
                                                if !self.a11y_action_listeners.is_empty() {
                                                    let node_id = global_id.accesskit_node_id();
                                                    for (action, listener) in
                                                        self.a11y_action_listeners.drain(..)
                                                    {
                                                        window.on_a11y_action(
                                                            node_id, action, listener,
                                                        );
                                                    }
                                                }
                                            }
                                        }

                                        f(&style, window, cx);

                                        if let Some(_hitbox) = hitbox {
                                            #[cfg(any(feature = "inspector", debug_assertions))]
                                            window.insert_inspector_hitbox(
                                                _hitbox.id,
                                                _inspector_id,
                                                cx,
                                            );

                                            if let Some(group) = self.group.as_ref() {
                                                GroupHitboxes::pop(group, cx);
                                            }
                                        }
                                    })
                                },
                            );
                        });
                    });
                });

                ((), element_state)
            },
        );
    }
}
