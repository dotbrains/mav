use super::*;

impl EditorElement {
    pub(super) fn layout_selections(
        &self,
        start_anchor: Anchor,
        end_anchor: Anchor,
        local_selections: &[Selection<Point>],
        snapshot: &EditorSnapshot,
        start_row: DisplayRow,
        end_row: DisplayRow,
        window: &mut Window,
        cx: &mut App,
    ) -> (
        Vec<(PlayerColor, Vec<SelectionLayout>)>,
        BTreeMap<DisplayRow, LineHighlightSpec>,
        Option<DisplayPoint>,
    ) {
        let mut selections: Vec<(PlayerColor, Vec<SelectionLayout>)> = Vec::new();
        let mut active_rows = BTreeMap::new();
        let mut newest_selection_head = None;

        let Some(editor_with_selections) = self.editor_with_selections(cx) else {
            return (selections, active_rows, newest_selection_head);
        };

        editor_with_selections.update(cx, |editor, cx| {
            if editor.show_local_selections {
                let mut layouts = Vec::new();
                let newest = editor.selections.newest(&editor.display_snapshot(cx));
                for selection in local_selections.iter().cloned() {
                    let is_empty = selection.start == selection.end;
                    let is_newest = selection == newest;

                    let layout = SelectionLayout::new(
                        selection,
                        editor.selections.line_mode(),
                        editor.cursor_offset_on_selection,
                        editor.cursor_shape,
                        &snapshot.display_snapshot,
                        is_newest,
                        editor.leader_id.is_none(),
                        None,
                    );
                    if is_newest {
                        newest_selection_head = Some(layout.head);
                    }

                    for row in cmp::max(layout.active_rows.start.0, start_row.0)
                        ..=cmp::min(layout.active_rows.end.0, end_row.0)
                    {
                        let contains_non_empty_selection = active_rows
                            .entry(DisplayRow(row))
                            .or_insert_with(LineHighlightSpec::default);
                        contains_non_empty_selection.selection |= !is_empty;
                    }
                    layouts.push(layout);
                }

                let mut player = editor.current_user_player_color(cx);
                if !editor.is_focused(window) {
                    const UNFOCUS_EDITOR_SELECTION_OPACITY: f32 = 0.5;
                    player.selection = player.selection.opacity(UNFOCUS_EDITOR_SELECTION_OPACITY);
                }
                selections.push((player, layouts));

                if let SelectionDragState::Dragging {
                    ref selection,
                    ref drop_cursor,
                    ref hide_drop_cursor,
                } = editor.selection_drag_state
                    && !hide_drop_cursor
                    && (drop_cursor
                        .start
                        .cmp(&selection.start, &snapshot.buffer_snapshot())
                        .eq(&Ordering::Less)
                        || drop_cursor
                            .end
                            .cmp(&selection.end, &snapshot.buffer_snapshot())
                            .eq(&Ordering::Greater))
                {
                    let drag_cursor_layout = SelectionLayout::new(
                        drop_cursor.clone(),
                        false,
                        editor.cursor_offset_on_selection,
                        CursorShape::Bar,
                        &snapshot.display_snapshot,
                        false,
                        false,
                        None,
                    );
                    let absent_color = cx.theme().players().absent();
                    selections.push((absent_color, vec![drag_cursor_layout]));
                }
            }

            if let Some(collaboration_hub) = &editor.collaboration_hub {
                // When following someone, render the local selections in their color.
                if let Some(leader_id) = editor.leader_id {
                    match leader_id {
                        CollaboratorId::PeerId(peer_id) => {
                            if let Some(collaborator) =
                                collaboration_hub.collaborators(cx).get(&peer_id)
                                && let Some(participant_index) = collaboration_hub
                                    .user_participant_indices(cx)
                                    .get(&collaborator.user_id)
                                && let Some((local_selection_style, _)) = selections.first_mut()
                            {
                                *local_selection_style = cx
                                    .theme()
                                    .players()
                                    .color_for_participant(participant_index.0);
                            }
                        }
                        CollaboratorId::Agent => {
                            if let Some((local_selection_style, _)) = selections.first_mut() {
                                *local_selection_style = cx.theme().players().agent();
                            }
                        }
                    }
                }

                let mut remote_selections = HashMap::default();
                for selection in snapshot.remote_selections_in_range(
                    &(start_anchor..end_anchor),
                    collaboration_hub.as_ref(),
                    cx,
                ) {
                    // Don't re-render the leader's selections, since the local selections
                    // match theirs.
                    if Some(selection.collaborator_id) == editor.leader_id {
                        continue;
                    }
                    let key = HoveredCursor {
                        replica_id: selection.replica_id,
                        selection_id: selection.selection.id,
                    };

                    let is_shown =
                        editor.show_cursor_names || editor.hovered_cursors.contains_key(&key);

                    remote_selections
                        .entry(selection.replica_id)
                        .or_insert((selection.color, Vec::new()))
                        .1
                        .push(SelectionLayout::new(
                            selection.selection,
                            selection.line_mode,
                            editor.cursor_offset_on_selection,
                            selection.cursor_shape,
                            &snapshot.display_snapshot,
                            false,
                            false,
                            if is_shown { selection.user_name } else { None },
                        ));
                }

                selections.extend(remote_selections.into_values());
            } else if !editor.is_focused(window) && editor.show_cursor_when_unfocused {
                let cursor_offset_on_selection = editor.cursor_offset_on_selection;

                let layouts = snapshot
                    .buffer_snapshot()
                    .selections_in_range(&(start_anchor..end_anchor), true)
                    .map(move |(_, line_mode, cursor_shape, selection)| {
                        SelectionLayout::new(
                            selection,
                            line_mode,
                            cursor_offset_on_selection,
                            cursor_shape,
                            &snapshot.display_snapshot,
                            false,
                            false,
                            None,
                        )
                    })
                    .collect::<Vec<_>>();
                let player = editor.current_user_player_color(cx);
                selections.push((player, layouts));
            }
        });

        #[cfg(debug_assertions)]
        Self::layout_debug_ranges(
            &mut selections,
            start_anchor..end_anchor,
            &snapshot.display_snapshot,
            cx,
        );

        (selections, active_rows, newest_selection_head)
    }

