use super::*;

impl EditorElement {
    pub(super) fn paint_gutter_diff_hunks(
        &self,
        layout: &mut EditorLayout,
        split_side: Option<SplitSide>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if layout.display_hunks.is_empty() {
            return;
        }

        let line_height = layout.position_map.line_height;
        window.paint_layer(layout.gutter_hitbox.bounds, |window| {
            for (hunk, hitbox) in &layout.display_hunks {
                let hunk_to_paint = match hunk {
                    DisplayDiffHunk::Folded { .. } => {
                        let hunk_bounds = Self::diff_hunk_bounds(
                            layout.position_map.scroll_position,
                            line_height,
                            layout.gutter_hitbox.bounds,
                            hunk,
                            &layout.position_map.snapshot,
                        );
                        Some((
                            hunk_bounds,
                            cx.theme().colors().version_control_modified,
                            Corners::all(px(0.)),
                            DiffHunkStatus::modified_none(),
                        ))
                    }
                    DisplayDiffHunk::Unfolded {
                        status,
                        display_row_range,
                        ..
                    } => hitbox.as_ref().map(|hunk_hitbox| {
                        let color = match split_side {
                            Some(SplitSide::Left) => cx.theme().colors().version_control_deleted,
                            Some(SplitSide::Right) => cx.theme().colors().version_control_added,
                            None => match status.kind {
                                DiffHunkStatusKind::Added => {
                                    cx.theme().colors().version_control_added
                                }
                                DiffHunkStatusKind::Modified => {
                                    cx.theme().colors().version_control_modified
                                }
                                DiffHunkStatusKind::Deleted => {
                                    cx.theme().colors().version_control_deleted
                                }
                            },
                        };
                        match status.kind {
                            DiffHunkStatusKind::Deleted if display_row_range.is_empty() => (
                                Bounds::new(
                                    point(
                                        hunk_hitbox.origin.x - hunk_hitbox.size.width,
                                        hunk_hitbox.origin.y,
                                    ),
                                    size(hunk_hitbox.size.width * 2., hunk_hitbox.size.height),
                                ),
                                color,
                                Corners::all(1. * line_height),
                                *status,
                            ),
                            _ => (hunk_hitbox.bounds, color, Corners::all(px(0.)), *status),
                        }
                    }),
                };

                if let Some((hunk_bounds, background_color, corner_radii, status)) = hunk_to_paint {
                    // Flatten the background color with the editor color to prevent
                    // elements below transparent hunks from showing through
                    let flattened_background_color = cx
                        .theme()
                        .colors()
                        .editor_background
                        .blend(background_color);

                    if !self.diff_hunk_hollow(status, cx) {
                        window.paint_quad(quad(
                            hunk_bounds,
                            corner_radii,
                            flattened_background_color,
                            Edges::default(),
                            transparent_black(),
                            BorderStyle::default(),
                        ));
                    } else {
                        let flattened_unstaged_background_color = cx
                            .theme()
                            .colors()
                            .editor_background
                            .blend(background_color.opacity(0.3));

                        window.paint_quad(quad(
                            hunk_bounds,
                            corner_radii,
                            flattened_unstaged_background_color,
                            Edges::all(px(1.0)),
                            flattened_background_color,
                            BorderStyle::Solid,
                        ));
                    }
                }
            }
        });
    }

    pub(super) fn gutter_strip_width(line_height: Pixels) -> Pixels {
        (0.275 * line_height).floor()
    }

