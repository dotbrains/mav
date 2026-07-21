use super::*;

impl EditorElement {
    pub(super) fn mouse_left_down(
        editor: &mut Editor,
        event: &MouseDownEvent,
        position_map: &PositionMap,
        line_numbers: &HashMap<MultiBufferRow, LineNumberLayout>,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if window.default_prevented() {
            return;
        }

        let text_hitbox = &position_map.text_hitbox;
        let gutter_hitbox = &position_map.gutter_hitbox;
        let point_for_position = position_map.point_for_position(event.position);
        let mut click_count = event.click_count;
        let mut modifiers = event.modifiers;

        if let Some(hovered_hunk) =
            position_map
                .display_hunks
                .iter()
                .find_map(|(hunk, hunk_hitbox)| match hunk {
                    DisplayDiffHunk::Folded { .. } => None,
                    DisplayDiffHunk::Unfolded {
                        multi_buffer_range, ..
                    } => hunk_hitbox
                        .as_ref()
                        .is_some_and(|hitbox| hitbox.is_hovered(window))
                        .then(|| multi_buffer_range.clone()),
                })
        {
            editor.toggle_single_diff_hunk(hovered_hunk, cx);
            cx.notify();
            return;
        } else if gutter_hitbox.is_hovered(window) {
            click_count = 3; // Simulate triple-click when clicking the gutter to select lines
        } else if !text_hitbox.is_hovered(window) {
            return;
        }

        if EditorSettings::get_global(cx)
            .drag_and_drop_selection
            .enabled
            && click_count == 1
            && !modifiers.shift
        {
            let newest_anchor = editor.selections.newest_anchor();
            let snapshot = editor.snapshot(window, cx);
            let selection = newest_anchor.map(|anchor| anchor.to_display_point(&snapshot));
            if point_for_position.intersects_selection(&selection) {
                editor.selection_drag_state = SelectionDragState::ReadyToDrag {
                    selection: newest_anchor.clone(),
                    click_position: event.position,
                    mouse_down_time: Instant::now(),
                };
                cx.stop_propagation();
                return;
            }
        }

        let is_singleton = editor.buffer().read(cx).is_singleton();

        if click_count == 2 && !is_singleton {
            match EditorSettings::get_global(cx).double_click_in_multibuffer {
                DoubleClickInMultibuffer::Select => {
                    // do nothing special on double click, all selection logic is below
                }
                DoubleClickInMultibuffer::Open => {
                    if modifiers.alt {
                        // if double click is made with alt, pretend it's a regular double click without opening and alt,
                        // and run the selection logic.
                        modifiers.alt = false;
                    } else {
                        let scroll_position_row = position_map.scroll_position.y;
                        let display_row = (((event.position - gutter_hitbox.bounds.origin).y
                            / position_map.line_height)
                            as f64
                            + position_map.scroll_position.y)
                            as u32;
                        let multi_buffer_row = position_map
                            .snapshot
                            .display_point_to_point(
                                DisplayPoint::new(DisplayRow(display_row), 0),
                                Bias::Right,
                            )
                            .row;
                        let line_offset_from_top = display_row - scroll_position_row as u32;
                        // if double click is made without alt, open the corresponding excerp
                        editor.open_excerpts_common(
                            Some(JumpData::MultiBufferRow {
                                row: MultiBufferRow(multi_buffer_row),
                                line_offset_from_top,
                            }),
                            false,
                            window,
                            cx,
                        );
                        return;
                    }
                }
            }
        }

        if !is_singleton {
            let display_row = (ScrollPixelOffset::from(
                (event.position - gutter_hitbox.bounds.origin).y / position_map.line_height,
            ) + position_map.scroll_position.y) as u32;
            let multi_buffer_row = position_map
                .snapshot
                .display_point_to_point(DisplayPoint::new(DisplayRow(display_row), 0), Bias::Right)
                .row;
            if line_numbers
                .get(&MultiBufferRow(multi_buffer_row))
                .is_some_and(|line_layout| {
                    line_layout.segments.iter().any(|segment| {
                        segment
                            .hitbox
                            .as_ref()
                            .is_some_and(|hitbox| hitbox.contains(&event.position))
                    })
                })
            {
                let line_offset_from_top = display_row - position_map.scroll_position.y as u32;

                editor.open_excerpts_common(
                    Some(JumpData::MultiBufferRow {
                        row: MultiBufferRow(multi_buffer_row),
                        line_offset_from_top,
                    }),
                    modifiers.alt,
                    window,
                    cx,
                );
                cx.stop_propagation();
                return;
            }
        }

        let position = point_for_position.nearest_valid;
        if let Some(mode) = Editor::columnar_selection_mode(&modifiers, cx) {
            editor.select(
                SelectPhase::BeginColumnar {
                    position,
                    reset: match mode {
                        ColumnarMode::FromMouse => true,
                        ColumnarMode::FromSelection => false,
                    },
                    mode,
                    goal_column: point_for_position.exact_unclipped.column(),
                },
                window,
                cx,
            );
        } else if modifiers.shift && !modifiers.control && !modifiers.alt && !modifiers.secondary()
        {
            editor.select(
                SelectPhase::Extend {
                    position,
                    click_count,
                },
                window,
                cx,
            );
        } else {
            editor.select(
                SelectPhase::Begin {
                    position,
                    add: Editor::is_alt_pressed(&modifiers, cx),
                    click_count,
                },
                window,
                cx,
            );
        }
        cx.stop_propagation();
    }

