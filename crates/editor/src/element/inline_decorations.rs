use super::*;

impl EditorElement {
    pub(super) fn layout_inline_decorations(
        &self,
        line_layouts: &[LineWithInvisibles],
        crease_trailers: &[Option<CreaseTrailerLayout>],
        row_block_types: &HashMap<DisplayRow, bool>,
        row_infos: &[RowInfo],
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        edit_prediction_popover_origin: Option<gpui::Point<Pixels>>,
        newest_selection_head: Option<DisplayPoint>,
        start_row: DisplayRow,
        end_row: DisplayRow,
        line_height: Pixels,
        em_width: Pixels,
        style: &EditorStyle,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> layout_data::InlineDecorationLayouts {
        let mut inline_diagnostics = self.layout_inline_diagnostics(
            line_layouts,
            crease_trailers,
            row_block_types,
            content_origin,
            scroll_position,
            scroll_pixel_position,
            edit_prediction_popover_origin,
            start_row,
            end_row,
            line_height,
            em_width,
            style,
            window,
            cx,
        );

        let mut inline_blame_layout = None;
        let mut inline_code_actions = None;
        if let Some(newest_selection_head) = newest_selection_head {
            let display_row = newest_selection_head.row();
            if (start_row..end_row).contains(&display_row)
                && !row_block_types.contains_key(&display_row)
            {
                inline_code_actions = self.layout_inline_code_actions(
                    newest_selection_head,
                    content_origin,
                    scroll_position,
                    scroll_pixel_position,
                    line_height,
                    snapshot,
                    window,
                    cx,
                );

                let line_ix = display_row.minus(start_row) as usize;
                if let (Some(row_info), Some(line_layout), Some(crease_trailer)) = (
                    row_infos.get(line_ix),
                    line_layouts.get(line_ix),
                    crease_trailers.get(line_ix),
                ) {
                    let crease_trailer_layout = crease_trailer.as_ref();
                    if let Some(layout) = self.layout_inline_blame(
                        display_row,
                        row_info,
                        line_layout,
                        crease_trailer_layout,
                        em_width,
                        content_origin,
                        scroll_position,
                        scroll_pixel_position,
                        line_height,
                        window,
                        cx,
                    ) {
                        inline_blame_layout = Some(layout);
                        // Blame overrides inline diagnostics.
                        inline_diagnostics.remove(&display_row);
                    }
                } else {
                    log::error!(
                        "bug: line_ix {} is out of bounds - row_infos.len(): {}, \
                        line_layouts.len(): {}, crease_trailers.len(): {}",
                        line_ix,
                        row_infos.len(),
                        line_layouts.len(),
                        crease_trailers.len(),
                    );
                }
            }
        }

        layout_data::InlineDecorationLayouts {
            inline_diagnostics,
            inline_blame_layout,
            inline_code_actions,
        }
    }

    pub(super) fn layout_inline_diagnostics(
        &self,
        line_layouts: &[LineWithInvisibles],
        crease_trailers: &[Option<CreaseTrailerLayout>],
        row_block_types: &HashMap<DisplayRow, bool>,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        edit_prediction_popover_origin: Option<gpui::Point<Pixels>>,
        start_row: DisplayRow,
        end_row: DisplayRow,
        line_height: Pixels,
        em_width: Pixels,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> HashMap<DisplayRow, AnyElement> {
        let max_severity = match self
            .editor
            .read(cx)
            .inline_diagnostics_enabled()
            .then(|| {
                ProjectSettings::get_global(cx)
                    .diagnostics
                    .inline
                    .max_severity
                    .unwrap_or_else(|| self.editor.read(cx).diagnostics_max_severity)
                    .into_lsp()
            })
            .flatten()
        {
            Some(max_severity) => max_severity,
            None => return HashMap::default(),
        };

        let active_diagnostics_group = self.editor.read(cx).active_diagnostic_group_id();

        let diagnostics_by_rows = self.editor.update(cx, |editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            editor
                .inline_diagnostics
                .iter()
                .filter(|(_, diagnostic)| diagnostic.severity <= max_severity)
                .filter(|(_, diagnostic)| match active_diagnostics_group {
                    Some(active_diagnostics_group) => {
                        // Active diagnostics are all shown in the editor already, no need to display them inline
                        diagnostic.group_id != active_diagnostics_group
                    }
                    None => true,
                })
                .map(|(point, diag)| (point.to_display_point(&snapshot), diag.clone()))
                .skip_while(|(point, _)| point.row() < start_row)
                .take_while(|(point, _)| point.row() < end_row)
                .filter(|(point, _)| !row_block_types.contains_key(&point.row()))
                .fold(HashMap::default(), |mut acc, (point, diagnostic)| {
                    acc.entry(point.row())
                        .or_insert_with(Vec::new)
                        .push(diagnostic);
                    acc
                })
        });

        if diagnostics_by_rows.is_empty() {
            return HashMap::default();
        }

        let severity_to_color = |sev: &lsp::DiagnosticSeverity| match sev {
            &lsp::DiagnosticSeverity::ERROR => Color::Error,
            &lsp::DiagnosticSeverity::WARNING => Color::Warning,
            &lsp::DiagnosticSeverity::INFORMATION => Color::Info,
            &lsp::DiagnosticSeverity::HINT => Color::Hint,
            _ => Color::Error,
        };

        let padding = ProjectSettings::get_global(cx).diagnostics.inline.padding as f32 * em_width;
        let min_x = column_pixels(
            &self.style,
            ProjectSettings::get_global(cx)
                .diagnostics
                .inline
                .min_column as usize,
            window,
        );

        let mut elements = HashMap::default();
        for (row, mut diagnostics) in diagnostics_by_rows {
            diagnostics.sort_by_key(|diagnostic| {
                (
                    diagnostic.severity,
                    std::cmp::Reverse(diagnostic.is_primary),
                    diagnostic.start.row,
                    diagnostic.start.column,
                )
            });

            let Some(diagnostic_to_render) = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.is_primary)
                .or_else(|| diagnostics.first())
            else {
                continue;
            };

            let pos_y = content_origin.y + line_height * (row.0 as f64 - scroll_position.y) as f32;

            let window_ix = row.0.saturating_sub(start_row.0) as usize;
            let pos_x = {
                let crease_trailer_layout = &crease_trailers[window_ix];
                let line_layout = &line_layouts[window_ix];

                let line_end = if let Some(crease_trailer) = crease_trailer_layout {
                    crease_trailer.bounds.right()
                } else {
                    Pixels::from(
                        ScrollPixelOffset::from(content_origin.x + line_layout.width)
                            - scroll_pixel_position.x,
                    )
                };

                let padded_line = line_end + padding;
                let min_start = Pixels::from(
                    ScrollPixelOffset::from(content_origin.x + min_x) - scroll_pixel_position.x,
                );

                cmp::max(padded_line, min_start)
            };

            let behind_edit_prediction_popover = edit_prediction_popover_origin
                .as_ref()
                .is_some_and(|edit_prediction_popover_origin| {
                    (pos_y..pos_y + line_height).contains(&edit_prediction_popover_origin.y)
                });
            let opacity = if behind_edit_prediction_popover {
                0.5
            } else {
                1.0
            };

            let mut element = h_flex()
                .id(("diagnostic", row.0))
                .h(line_height)
                .w_full()
                .px_1()
                .rounded_xs()
                .opacity(opacity)
                .bg(severity_to_color(&diagnostic_to_render.severity)
                    .color(cx)
                    .opacity(0.05))
                .text_color(severity_to_color(&diagnostic_to_render.severity).color(cx))
                .text_sm()
                .font(style.text.font())
                .child(diagnostic_to_render.message.clone())
                .into_any();

            element.prepaint_as_root(point(pos_x, pos_y), AvailableSpace::min_size(), window, cx);

            elements.insert(row, element);
        }

        elements
    }

