use super::*;

impl EditorElement {
    pub(crate) fn mouse_moved(
        editor: &mut Editor,
        event: &MouseMoveEvent,
        position_map: &PositionMap,
        split_side: Option<SplitSide>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let text_hitbox = &position_map.text_hitbox;
        let gutter_hitbox = &position_map.gutter_hitbox;
        let modifiers = event.modifiers;
        let text_hovered = text_hitbox.is_hovered(window);
        let gutter_hovered = gutter_hitbox.is_hovered(window);
        editor.set_gutter_hovered(gutter_hovered, cx);

        let point_for_position = position_map.point_for_position(event.position);
        let valid_point = point_for_position.nearest_valid;

        // Update diff review drag state if we're dragging
        if editor.diff_review_drag_state.is_some() {
            editor.update_diff_review_drag(valid_point.row(), window, cx);
        }

        let hovered_diff_control = position_map
            .diff_hunk_control_bounds
            .iter()
            .find(|(_, bounds)| bounds.contains(&event.position))
            .map(|(row, _)| *row);

        let hovered_diff_hunk_row = if let Some(control_row) = hovered_diff_control {
            Some(control_row)
        } else if text_hovered {
            let current_row = valid_point.row();
            position_map.display_hunks.iter().find_map(|(hunk, _)| {
                if let DisplayDiffHunk::Unfolded {
                    display_row_range, ..
                } = hunk
                {
                    if display_row_range.contains(&current_row) {
                        Some(display_row_range.start)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        } else {
            None
        };

        if hovered_diff_hunk_row != editor.hovered_diff_hunk_row {
            editor.hovered_diff_hunk_row = hovered_diff_hunk_row;
            cx.notify();
        }

        if text_hovered
            && let Some((bounds, buffer_id, blame_entry)) = &position_map.inline_blame_bounds
        {
            let mouse_over_inline_blame = bounds.contains(&event.position);
            let mouse_over_popover = editor
                .inline_blame_popover
                .as_ref()
                .and_then(|state| state.popover_bounds)
                .is_some_and(|bounds| bounds.contains(&event.position));
            let keyboard_grace = editor
                .inline_blame_popover
                .as_ref()
                .is_some_and(|state| state.keyboard_grace);

            if mouse_over_inline_blame || mouse_over_popover {
                editor.show_blame_popover(*buffer_id, blame_entry, event.position, false, cx);
            } else if !keyboard_grace {
                editor.hide_blame_popover(false, cx);
            }
        } else {
            let keyboard_grace = editor
                .inline_blame_popover
                .as_ref()
                .is_some_and(|state| state.keyboard_grace);
            if !keyboard_grace {
                editor.hide_blame_popover(false, cx);
            }
        }

        // Handle diff review indicator when gutter is hovered in diff mode with AI enabled
        let show_diff_review = editor.show_diff_review_button()
            && cx.has_flag::<DiffReviewFeatureFlag>()
            && !DisableAiSettings::is_ai_disabled_for_buffer(
                editor.buffer.read(cx).as_singleton().as_ref(),
                cx,
            );

        let diff_review_indicator = if gutter_hovered && show_diff_review {
            let is_visible = editor
                .gutter_diff_review_indicator
                .0
                .is_some_and(|indicator| indicator.is_active);

            if !is_visible {
                editor
                    .gutter_diff_review_indicator
                    .1
                    .get_or_insert_with(|| {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(200))
                                .await;

                            this.update(cx, |this, cx| {
                                if let Some(indicator) =
                                    this.gutter_diff_review_indicator.0.as_mut()
                                {
                                    indicator.is_active = true;
                                    cx.notify();
                                }
                            })
                            .ok();
                        })
                    });
            }

            let anchor = position_map
                .snapshot
                .display_point_to_anchor(valid_point, Bias::Left);
            Some(PhantomDiffReviewIndicator {
                start: anchor,
                end: anchor,
                is_active: is_visible,
            })
        } else {
            editor.gutter_diff_review_indicator.1 = None;
            None
        };

        if diff_review_indicator != editor.gutter_diff_review_indicator.0 {
            editor.gutter_diff_review_indicator.0 = diff_review_indicator;
            cx.notify();
        }

        // Don't show breakpoint indicator when diff review indicator is active on this row
        let is_on_diff_review_button_row = diff_review_indicator.is_some_and(|indicator| {
            let start_row = indicator
                .start
                .to_display_point(&position_map.snapshot.display_snapshot)
                .row();
            indicator.is_active && start_row == valid_point.row()
        });

        let gutter_hover_button = if gutter_hovered
            && !is_on_diff_review_button_row
            && split_side != Some(SplitSide::Left)
        {
            let buffer_anchor = position_map
                .snapshot
                .display_point_to_anchor(valid_point, Bias::Left);

            if position_map
                .snapshot
                .buffer_snapshot()
                .anchor_to_buffer_anchor(buffer_anchor)
                .is_some()
            {
                let is_visible = editor
                    .gutter_hover_button
                    .0
                    .is_some_and(|indicator| indicator.is_active);

                if !is_visible {
                    editor.gutter_hover_button.1.get_or_insert_with(|| {
                        cx.spawn(async move |this, cx| {
                            cx.background_executor()
                                .timer(Duration::from_millis(200))
                                .await;

                            this.update(cx, |this, cx| {
                                if let Some(indicator) = this.gutter_hover_button.0.as_mut() {
                                    indicator.is_active = true;
                                    cx.notify();
                                }
                            })
                            .ok();
                        })
                    });
                }

                Some(GutterHoverButton {
                    display_row: valid_point.row(),
                    is_active: is_visible,
                })
            } else {
                editor.gutter_hover_button.1 = None;
                None
            }
        } else if editor.has_mouse_context_menu() {
            editor.gutter_hover_button.1 = None;
            editor.gutter_hover_button.0
        } else {
            editor.gutter_hover_button.1 = None;
            None
        };

        if &gutter_hover_button != &editor.gutter_hover_button.0 {
            editor.gutter_hover_button.0 = gutter_hover_button;
            cx.notify();
        }

        // Don't trigger hover popover if mouse is hovering over context menu
        if text_hovered {
            editor.update_hovered_link(
                point_for_position,
                Some(event.position),
                &position_map.snapshot,
                modifiers,
                window,
                cx,
            );

            if let Some(point) = point_for_position.as_valid() {
                let anchor = position_map
                    .snapshot
                    .buffer_snapshot()
                    .anchor_before(point.to_offset(&position_map.snapshot, Bias::Left));
                hover_at(editor, Some(anchor), Some(event.position), window, cx);
                Self::update_visible_cursor(editor, point, position_map, window, cx);
            } else {
                editor.update_inlay_link_and_hover_points(
                    &position_map.snapshot,
                    point_for_position,
                    Some(event.position),
                    modifiers.secondary(),
                    modifiers.shift,
                    window,
                    cx,
                );
            }
        } else {
            editor.hide_hovered_link(cx);
            hover_at(editor, None, Some(event.position), window, cx);
        }
    }

    fn update_visible_cursor(
        editor: &mut Editor,
        point: DisplayPoint,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let snapshot = &position_map.snapshot;
        let Some(hub) = editor.collaboration_hub() else {
            return;
        };
        let start = snapshot.display_snapshot.clip_point(
            DisplayPoint::new(point.row(), point.column().saturating_sub(1)),
            Bias::Left,
        );
        let end = snapshot.display_snapshot.clip_point(
            DisplayPoint::new(
                point.row(),
                (point.column() + 1).min(snapshot.line_len(point.row())),
            ),
            Bias::Right,
        );

        let range = snapshot
            .buffer_snapshot()
            .anchor_before(start.to_point(&snapshot.display_snapshot))
            ..snapshot
                .buffer_snapshot()
                .anchor_after(end.to_point(&snapshot.display_snapshot));

        let Some(selection) = snapshot.remote_selections_in_range(&range, hub, cx).next() else {
            return;
        };
        let key = HoveredCursor {
            replica_id: selection.replica_id,
            selection_id: selection.selection.id,
        };
        editor.hovered_cursors.insert(
            key.clone(),
            cx.spawn_in(window, async move |editor, cx| {
                cx.background_executor().timer(CURSORS_VISIBLE_FOR).await;
                editor
                    .update(cx, |editor, cx| {
                        editor.hovered_cursors.remove(&key);
                        cx.notify();
                    })
                    .ok();
            }),
        );
        cx.notify()
    }
}