    pub(super) fn collect_cursors(
        &self,
        snapshot: &EditorSnapshot,
        cx: &mut App,
    ) -> Vec<(DisplayPoint, Hsla)> {
        let editor = self.editor.read(cx);
        let mut cursors = Vec::new();
        let mut skip_local = false;
        let mut add_cursor = |anchor: Anchor, color| {
            cursors.push((anchor.to_display_point(&snapshot.display_snapshot), color));
        };
        // Remote cursors
        if let Some(collaboration_hub) = &editor.collaboration_hub {
            for remote_selection in snapshot.remote_selections_in_range(
                &(Anchor::Min..Anchor::Max),
                collaboration_hub.deref(),
                cx,
            ) {
                add_cursor(
                    remote_selection.selection.head(),
                    remote_selection.color.cursor,
                );
                if Some(remote_selection.collaborator_id) == editor.leader_id {
                    skip_local = true;
                }
            }
        }
        // Local cursors
        if !skip_local {
            let color = cx.theme().players().local().cursor;
            editor
                .selections
                .disjoint_anchors()
                .iter()
                .for_each(|selection| {
                    add_cursor(selection.head(), color);
                });
            if let Some(ref selection) = editor.selections.pending_anchor() {
                add_cursor(selection.head(), color);
            }
        }
        cursors
    }

