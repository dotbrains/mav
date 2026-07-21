use super::*;

impl EditorElement {
    pub(super) fn paint_background(
        &self,
        layout: &EditorLayout,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.paint_layer(layout.hitbox.bounds, |window| {
            let scroll_top = layout.position_map.scroll_position.y;
            let gutter_bg = cx.theme().colors().editor_gutter_background;
            window.paint_quad(fill(layout.gutter_hitbox.bounds, gutter_bg));
            window.paint_quad(fill(
                layout.position_map.text_hitbox.bounds,
                self.style.background,
            ));

            if matches!(
                layout.mode,
                EditorMode::Full { .. } | EditorMode::Minimap { .. }
            ) {
                let show_active_line_background = match layout.mode {
                    EditorMode::Full {
                        show_active_line_background,
                        ..
                    } => show_active_line_background,
                    EditorMode::Minimap { .. } => true,
                    _ => false,
                };
                let mut active_rows = layout.active_rows.iter().peekable();
                while let Some((start_row, contains_non_empty_selection)) = active_rows.next() {
                    let mut end_row = start_row.0;
                    while active_rows
                        .peek()
                        .is_some_and(|(active_row, has_selection)| {
                            active_row.0 == end_row + 1
                                && has_selection.selection == contains_non_empty_selection.selection
                        })
                    {
                        active_rows.next().unwrap();
                        end_row += 1;
                    }

                    if show_active_line_background && !contains_non_empty_selection.selection {
                        let highlight_h_range =
                            match layout.position_map.snapshot.current_line_highlight {
                                CurrentLineHighlight::Gutter => Some(Range {
                                    start: layout.hitbox.left(),
                                    end: layout.gutter_hitbox.right(),
                                }),
                                CurrentLineHighlight::Line => Some(Range {
                                    start: layout.position_map.text_hitbox.bounds.left(),
                                    end: layout.position_map.text_hitbox.bounds.right(),
                                }),
                                CurrentLineHighlight::All => Some(Range {
                                    start: layout.hitbox.left(),
                                    end: layout.hitbox.right(),
                                }),
                                CurrentLineHighlight::None => None,
                            };
                        if let Some(range) = highlight_h_range {
                            let active_line_bg = cx.theme().colors().editor_active_line_background;
                            let bounds = Bounds {
                                origin: point(
                                    range.start,
                                    layout.hitbox.origin.y
                                        + Pixels::from(
                                            (start_row.as_f64() - scroll_top)
                                                * ScrollPixelOffset::from(
                                                    layout.position_map.line_height,
                                                ),
                                        ),
                                ),
                                size: size(
                                    range.end - range.start,
                                    layout.position_map.line_height
                                        * (end_row - start_row.0 + 1) as f32,
                                ),
                            };
                            window.paint_quad(fill(bounds, active_line_bg));
                        }
                    }
                }

                let mut paint_highlight = |highlight_row_start: DisplayRow,
                                           highlight_row_end: DisplayRow,
                                           highlight: crate::LineHighlight,
                                           edges| {
                    let mut origin_x = layout.hitbox.left();
                    let mut width = layout.hitbox.size.width;
                    if !highlight.include_gutter {
                        origin_x += layout.gutter_hitbox.size.width;
                        width -= layout.gutter_hitbox.size.width;
                    }

                    let origin = point(
                        origin_x,
                        layout.hitbox.origin.y
                            + Pixels::from(
                                (highlight_row_start.as_f64() - scroll_top)
                                    * ScrollPixelOffset::from(layout.position_map.line_height),
                            ),
                    );
                    let size = size(
                        width,
                        layout.position_map.line_height
                            * highlight_row_end.next_row().minus(highlight_row_start) as f32,
                    );
                    let mut quad = fill(Bounds { origin, size }, highlight.background);
                    if let Some(border_color) = highlight.border {
                        quad.border_color = border_color;
                        quad.border_widths = edges
                    }
                    window.paint_quad(quad);
                };

                let mut current_paint: Option<(LineHighlight, Range<DisplayRow>, Edges<Pixels>)> =
                    None;
                for (&new_row, &new_background) in &layout.highlighted_rows {
                    match &mut current_paint {
                        &mut Some((current_background, ref mut current_range, mut edges)) => {
                            let new_range_started = current_background != new_background
                                || current_range.end.next_row() != new_row;
                            if new_range_started {
                                if current_range.end.next_row() == new_row {
                                    edges.bottom = px(0.);
                                };
                                paint_highlight(
                                    current_range.start,
                                    current_range.end,
                                    current_background,
                                    edges,
                                );
                                let edges = Edges {
                                    top: if current_range.end.next_row() != new_row {
                                        px(1.)
                                    } else {
                                        px(0.)
                                    },
                                    bottom: px(1.),
                                    ..Default::default()
                                };
                                current_paint = Some((new_background, new_row..new_row, edges));
                                continue;
                            } else {
                                current_range.end = current_range.end.next_row();
                            }
                        }
                        None => {
                            let edges = Edges {
                                top: px(1.),
                                bottom: px(1.),
                                ..Default::default()
                            };
                            current_paint = Some((new_background, new_row..new_row, edges))
                        }
                    };
                }
                if let Some((color, range, edges)) = current_paint {
                    paint_highlight(range.start, range.end, color, edges);
                }

                for (guide_x, active) in layout.wrap_guides.iter() {
                    let color = if *active {
                        cx.theme().colors().editor_active_wrap_guide
                    } else {
                        cx.theme().colors().editor_wrap_guide
                    };
                    window.paint_quad(fill(
                        window.pixel_snap_bounds(Bounds {
                            origin: point(*guide_x, layout.position_map.text_hitbox.origin.y),
                            size: size(px(1.), layout.position_map.text_hitbox.size.height),
                        }),
                        color,
                    ));
                }
            }
        })
    }
}
