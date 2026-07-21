use super::*;

impl EditorElement {
    pub(super) fn layout_bookmarks(
        &self,
        gutter: &Gutter<'_>,
        bookmarks: &HashSet<DisplayRow>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        if self.split_side == Some(SplitSide::Left) {
            return Vec::new();
        }

        self.editor.update(cx, |editor, cx| {
            bookmarks
                .iter()
                .filter_map(|row| {
                    gutter.layout_item_skipping_folds(
                        *row,
                        |cx, _| editor.render_bookmark(*row, cx).into_any_element(),
                        window,
                        cx,
                    )
                })
                .collect_vec()
        })
    }

    pub(super) fn layout_gutter_hover_button(
        &self,
        gutter: &Gutter,
        position: Anchor,
        row: DisplayRow,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        if self.split_side == Some(SplitSide::Left) {
            return None;
        }

        self.editor.update(cx, |editor, cx| {
            gutter.layout_item_skipping_folds(
                row,
                |cx, window| {
                    editor
                        .render_gutter_hover_button(position, row, window, cx)
                        .into_any_element()
                },
                window,
                cx,
            )
        })
    }

    pub(super) fn layout_breakpoints(
        &self,
        gutter: &Gutter,
        breakpoints: &HashMap<DisplayRow, (Anchor, Breakpoint, Option<BreakpointSessionState>)>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        if self.split_side == Some(SplitSide::Left) {
            return Vec::new();
        }

        self.editor.update(cx, |editor, cx| {
            breakpoints
                .iter()
                .filter_map(|(row, (text_anchor, bp, state))| {
                    gutter.layout_item_skipping_folds(
                        *row,
                        |cx, _| {
                            editor
                                .render_breakpoint(*text_anchor, *row, &bp, *state, cx)
                                .into_any_element()
                        },
                        window,
                        cx,
                    )
                })
                .collect_vec()
        })
    }

    pub(super) fn should_render_diff_review_button(
        &self,
        range: Range<DisplayRow>,
        row_infos: &[RowInfo],
        snapshot: &EditorSnapshot,
        cx: &App,
    ) -> Option<(DisplayRow, Option<u32>)> {
        if !cx.has_flag::<DiffReviewFeatureFlag>() {
            return None;
        }

        let show_diff_review_button = self.editor.read(cx).show_diff_review_button();
        if !show_diff_review_button {
            return None;
        }

        let indicator = self.editor.read(cx).gutter_diff_review_indicator.0?;
        if !indicator.is_active {
            return None;
        }

        let display_row = indicator
            .start
            .to_display_point(&snapshot.display_snapshot)
            .row();
        let row_index = (display_row.0.saturating_sub(range.start.0)) as usize;

        let row_info = row_infos.get(row_index);
        if row_info.is_some_and(|row_info| row_info.expand_info.is_some()) {
            return None;
        }

        let buffer_id = row_info.and_then(|info| info.buffer_id);
        if buffer_id.is_none() {
            return None;
        }

        let editor = self.editor.read(cx);
        if buffer_id.is_some_and(|buffer_id| editor.is_buffer_folded(buffer_id, cx)) {
            return None;
        }

        let buffer_row = row_info.and_then(|info| info.buffer_row);
        Some((display_row, buffer_row))
    }

    pub(super) fn layout_run_indicators(
        &self,
        gutter: &Gutter,
        run_indicators: &HashSet<DisplayRow>,
        breakpoints: &HashMap<DisplayRow, (Anchor, Breakpoint, Option<BreakpointSessionState>)>,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        if self.split_side == Some(SplitSide::Left) {
            return Vec::new();
        }

        self.editor.update(cx, |editor, cx| {
            let active_task_indicator_row =
                // TODO: add edit button on the right side of each row in the context menu
                if let Some(crate::CodeContextMenu::CodeActions(CodeActionsMenu {
                    deployed_from,
                    actions,
                    ..
                })) = editor.context_menu.borrow().as_ref()
                {
                    actions
                        .tasks()
                        .map(|tasks| tasks.position.to_display_point(gutter.snapshot).row())
                        .or_else(|| match deployed_from {
                            Some(CodeActionSource::Indicator(row)) => Some(*row),
                            _ => None,
                        })
                } else {
                    None
                };

            run_indicators
                .iter()
                .filter_map(|display_row| {
                    gutter.layout_item(
                        *display_row,
                        |cx, _| {
                            editor
                                .render_run_indicator(
                                    &self.style,
                                    Some(*display_row) == active_task_indicator_row,
                                    breakpoints.get(&display_row).map(|(anchor, _, _)| *anchor),
                                    *display_row,
                                    cx,
                                )
                                .into_any_element()
                        },
                        window,
                        cx,
                    )
                })
                .collect_vec()
        })
    }