    pub(super) fn layout_inline_code_actions(
        &self,
        display_point: DisplayPoint,
        content_origin: gpui::Point<Pixels>,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        line_height: Pixels,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        // Don't show code actions in split diff view
        if self.split_side.is_some() {
            return None;
        }

        if !snapshot
            .show_code_actions
            .unwrap_or(EditorSettings::get_global(cx).inline_code_actions)
        {
            return None;
        }

        let icon_size = ui::IconSize::XSmall;
        let mut button = self.editor.update(cx, |editor, cx| {
            if !editor.has_available_code_actions_for_selection() {
                return None;
            }
            let active = editor
                .context_menu
                .borrow()
                .as_ref()
                .and_then(|menu| {
                    if let crate::CodeContextMenu::CodeActions(CodeActionsMenu {
                        deployed_from,
                        ..
                    }) = menu
                    {
                        deployed_from.as_ref()
                    } else {
                        None
                    }
                })
                .is_some_and(|source| matches!(source, CodeActionSource::Indicator(..)));
            Some(editor.render_inline_code_actions(icon_size, display_point.row(), active, cx))
        })?;

        let buffer_point = display_point.to_point(&snapshot.display_snapshot);

        // do not show code action for folded line
        if snapshot.is_line_folded(MultiBufferRow(buffer_point.row)) {
            return None;
        }

        // do not show code action for blank line with cursor
        let line_indent = snapshot
            .display_snapshot
            .buffer_snapshot()
            .line_indent_for_row(MultiBufferRow(buffer_point.row));
        if line_indent.is_line_blank() {
            return None;
        }

        const INLINE_SLOT_CHAR_LIMIT: u32 = 4;
        const MAX_ALTERNATE_DISTANCE: u32 = 8;

        let is_valid_row = |row_candidate: u32| -> bool {
            // move to other row if folded row
            if snapshot.is_line_folded(MultiBufferRow(row_candidate)) {
                return false;
            }
            if buffer_point.row == row_candidate {
                // move to other row if cursor is in slot
                if buffer_point.column < INLINE_SLOT_CHAR_LIMIT {
                    return false;
                }
            } else {
                let candidate_point = MultiBufferPoint {
                    row: row_candidate,
                    column: 0,
                };
                // move to other row if different excerpt
                let range = if candidate_point < buffer_point {
                    candidate_point..buffer_point
                } else {
                    buffer_point..candidate_point
                };
                if snapshot
                    .display_snapshot
                    .buffer_snapshot()
                    .excerpt_containing(range)
                    .is_none()
                {
                    return false;
                }
            }
            let line_indent = snapshot
                .display_snapshot
                .buffer_snapshot()
                .line_indent_for_row(MultiBufferRow(row_candidate));
            // use this row if it's blank
            if line_indent.is_line_blank() {
                true
            } else {
                // use this row if code starts after slot
                let indent_size = snapshot
                    .display_snapshot
                    .buffer_snapshot()
                    .indent_size_for_line(MultiBufferRow(row_candidate));
                indent_size.len >= INLINE_SLOT_CHAR_LIMIT
            }
        };

        let new_buffer_row = if is_valid_row(buffer_point.row) {
            Some(buffer_point.row)
        } else {
            let max_row = snapshot.display_snapshot.buffer_snapshot().max_point().row;
            (1..=MAX_ALTERNATE_DISTANCE).find_map(|offset| {
                let row_above = buffer_point.row.saturating_sub(offset);
                let row_below = buffer_point.row + offset;
                if row_above != buffer_point.row && is_valid_row(row_above) {
                    Some(row_above)
                } else if row_below <= max_row && is_valid_row(row_below) {
                    Some(row_below)
                } else {
                    None
                }
            })
        }?;

        let new_display_row = snapshot
            .display_snapshot
            .point_to_display_point(
                Point {
                    row: new_buffer_row,
                    column: buffer_point.column,
                },
                text::Bias::Left,
            )
            .row();

        let start_y = content_origin.y
            + (((new_display_row.as_f64() - scroll_position.y) as f32) * line_height)
            + (line_height / 2.0)
            - (icon_size.square(window, cx) / 2.);
        let start_x = (ScrollPixelOffset::from(content_origin.x) - scroll_pixel_position.x
            + ScrollPixelOffset::from(window.rem_size() * 0.1))
        .into();

        let absolute_offset = gpui::point(start_x, start_y);
        button.layout_as_root(gpui::AvailableSpace::min_size(), window, cx);
        button.prepaint_as_root(
            absolute_offset,
            gpui::AvailableSpace::min_size(),
            window,
            cx,
        );
        Some(button)
    }
}
