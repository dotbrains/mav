use super::super::*;
use super::MarkdownElement;

impl MarkdownElement {
    pub(super) fn paint_highlight_range(
        start: usize,
        end: usize,
        color: Hsla,
        rendered_text: &RenderedText,
        window: &mut Window,
    ) {
        for bounds in rendered_text.bounds_for_source_range(start..end) {
            window.paint_quad(quad(
                bounds,
                Pixels::ZERO,
                color,
                Edges::default(),
                Hsla::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }

    pub(super) fn paint_selection(
        &self,
        rendered_text: &RenderedText,
        window: &mut Window,
        cx: &mut App,
    ) {
        let selection = self.markdown.read(cx).selection.clone();
        Self::paint_highlight_range(
            selection.start,
            selection.end,
            self.style.selection_background_color,
            rendered_text,
            window,
        );
    }

    pub(super) fn paint_search_highlights(
        &self,
        rendered_text: &RenderedText,
        window: &mut Window,
        cx: &mut App,
    ) {
        let markdown = self.markdown.read(cx);
        let active_index = markdown.active_search_highlight;
        let colors = cx.theme().colors();

        let highlight_bounds = rendered_text.bounds_for_sorted_source_ranges(
            markdown
                .search_highlights
                .iter()
                .enumerate()
                .map(|(ix, range)| (ix, range.clone())),
        );
        for (highlight_ix, bounds) in highlight_bounds {
            let color = if Some(highlight_ix) == active_index {
                colors.search_active_match_background
            } else {
                colors.search_match_background
            };
            window.paint_quad(quad(
                bounds,
                Pixels::ZERO,
                color,
                Edges::default(),
                Hsla::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }

    pub(super) fn paint_mouse_listeners(
        &mut self,
        hitbox: &Hitbox,
        rendered_text: &RenderedText,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.style.prevent_mouse_interaction {
            return;
        }

        let is_hovering_clickable = hitbox.is_hovered(window)
            && !self.markdown.read(cx).selection.pending
            && rendered_text
                .source_index_for_position(window.mouse_position())
                .ok()
                .is_some_and(|source_index| {
                    rendered_text.link_for_source_index(source_index).is_some()
                        || rendered_text
                            .footnote_ref_for_source_index(source_index)
                            .is_some()
                });

        if is_hovering_clickable {
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        } else {
            window.set_cursor_style(CursorStyle::IBeam, hitbox);
        }

        let on_open_url = self.on_url_click.take();
        let on_source_click = self.on_source_click.take();

        self.on_mouse_event(window, cx, {
            let hitbox = hitbox.clone();
            let rendered_text = rendered_text.clone();
            move |markdown, event: &MouseDownEvent, phase, window, _cx| {
                if phase.capture()
                    && event.button == MouseButton::Right
                    && hitbox.is_hovered(window)
                {
                    let link = rendered_text
                        .source_index_for_position(event.position)
                        .ok()
                        .and_then(|ix| rendered_text.link_for_source_index(ix))
                        .map(|link| link.destination_url.clone());
                    markdown.capture_for_context_menu(link, Some(&rendered_text));
                }
            }
        });

        self.on_mouse_event(window, cx, {
            let rendered_text = rendered_text.clone();
            let hitbox = hitbox.clone();
            move |markdown, event: &MouseDownEvent, phase, window, cx| {
                if hitbox.is_hovered(window) {
                    if phase.bubble() && event.button != MouseButton::Right {
                        let position_result =
                            rendered_text.source_index_for_position(event.position);

                        if let Ok(source_index) = position_result {
                            if let Some(footnote_ref) =
                                rendered_text.footnote_ref_for_source_index(source_index)
                            {
                                markdown.pressed_footnote_ref = Some(footnote_ref.clone());
                            } else if let Some(link) =
                                rendered_text.link_for_source_index(source_index)
                            {
                                markdown.pressed_link = Some(link.clone());
                            }
                        }

                        if markdown.pressed_footnote_ref.is_none()
                            && markdown.pressed_link.is_none()
                        {
                            let source_index = match position_result {
                                Ok(ix) | Err(ix) => ix,
                            };
                            if let Some(handler) = on_source_click.as_ref() {
                                let blocked = handler(source_index, event.click_count, window, cx);
                                if blocked {
                                    markdown.selection = Selection::default();
                                    markdown.pressed_link = None;
                                    window.prevent_default();
                                    cx.notify();
                                    return;
                                }
                            }
                            let (range, mode, reversed) = match event.click_count {
                                1 if event.modifiers.shift => {
                                    let tail = markdown.selection.tail();
                                    let reversed = source_index < tail;
                                    let range = if reversed {
                                        source_index..tail
                                    } else {
                                        tail..source_index
                                    };
                                    (range, SelectMode::Character, reversed)
                                }
                                1 => {
                                    let range = source_index..source_index;
                                    (range, SelectMode::Character, false)
                                }
                                2 => {
                                    let range = rendered_text.surrounding_word_range(source_index);
                                    (range.clone(), SelectMode::Word(range), false)
                                }
                                3 => {
                                    let range = rendered_text.surrounding_line_range(source_index);
                                    (range.clone(), SelectMode::Line(range), false)
                                }
                                _ => {
                                    let range = 0..rendered_text
                                        .lines
                                        .last()
                                        .map(|line| line.source_end)
                                        .unwrap_or(0);
                                    (range, SelectMode::All, false)
                                }
                            };
                            markdown.selection = Selection {
                                start: range.start,
                                end: range.end,
                                reversed,
                                pending: true,
                                mode,
                            };
                            window.focus(&markdown.focus_handle, cx);
                        }

                        window.prevent_default();
                        cx.notify();
                    }
                } else if phase.capture() && event.button == MouseButton::Left {
                    markdown.selection = Selection::default();
                    markdown.pressed_link = None;
                    cx.notify();
                }
            }
        });
        self.on_mouse_event(window, cx, {
            let rendered_text = rendered_text.clone();
            let hitbox = hitbox.clone();
            let was_hovering_clickable = is_hovering_clickable;
            move |markdown, event: &MouseMoveEvent, phase, window, cx| {
                if phase.capture() {
                    return;
                }

                if markdown.selection.pending {
                    let source_index = match rendered_text.source_index_for_position(event.position)
                    {
                        Ok(ix) | Err(ix) => ix,
                    };
                    markdown.selection.set_head(source_index, &rendered_text);
                    markdown.autoscroll_code_block(source_index, event.position);
                    markdown.autoscroll_request = Some(source_index);
                    cx.notify();
                } else {
                    let is_hovering_clickable = hitbox.is_hovered(window)
                        && rendered_text
                            .source_index_for_position(event.position)
                            .ok()
                            .is_some_and(|source_index| {
                                rendered_text.link_for_source_index(source_index).is_some()
                                    || rendered_text
                                        .footnote_ref_for_source_index(source_index)
                                        .is_some()
                            });
                    if is_hovering_clickable != was_hovering_clickable {
                        cx.notify();
                    }
                }
            }
        });
        self.on_mouse_event(window, cx, {
            let rendered_text = rendered_text.clone();
            move |markdown, event: &MouseUpEvent, phase, window, cx| {
                if phase.bubble() {
                    let source_index = rendered_text.source_index_for_position(event.position).ok();
                    if let Some(pressed_footnote_ref) = markdown.pressed_footnote_ref.take()
                        && source_index
                            .and_then(|ix| rendered_text.footnote_ref_for_source_index(ix))
                            == Some(&pressed_footnote_ref)
                    {
                        if let Some(source_index) =
                            markdown.footnote_definition_content_start(&pressed_footnote_ref.label)
                        {
                            markdown.autoscroll_request = Some(source_index);
                            cx.notify();
                        }
                    } else if let Some(pressed_link) = markdown.pressed_link.take()
                        && source_index.and_then(|ix| rendered_text.link_for_source_index(ix))
                            == Some(&pressed_link)
                    {
                        if let Some(open_url) = on_open_url.as_ref() {
                            open_url(pressed_link.destination_url, window, cx);
                        } else {
                            cx.open_url(&pressed_link.destination_url);
                        }
                    }
                } else if markdown.selection.pending {
                    markdown.selection.pending = false;
                    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                    {
                        let text = rendered_text
                            .text_for_range(markdown.selection.start..markdown.selection.end);
                        cx.write_to_primary(ClipboardItem::new_string(text))
                    }
                    cx.notify();
                }
            }
        });
    }

    pub(super) fn autoscroll(
        &self,
        rendered_text: &RenderedText,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<()> {
        let autoscroll_index = self
            .markdown
            .update(cx, |markdown, _| markdown.autoscroll_request.take())?;
        let (position, line_height) = rendered_text.position_for_source_index(autoscroll_index)?;

        match &self.autoscroll {
            AutoscrollBehavior::Controlled(scroll_handle) => {
                let viewport = scroll_handle.bounds();
                let margin = line_height * 3.;
                let top_goal = viewport.top() + margin;
                let bottom_goal = viewport.bottom() - margin;
                let current_offset = scroll_handle.offset();

                let new_offset_y = if position.y < top_goal {
                    current_offset.y + (top_goal - position.y)
                } else if position.y + line_height > bottom_goal {
                    current_offset.y + (bottom_goal - (position.y + line_height))
                } else {
                    current_offset.y
                };

                scroll_handle.set_offset(point(
                    current_offset.x,
                    new_offset_y.clamp(-scroll_handle.max_offset().y, Pixels::ZERO),
                ));
            }
            AutoscrollBehavior::Propagate => {
                let text_style = self.style.base_text_style.clone();
                let font_id = window.text_system().resolve_font(&text_style.font());
                let font_size = text_style.font_size.to_pixels(window.rem_size());
                let em_width = window.text_system().em_width(font_id, font_size).unwrap();
                window.request_autoscroll(Bounds::from_corners(
                    point(position.x - 3. * em_width, position.y - 3. * line_height),
                    point(position.x + 3. * em_width, position.y + 3. * line_height),
                ));
            }
        }
        Some(())
    }

    pub(super) fn on_mouse_event<T: MouseEvent>(
        &self,
        window: &mut Window,
        _cx: &mut App,
        mut f: impl 'static
        + FnMut(&mut Markdown, &T, DispatchPhase, &mut Window, &mut Context<Markdown>),
    ) {
        window.on_mouse_event({
            let markdown = self.markdown.downgrade();
            move |event, phase, window, cx| {
                markdown
                    .update(cx, |markdown, cx| f(markdown, event, phase, window, cx))
                    .log_err();
            }
        });
    }
}
