use super::*;

impl TerminalElement {
    pub(super) fn paint_terminal(
        &mut self,
        global_id: Option<&GlobalElementId>,
        inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut LayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask::new(bounds)), |window| {
            let scroll_top = self.terminal_view.read(cx).scroll_top;

            window.paint_quad(fill(bounds, layout.background_color));
            let origin = layout.dimensions.bounds.origin - GpuiPoint::new(px(0.), scroll_top);
            let scale_factor = window.scale_factor();
            let snap_px = |value: Pixels| {
                Pixels::from((f32::from(value) * scale_factor).floor() / scale_factor)
            };
            let origin = point(snap_px(origin.x), snap_px(origin.y));

            let marked_text_cloned: Option<String> = {
                let ime_state = &self.terminal_view.read(cx).ime_state;
                ime_state.as_ref().map(|state| state.marked_text.clone())
            };

            let terminal_input_handler = TerminalInputHandler {
                terminal: self.terminal.clone(),
                terminal_view: self.terminal_view.clone(),
                cursor_bounds: layout.ime_cursor_bounds.map(|bounds| bounds + origin),
                workspace: self.workspace.clone(),
            };

            self.register_mouse_listeners(
                layout.mode,
                &layout.hitbox,
                &layout.content_mode,
                window,
            );
            if window.modifiers().secondary()
                && bounds.contains(&window.mouse_position())
                && self.terminal_view.read(cx).hover.is_some()
            {
                window.set_cursor_style(gpui::CursorStyle::PointingHand, &layout.hitbox);
            } else {
                window.set_cursor_style(gpui::CursorStyle::IBeam, &layout.hitbox);
            }

            let original_cursor = layout.cursor.take();
            let hyperlink_tooltip = layout.hyperlink_tooltip.take();
            let block_below_cursor_element = layout.block_below_cursor_element.take();
            self.interactivity.paint(
                global_id,
                inspector_id,
                bounds,
                Some(&layout.hitbox),
                window,
                cx,
                |_, window, cx| {
                    window.handle_input(&self.focus, terminal_input_handler, cx);

                    window.on_key_event({
                        let this = self.terminal.clone();
                        move |event: &ModifiersChangedEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble {
                                return;
                            }

                            this.update(cx, |term, cx| {
                                term.try_modifiers_change(&event.modifiers, window, cx)
                            });
                        }
                    });

                    for rect in &layout.rects {
                        rect.paint(origin, &layout.dimensions, window);
                    }

                    for (relative_highlighted_range, color) in &layout.relative_highlighted_ranges {
                        if let Some((start_y, highlighted_range_lines)) =
                            to_highlighted_range_lines(relative_highlighted_range, layout, origin)
                        {
                            let corner_radius = if EditorSettings::get_global(cx).rounded_selection
                            {
                                0.15 * layout.dimensions.line_height
                            } else {
                                Pixels::ZERO
                            };
                            let hr = HighlightedRange {
                                start_y,
                                line_height: layout.dimensions.line_height,
                                lines: highlighted_range_lines,
                                color: *color,
                                corner_radius: corner_radius,
                            };
                            hr.paint(true, bounds, window);
                        }
                    }

                    // Paint batched text runs instead of individual cells
                    let text_paint_start = Instant::now();
                    for batch in &layout.batched_text_runs {
                        batch.paint(origin, &layout.dimensions, window, cx);
                    }
                    let text_paint_time = text_paint_start.elapsed();

                    if let Some(text_to_mark) = &marked_text_cloned
                        && !text_to_mark.is_empty()
                        && let Some(ime_bounds) = layout.ime_cursor_bounds
                    {
                        let ime_position = (ime_bounds + origin).origin;
                        let mut ime_style = layout.base_text_style.clone();
                        ime_style.underline = Some(UnderlineStyle {
                            color: Some(ime_style.color),
                            thickness: px(1.0),
                            wavy: false,
                        });

                        let shaped_line = window.text_system().shape_line(
                            text_to_mark.clone().into(),
                            ime_style.font_size.to_pixels(window.rem_size()),
                            &[TextRun {
                                len: text_to_mark.len(),
                                font: ime_style.font(),
                                color: ime_style.color,
                                underline: ime_style.underline,
                                ..Default::default()
                            }],
                            None,
                        );

                        // Paint background to cover terminal text behind marked text
                        let ime_background_bounds = Bounds::new(
                            ime_position,
                            size(shaped_line.width, layout.dimensions.line_height),
                        );
                        window.paint_quad(fill(ime_background_bounds, layout.background_color));

                        shaped_line
                            .paint(
                                ime_position,
                                layout.dimensions.line_height,
                                gpui::TextAlign::Left,
                                None,
                                window,
                                cx,
                            )
                            .log_err();
                    }

                    if self.cursor_visible
                        && marked_text_cloned.is_none()
                        && let Some(mut cursor) = original_cursor
                    {
                        cursor.paint(origin, window, cx);
                    }

                    if let Some(mut element) = block_below_cursor_element {
                        element.paint(window, cx);
                    }

                    if let Some(mut element) = hyperlink_tooltip {
                        element.paint(window, cx);
                    }

                    log::debug!(
                        "Terminal paint: {} text runs, {} rects, \
                        text paint took {:?}, total paint took {total_paint_time:?}",
                        layout.batched_text_runs.len(),
                        layout.rects.len(),
                        text_paint_time,
                        total_paint_time = paint_start.elapsed()
                    );
                },
            );
        });
    }
}