    pub(super) fn diff_hunk_bounds(
        scroll_position: gpui::Point<ScrollOffset>,
        line_height: Pixels,
        gutter_bounds: Bounds<Pixels>,
        hunk: &DisplayDiffHunk,
        snapshot: &EditorSnapshot,
    ) -> Bounds<Pixels> {
        let scroll_top = scroll_position.y * ScrollPixelOffset::from(line_height);
        let gutter_strip_width = Self::gutter_strip_width(line_height);

        match hunk {
            DisplayDiffHunk::Folded { display_row, .. } => {
                let start_y = (display_row.as_f64() * ScrollPixelOffset::from(line_height)
                    - scroll_top)
                    .into();
                let end_y = start_y + line_height;
                let highlight_origin = gutter_bounds.origin + point(px(0.), start_y);
                let highlight_size = size(gutter_strip_width, end_y - start_y);
                Bounds::new(highlight_origin, highlight_size)
            }
            DisplayDiffHunk::Unfolded {
                display_row_range,
                status,
                ..
            } => {
                if status.is_deleted() && display_row_range.is_empty() {
                    let row = display_row_range.start;

                    let offset = ScrollPixelOffset::from(line_height / 2.);
                    let start_y =
                        (row.as_f64() * ScrollPixelOffset::from(line_height) - offset - scroll_top)
                            .into();
                    let end_y = start_y + line_height;

                    let width = (0.35 * line_height).floor();
                    let highlight_origin = gutter_bounds.origin + point(px(0.), start_y);
                    let highlight_size = size(width, end_y - start_y);
                    Bounds::new(highlight_origin, highlight_size)
                } else {
                    let start_row = display_row_range.start;
                    let end_row = display_row_range.end;
                    // If we're in a multibuffer, row range span might include an
                    // excerpt header, so if we were to draw the marker straight away,
                    // the hunk might include the rows of that header.
                    // Making the range inclusive doesn't quite cut it, as we rely on the exclusivity for the soft wrap.
                    // Instead, we simply check whether the range we're dealing with includes
                    // any excerpt headers and if so, we stop painting the diff hunk on the first row of that header.
                    let end_row_in_current_excerpt = snapshot
                        .blocks_in_range(start_row..end_row)
                        .find_map(|(start_row, block)| {
                            if matches!(
                                block,
                                Block::ExcerptBoundary { .. } | Block::BufferHeader { .. }
                            ) {
                                Some(start_row)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(end_row);

                    let start_y = (start_row.as_f64() * ScrollPixelOffset::from(line_height)
                        - scroll_top)
                        .into();
                    let end_y = Pixels::from(
                        end_row_in_current_excerpt.as_f64() * ScrollPixelOffset::from(line_height)
                            - scroll_top,
                    );

                    let highlight_origin = gutter_bounds.origin + point(px(0.), start_y);
                    let highlight_size = size(gutter_strip_width, end_y - start_y);
                    Bounds::new(highlight_origin, highlight_size)
                }
            }
        }
    }

    pub(super) fn paint_gutter_indicators(
        &self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_layer(layout.gutter_hitbox.bounds, |window| {
            window.with_element_namespace("crease_toggles", |window| {
                for crease_toggle in layout.crease_toggles.iter_mut().flatten() {
                    crease_toggle.paint(window, cx);
                }
            });

            window.with_element_namespace("expand_toggles", |window| {
                for (expand_toggle, _) in layout.expand_toggles.iter_mut().flatten() {
                    expand_toggle.paint(window, cx);
                }
            });

            for bookmark in layout.bookmarks.iter_mut() {
                bookmark.paint(window, cx);
            }

            for breakpoint in layout.breakpoints.iter_mut() {
                breakpoint.paint(window, cx);
            }

            for test_indicator in layout.test_indicators.iter_mut() {
                test_indicator.paint(window, cx);
            }

            if let Some(diff_review_button) = layout.diff_review_button.as_mut() {
                diff_review_button.paint(window, cx);
            }
        });
    }

    pub(super) fn paint_gutter_highlights(
        &self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        for (_, hunk_hitbox) in &layout.display_hunks {
            if let Some(hunk_hitbox) = hunk_hitbox
                && !self
                    .editor
                    .read(cx)
                    .buffer()
                    .read(cx)
                    .all_diff_hunks_expanded()
            {
                window.set_cursor_style(CursorStyle::PointingHand, hunk_hitbox);
            }
        }

        let show_git_gutter = layout
            .position_map
            .snapshot
            .show_git_diff_gutter
            .unwrap_or_else(|| {
                matches!(
                    ProjectSettings::get_global(cx).git.git_gutter,
                    GitGutterSetting::TrackedFiles
                )
            });
        if show_git_gutter {
            self.paint_gutter_diff_hunks(layout, self.split_side, window, cx)
        }

        let highlight_width = 0.275 * layout.position_map.line_height;
        let highlight_corner_radii = Corners::all(0.05 * layout.position_map.line_height);
        window.paint_layer(layout.gutter_hitbox.bounds, |window| {
            for (range, color) in &layout.highlighted_gutter_ranges {
                let start_row = if range.start.row() < layout.visible_display_row_range.start {
                    layout.visible_display_row_range.start - DisplayRow(1)
                } else {
                    range.start.row()
                };
                let end_row = if range.end.row() > layout.visible_display_row_range.end {
                    layout.visible_display_row_range.end + DisplayRow(1)
                } else {
                    range.end.row()
                };

                let start_y = layout.gutter_hitbox.top()
                    + Pixels::from(
                        start_row.0 as f64
                            * ScrollPixelOffset::from(layout.position_map.line_height)
                            - layout.position_map.scroll_pixel_position.y,
                    );
                let end_y = layout.gutter_hitbox.top()
                    + Pixels::from(
                        (end_row.0 + 1) as f64
                            * ScrollPixelOffset::from(layout.position_map.line_height)
                            - layout.position_map.scroll_pixel_position.y,
                    );
                let bounds = Bounds::from_corners(
                    point(layout.gutter_hitbox.left(), start_y),
                    point(layout.gutter_hitbox.left() + highlight_width, end_y),
                );
                window.paint_quad(fill(bounds, *color).corner_radii(highlight_corner_radii));
            }
        });
    }

    pub(super) fn paint_blamed_display_rows(
        &self,
        layout: &mut EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(blamed_display_rows) = layout.blamed_display_rows.take() else {
            return;
        };

        window.paint_layer(layout.gutter_hitbox.bounds, |window| {
            for mut blame_element in blamed_display_rows.into_iter() {
                blame_element.paint(window, cx);
            }
        })
    }
}

impl EditorElement {
    pub(super) fn diff_hunk_hollow(&self, status: DiffHunkStatus, cx: &mut App) -> bool {
        let unstaged =
            self.editor.read(cx).render_diff_hunks_as_unstaged || status.has_secondary_hunk();
        let unstaged_hollow = matches!(
            ProjectSettings::get_global(cx).git.hunk_style,
            GitHunkStyleSetting::UnstagedHollow
        );

        unstaged == unstaged_hollow
    }
}