    pub(super) fn mouse_right_down(
        editor: &mut Editor,
        event: &MouseDownEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if position_map.gutter_hitbox.is_hovered(window) {
            let gutter_right_padding = editor.gutter_dimensions.right_padding;
            let hitbox = &position_map.gutter_hitbox;

            if event.position.x <= hitbox.bounds.right() - gutter_right_padding
                // Don't show the gutter_context_menu in collab notes
                && editor.project.is_some()
            {
                let point_for_position = position_map.point_for_position(event.position);
                editor.set_gutter_context_menu(
                    point_for_position.nearest_valid.row(),
                    None,
                    event.position,
                    window,
                    cx,
                );
            }
            return;
        }

        if !position_map.text_hitbox.is_hovered(window) {
            return;
        }

        let point_for_position = position_map.point_for_position(event.position);
        mouse_context_menu::deploy_context_menu(
            editor,
            Some(event.position),
            point_for_position.nearest_valid,
            window,
            cx,
        );
        cx.stop_propagation();
    }

    pub(super) fn mouse_middle_down(
        editor: &mut Editor,
        event: &MouseDownEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        if !position_map.text_hitbox.is_hovered(window) || window.default_prevented() {
            return;
        }

        let point_for_position = position_map.point_for_position(event.position);
        let position = point_for_position.nearest_valid;

        editor.select(
            SelectPhase::BeginColumnar {
                position,
                reset: true,
                mode: ColumnarMode::FromMouse,
                goal_column: point_for_position.exact_unclipped.column(),
            },
            window,
            cx,
        );
    }