    pub(super) fn layout_expand_toggles(
        &self,
        gutter_hitbox: &Hitbox,
        gutter_dimensions: GutterDimensions,
        em_width: Pixels,
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        start_row: DisplayRow,
        buffer_rows: &[RowInfo],
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Option<(AnyElement, gpui::Point<Pixels>)>> {
        if self.editor.read(cx).disable_expand_excerpt_buttons {
            return vec![];
        }

        let editor_font_size = self.style.text.font_size.to_pixels(window.rem_size()) * 1.2;

        let max_line_number_length = self
            .editor
            .read(cx)
            .buffer()
            .read(cx)
            .snapshot(cx)
            .widest_line_number()
            .ilog10()
            + 1;

        let git_gutter_width = Self::gutter_strip_width(line_height)
            + gutter_dimensions
                .git_blame_entries_width
                .unwrap_or_default();
        let available_width = gutter_dimensions.left_padding - git_gutter_width;

        buffer_rows
            .iter()
            .enumerate()
            .map(|(ix, row_info)| {
                let ExpandInfo {
                    direction,
                    start_anchor,
                } = row_info.expand_info?;

                let icon_name = match direction {
                    ExpandExcerptDirection::Up => IconName::ExpandUp,
                    ExpandExcerptDirection::Down => IconName::ExpandDown,
                    ExpandExcerptDirection::UpAndDown => IconName::ExpandVertical,
                };

                let editor = self.editor.clone();
                let is_wide = max_line_number_length
                    >= EditorSettings::get_global(cx).gutter.min_line_number_digits as u32
                    && row_info
                        .buffer_row
                        .is_some_and(|row| (row + 1).ilog10() + 1 == max_line_number_length)
                    || gutter_dimensions.right_padding == px(0.);

                let width = if is_wide {
                    available_width - px(5.)
                } else {
                    available_width + em_width - px(5.)
                };

                let toggle = IconButton::new(("expand", ix), icon_name)
                    .icon_color(Color::Custom(cx.theme().colors().editor_line_number))
                    .icon_size(IconSize::Custom(rems(editor_font_size / window.rem_size())))
                    .width(width)
                    .on_click(move |_, window, cx| {
                        editor.update(cx, |editor, cx| {
                            editor.expand_excerpt(start_anchor, direction, window, cx);
                        });
                    })
                    .tooltip(Tooltip::for_action_title(
                        "Expand Excerpt",
                        &crate::actions::ExpandExcerpts::default(),
                    ))
                    .into_any_element();

                let position = point(
                    git_gutter_width + px(1.),
                    line_height
                        * (DisplayRow(start_row.0 + ix as u32).as_f64() - scroll_position.y) as f32
                        + px(1.),
                );
                let origin = gutter_hitbox.origin + position;

                Some((toggle, origin))
            })
            .collect()
    }

    pub(super) fn layout_line_numbers(
        &self,
        gutter: &Gutter<'_>,
        active_rows: &BTreeMap<DisplayRow, LineHighlightSpec>,
        current_selection_head: Option<DisplayRow>,
        window: &mut Window,
        cx: &mut App,
    ) -> Arc<HashMap<MultiBufferRow, LineNumberLayout>> {
        let include_line_numbers = gutter
            .snapshot
            .show_line_numbers
            .unwrap_or_else(|| EditorSettings::get_global(cx).gutter.line_numbers);
        if !include_line_numbers {
            return Arc::default();
        }

        let relative = self.editor.read(cx).relative_line_numbers(cx);

        let relative_line_numbers_enabled = relative.enabled();
        let relative_rows = if relative_line_numbers_enabled
            && let Some(current_selection_head) = current_selection_head
        {
            gutter.snapshot.calculate_relative_line_numbers(
                &gutter.range,
                current_selection_head,
                relative.wrapped(),
            )
        } else {
            Default::default()
        };

        let mut line_number = String::new();
        let segments = gutter
            .row_infos
            .iter()
            .enumerate()
            .flat_map(|(ix, row_info)| {
                let display_row = DisplayRow(gutter.range.start.0 + ix as u32);
                line_number.clear();
                let non_relative_number = if relative.wrapped() {
                    row_info.buffer_row.or(row_info.wrapped_buffer_row)? + 1
                } else {
                    row_info.buffer_row? + 1
                };
                let relative_number = relative_rows.get(&display_row);
                if !(relative_line_numbers_enabled && relative_number.is_some())
                    && !gutter.snapshot.number_deleted_lines
                    && row_info
                        .diff_status
                        .is_some_and(|status| status.is_deleted())
                {
                    return None;
                }

                let number = relative_number.unwrap_or(&non_relative_number);
                write!(&mut line_number, "{number}").unwrap();

                let spec = active_rows.get(&display_row);
                let color = LineNumberStyle::new(
                    spec.is_some(),
                    spec.is_some_and(|spec| spec.breakpoint),
                    row_info.diff_status,
                )
                .color(cx.theme().colors());

                let shaped_line =
                    self.shape_line_number(SharedString::from(&line_number), color, window);
                let scroll_top =
                    gutter.scroll_position.y * ScrollPixelOffset::from(gutter.line_height);
                let line_origin = gutter.hitbox.origin
                    + point(
                        gutter.hitbox.size.width
                            - shaped_line.width
                            - gutter.dimensions.right_padding,
                        ix as f32 * gutter.line_height
                            - Pixels::from(
                                scroll_top % ScrollPixelOffset::from(gutter.line_height),
                            ),
                    );

                #[cfg(not(test))]
                let hitbox = Some(window.insert_hitbox(
                    Bounds::new(line_origin, size(shaped_line.width, gutter.line_height)),
                    HitboxBehavior::Normal,
                ));
                #[cfg(test)]
                let hitbox = {
                    let _ = line_origin;
                    None
                };

                let segment = LineNumberSegment {
                    shaped_line,
                    hitbox,
                };

                let buffer_row = DisplayPoint::new(display_row, 0)
                    .to_point(gutter.snapshot)
                    .row;
                let multi_buffer_row = MultiBufferRow(buffer_row);

                Some((multi_buffer_row, segment))
            });

        let mut line_numbers: HashMap<MultiBufferRow, LineNumberLayout> = HashMap::default();
        for (buffer_row, segment) in segments {
            line_numbers
                .entry(buffer_row)
                .or_insert_with(|| LineNumberLayout {
                    segments: Default::default(),
                })
                .segments
                .push(segment);
        }
        Arc::new(line_numbers)
    }

    pub(super) fn layout_crease_toggles(
        &self,
        rows: Range<DisplayRow>,
        row_infos: &[RowInfo],
        active_rows: &BTreeMap<DisplayRow, LineHighlightSpec>,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Option<AnyElement>> {
        let include_fold_statuses = EditorSettings::get_global(cx).gutter.folds
            && snapshot.mode.is_full()
            && snapshot.display_snapshot.companion_snapshot().is_none()
            && self.editor.read(cx).buffer_kind(cx) == ItemBufferKind::Singleton;
        if include_fold_statuses {
            row_infos
                .iter()
                .enumerate()
                .map(|(ix, info)| {
                    if info.expand_info.is_some() {
                        return None;
                    }
                    let row = info.multibuffer_row?;
                    let display_row = DisplayRow(rows.start.0 + ix as u32);
                    let active = active_rows.contains_key(&display_row);

                    snapshot.render_crease_toggle(row, active, self.editor.clone(), window, cx)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub(super) fn layout_crease_trailers(
        &self,
        buffer_rows: impl IntoIterator<Item = RowInfo>,
        snapshot: &EditorSnapshot,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Option<AnyElement>> {
        buffer_rows
            .into_iter()
            .map(|row_info| {
                if row_info.expand_info.is_some() {
                    return None;
                }
                if let Some(row) = row_info.multibuffer_row {
                    snapshot.render_crease_trailer(row, window, cx)
                } else {
                    None
                }
            })
            .collect()
    }
}
