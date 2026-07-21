use super::*;

impl EditorElement {
    pub(super) fn mouse_dragged(
        editor: &mut Editor,
        event: &MouseMoveEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if !editor.has_pending_selection()
            && matches!(editor.selection_drag_state, SelectionDragState::None)
        {
            return;
        }

        let point_for_position = position_map.point_for_position(event.position);
        let text_hitbox = &position_map.text_hitbox;

        let scroll_delta = {
            let text_bounds = text_hitbox.bounds;
            let mut scroll_delta = gpui::Point::<f32>::default();
            let vertical_margin = position_map.line_height.min(text_bounds.size.height / 3.0);
            let top = text_bounds.origin.y + vertical_margin;
            let bottom = text_bounds.bottom_left().y - vertical_margin;
            if event.position.y < top {
                scroll_delta.y = -scale_vertical_mouse_autoscroll_delta(top - event.position.y);
            }
            if event.position.y > bottom {
                scroll_delta.y = scale_vertical_mouse_autoscroll_delta(event.position.y - bottom);
            }

            // We need horizontal width of text
            let style = editor.style.clone().unwrap_or_default();
            let font_id = window.text_system().resolve_font(&style.text.font());
            let font_size = style.text.font_size.to_pixels(window.rem_size());
            let em_width = window
                .text_system()
                .em_width(font_id, font_size)
                .unwrap_or(font_size);

            let scroll_margin_x = EditorSettings::get_global(cx).horizontal_scroll_margin;

            let scroll_space: Pixels = scroll_margin_x * em_width;

            let left = text_bounds.origin.x + scroll_space;
            let right = text_bounds.top_right().x - scroll_space;

            if event.position.x < left {
                scroll_delta.x = -scale_horizontal_mouse_autoscroll_delta(left - event.position.x);
            }
            if event.position.x > right {
                scroll_delta.x = scale_horizontal_mouse_autoscroll_delta(event.position.x - right);
            }
            scroll_delta
        };

        if !editor.has_pending_selection() {
            let drop_anchor = position_map
                .snapshot
                .display_point_to_anchor(point_for_position.nearest_valid, Bias::Left);
            match editor.selection_drag_state {
                SelectionDragState::Dragging {
                    ref mut drop_cursor,
                    ref mut hide_drop_cursor,
                    ..
                } => {
                    drop_cursor.start = drop_anchor;
                    drop_cursor.end = drop_anchor;
                    *hide_drop_cursor = !text_hitbox.is_hovered(window);
                    editor.apply_scroll_delta(scroll_delta, window, cx);
                    cx.notify();
                }
                SelectionDragState::ReadyToDrag {
                    ref selection,
                    ref click_position,
                    ref mouse_down_time,
                } => {
                    let drag_and_drop_delay = Duration::from_millis(
                        EditorSettings::get_global(cx)
                            .drag_and_drop_selection
                            .delay
                            .0,
                    );
                    if mouse_down_time.elapsed() >= drag_and_drop_delay {
                        let drop_cursor = Selection {
                            id: post_inc(&mut editor.selections.next_selection_id()),
                            start: drop_anchor,
                            end: drop_anchor,
                            reversed: false,
                            goal: SelectionGoal::None,
                        };
                        editor.selection_drag_state = SelectionDragState::Dragging {
                            selection: selection.clone(),
                            drop_cursor,
                            hide_drop_cursor: false,
                        };
                        editor.apply_scroll_delta(scroll_delta, window, cx);
                        cx.notify();
                    } else {
                        let click_point = position_map.point_for_position(*click_position);
                        editor.selection_drag_state = SelectionDragState::None;
                        editor.select(
                            SelectPhase::Begin {
                                position: click_point.nearest_valid,
                                add: false,
                                click_count: 1,
                            },
                            window,
                            cx,
                        );
                        editor.select(
                            SelectPhase::Update {
                                position: point_for_position.nearest_valid,
                                goal_column: point_for_position.exact_unclipped.column(),
                                scroll_delta,
                            },
                            window,
                            cx,
                        );
                    }
                }
                _ => {}
            }
        } else {
            editor.select(
                SelectPhase::Update {
                    position: point_for_position.nearest_valid,
                    goal_column: point_for_position.exact_unclipped.column(),
                    scroll_delta,
                },
                window,
                cx,
            );
        }
    }
}

fn scale_vertical_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 100.0).min(px(3.0)).into()
}

fn scale_horizontal_mouse_autoscroll_delta(delta: Pixels) -> f32 {
    (delta.pow(1.2) / 300.0).into()
}