    pub(super) fn mouse_up(
        editor: &mut Editor,
        event: &MouseUpEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        // Handle diff review drag completion
        if editor.diff_review_drag_state.is_some() {
            editor.end_diff_review_drag(window, cx);
            cx.stop_propagation();
            return;
        }

        let text_hitbox = &position_map.text_hitbox;
        let end_selection = editor.has_pending_selection();
        let pending_nonempty_selections = editor.has_pending_nonempty_selection();
        let point_for_position = position_map.point_for_position(event.position);

        match editor.selection_drag_state {
            SelectionDragState::ReadyToDrag {
                selection: _,
                ref click_position,
                mouse_down_time: _,
            } => {
                if event.position == *click_position {
                    editor.select(
                        SelectPhase::Begin {
                            position: point_for_position.nearest_valid,
                            add: false,
                            click_count: 1, // ready to drag state only occurs on click count 1
                        },
                        window,
                        cx,
                    );
                    editor.selection_drag_state = SelectionDragState::None;
                    cx.stop_propagation();
                    return;
                } else {
                    debug_panic!("drag state can never be in ready state after drag")
                }
            }
            SelectionDragState::Dragging { ref selection, .. } => {
                let snapshot = editor.snapshot(window, cx);
                let selection_display = selection.map(|anchor| anchor.to_display_point(&snapshot));
                if !point_for_position.intersects_selection(&selection_display)
                    && text_hitbox.is_hovered(window)
                {
                    let is_cut = !(cfg!(target_os = "macos") && event.modifiers.alt
                        || cfg!(not(target_os = "macos")) && event.modifiers.control);
                    editor.move_selection_on_drop(
                        &selection.clone(),
                        point_for_position.nearest_valid,
                        is_cut,
                        window,
                        cx,
                    );
                }
                editor.selection_drag_state = SelectionDragState::None;
                cx.stop_propagation();
                cx.notify();
                return;
            }
            _ => {}
        }

        if end_selection {
            editor.select(SelectPhase::End, window, cx);
        }

        if end_selection && pending_nonempty_selections {
            cx.stop_propagation();
        } else if cfg!(any(target_os = "linux", target_os = "freebsd"))
            && event.button == MouseButton::Middle
        {
            #[allow(
                clippy::collapsible_if,
                clippy::needless_return,
                reason = "The cfg-block below makes this a false positive"
            )]
            if !text_hitbox.is_hovered(window) || editor.read_only(cx) {
                return;
            }

            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            if EditorSettings::get_global(cx).middle_click_paste {
                if let Some(text) = cx.read_from_primary().and_then(|item| item.text()) {
                    let point_for_position = position_map.point_for_position(event.position);
                    let position = point_for_position.nearest_valid;

                    editor.select(
                        SelectPhase::Begin {
                            position,
                            add: false,
                            click_count: 1,
                        },
                        window,
                        cx,
                    );
                    editor.insert(&text, window, cx);
                }
                cx.stop_propagation()
            }
        }
    }

    pub(super) fn click(
        editor: &mut Editor,
        event: &ClickEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let text_hitbox = &position_map.text_hitbox;
        let pending_nonempty_selections = editor.has_pending_nonempty_selection();

        let hovered_link_modifier = Editor::is_cmd_or_ctrl_pressed(&event.modifiers(), cx);
        let mouse_down_hovered_link_modifier = if let ClickEvent::Mouse(mouse_event) = event {
            Editor::is_cmd_or_ctrl_pressed(&mouse_event.down.modifiers, cx)
        } else {
            true
        };

        if let Some(mouse_position) = event.mouse_position()
            && !pending_nonempty_selections
            && hovered_link_modifier
            && mouse_down_hovered_link_modifier
            && text_hitbox.is_hovered(window)
            && !matches!(
                editor.selection_drag_state,
                SelectionDragState::Dragging { .. }
            )
        {
            let point = position_map.point_for_position(mouse_position);
            editor.handle_click_hovered_link(point, event.modifiers(), window, cx);
            editor.selection_drag_state = SelectionDragState::None;

            cx.stop_propagation();
        }
    }

    pub(super) fn pressure_click(
        editor: &mut Editor,
        event: &MousePressureEvent,
        position_map: &PositionMap,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) {
        let text_hitbox = &position_map.text_hitbox;
        let force_click_possible =
            matches!(editor.prev_pressure_stage, Some(PressureStage::Normal))
                && event.stage == PressureStage::Force;

        editor.prev_pressure_stage = Some(event.stage);

        if force_click_possible && text_hitbox.is_hovered(window) {
            let point = position_map.point_for_position(event.position);
            editor.handle_click_hovered_link(point, event.modifiers, window, cx);
            editor.selection_drag_state = SelectionDragState::None;
            cx.stop_propagation();
        }
    }
}