    pub(super) fn layout_visible_cursors(
        &self,
        snapshot: &EditorSnapshot,
        selections: &[(PlayerColor, Vec<SelectionLayout>)],
        row_block_types: &HashMap<DisplayRow, bool>,
        visible_display_row_range: Range<DisplayRow>,
        line_layouts: &[LineWithInvisibles],
        text_hitbox: &Hitbox,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        em_width: Pixels,
        em_advance: Pixels,
        autoscroll_containing_element: bool,
        redacted_ranges: &[Range<DisplayPoint>],
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<CursorLayout> {
        let mut autoscroll_bounds = None;
        let cursor_layouts = self.editor.update(cx, |editor, cx| {
            let mut cursors = Vec::new();

            let show_local_cursors = editor.show_local_cursors(window, cx);

            for (player_color, selections) in selections {
                for selection in selections {
                    let cursor_position = selection.head;

                    let in_range = visible_display_row_range.contains(&cursor_position.row());
                    if (selection.is_local && !show_local_cursors)
                        || !in_range
                        || row_block_types.get(&cursor_position.row()) == Some(&true)
                    {
                        continue;
                    }

                    let cursor_row_layout = &line_layouts
                        [cursor_position.row().minus(visible_display_row_range.start) as usize];
                    let cursor_column = cursor_position.column() as usize;

                    let cursor_character_x = cursor_row_layout.x_for_index(cursor_column)
                        + cursor_row_layout
                            .alignment_offset(self.style.text.text_align, text_hitbox.size.width);
                    let cursor_next_x = cursor_row_layout.x_for_index(cursor_column + 1)
                        + cursor_row_layout
                            .alignment_offset(self.style.text.text_align, text_hitbox.size.width);
                    let mut cell_width = cursor_next_x - cursor_character_x;
                    if cell_width == Pixels::ZERO {
                        cell_width = em_advance;
                    }

                    let mut block_width = cell_width;
                    let mut block_text = None;

                    let is_cursor_in_redacted_range = redacted_ranges
                        .iter()
                        .any(|range| range.start <= cursor_position && cursor_position < range.end);

                    if selection.cursor_shape == CursorShape::Block && !is_cursor_in_redacted_range
                    {
                        if let Some(text) = snapshot.grapheme_at(cursor_position).or_else(|| {
                            if snapshot.is_empty() {
                                snapshot.placeholder_text().and_then(|s| {
                                    s.graphemes(true).next().map(|s| s.to_string().into())
                                })
                            } else {
                                None
                            }
                        }) {
                            let is_ascii_whitespace_only =
                                text.as_ref().chars().all(|c| c.is_ascii_whitespace());
                            let len = text.len();

                            let mut font = cursor_row_layout
                                .font_id_for_index(cursor_column)
                                .and_then(|cursor_font_id| {
                                    window.text_system().get_font_for_id(cursor_font_id)
                                })
                                .unwrap_or(self.style.text.font());
                            font.features = self.style.text.font_features.clone();

                            // Invert the text color for the block cursor. Ensure that the text
                            // color is opaque enough to be visible against the background color.
                            //
                            // 0.75 is an arbitrary threshold to determine if the background color is
                            // opaque enough to use as a text color.
                            //
                            // TODO: In the future we should ensure themes have a `text_inverse` color.
                            let color = if cx.theme().colors().editor_background.a < 0.75 {
                                match cx.theme().appearance {
                                    Appearance::Dark => Hsla::black(),
                                    Appearance::Light => Hsla::white(),
                                }
                            } else {
                                cx.theme().colors().editor_background
                            };

                            let shaped = window.text_system().shape_line(
                                text,
                                cursor_row_layout.font_size,
                                &[TextRun {
                                    len,
                                    font,
                                    color,
                                    ..Default::default()
                                }],
                                None,
                            );
                            if !is_ascii_whitespace_only {
                                block_width = block_width.max(shaped.width);
                            }
                            block_text = Some(shaped);
                        }
                    }

                    let x = cursor_character_x - scroll_pixel_position.x.into();
                    let y = ((cursor_position.row().as_f64() - scroll_position.y)
                        * ScrollPixelOffset::from(line_height))
                    .into();
                    if selection.is_newest {
                        editor.pixel_position_of_newest_cursor = Some(point(
                            text_hitbox.origin.x + x + block_width / 2.,
                            text_hitbox.origin.y + y + line_height / 2.,
                        ));

                        if autoscroll_containing_element {
                            let top = text_hitbox.origin.y
                                + ((cursor_position.row().as_f64() - scroll_position.y - 3.)
                                    .max(0.)
                                    * ScrollPixelOffset::from(line_height))
                                .into();
                            let left = text_hitbox.origin.x
                                + ((cursor_position.column() as ScrollOffset
                                    - scroll_position.x
                                    - 3.)
                                    .max(0.)
                                    * ScrollPixelOffset::from(em_width))
                                .into();

                            let bottom = text_hitbox.origin.y
                                + ((cursor_position.row().as_f64() - scroll_position.y + 4.)
                                    * ScrollPixelOffset::from(line_height))
                                .into();
                            let right = text_hitbox.origin.x
                                + ((cursor_position.column() as ScrollOffset - scroll_position.x
                                    + 4.)
                                    * ScrollPixelOffset::from(em_width))
                                .into();

                            autoscroll_bounds =
                                Some(Bounds::from_corners(point(left, top), point(right, bottom)))
                        }
                    }

                    let mut cursor = CursorLayout {
                        color: player_color.cursor,
                        block_width,
                        origin: point(x, y),
                        line_height,
                        shape: selection.cursor_shape,
                        block_text,
                        cursor_name: None,
                    };
                    let cursor_name = selection.user_name.clone().map(|name| CursorName {
                        string: name,
                        color: self.style.background,
                        is_top_row: cursor_position.row().0 == 0,
                    });
                    cursor.layout(content_origin, cursor_name, window, cx);
                    cursors.push(cursor);
                }
            }

            cursors
        });

        if let Some(bounds) = autoscroll_bounds {
            window.request_autoscroll(bounds);
        }

        cursor_layouts
    }

    #[cfg(debug_assertions)]
    fn layout_debug_ranges(
        selections: &mut Vec<(PlayerColor, Vec<SelectionLayout>)>,
        anchor_range: Range<Anchor>,
        display_snapshot: &DisplaySnapshot,
        cx: &App,
    ) {
        let theme = cx.theme();
        text::debug::GlobalDebugRanges::with_locked(|debug_ranges| {
            if debug_ranges.ranges.is_empty() {
                return;
            }
            let buffer_snapshot = &display_snapshot.buffer_snapshot();
            for (excerpt_buffer_snapshot, buffer_range, _) in
                buffer_snapshot.range_to_buffer_ranges(anchor_range.start..anchor_range.end)
            {
                let buffer_range = excerpt_buffer_snapshot.anchor_after(buffer_range.start)
                    ..excerpt_buffer_snapshot.anchor_before(buffer_range.end);
                selections.extend(debug_ranges.ranges.iter().flat_map(|debug_range| {
                    debug_range.ranges.iter().filter_map(|range| {
                        let player_color = theme
                            .players()
                            .color_for_participant(debug_range.occurrence_index as u32 + 1);
                        if range.start.buffer_id != excerpt_buffer_snapshot.remote_id() {
                            return None;
                        }
                        let clipped_start = range
                            .start
                            .max(&buffer_range.start, &excerpt_buffer_snapshot);
                        let clipped_end =
                            range.end.min(&buffer_range.end, &excerpt_buffer_snapshot);
                        let range = buffer_snapshot
                            .buffer_anchor_range_to_anchor_range(*clipped_start..*clipped_end)?;
                        let start = range.start.to_display_point(display_snapshot);
                        let end = range.end.to_display_point(display_snapshot);
                        let selection_layout = SelectionLayout {
                            head: start,
                            range: start..end,
                            cursor_shape: CursorShape::Bar,
                            is_newest: false,
                            is_local: false,
                            active_rows: start.row()..end.row(),
                            user_name: Some(SharedString::new(debug_range.value.clone())),
                        };
                        Some((player_color, vec![selection_layout]))
                    })
                }));
            }
        });
    }
}
