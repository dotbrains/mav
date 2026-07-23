use super::*;

impl Editor {
    pub(crate) fn render_edit_prediction_popover(
        &mut self,
        text_bounds: &Bounds<Pixels>,
        content_origin: gpui::Point<Pixels>,
        right_margin: Pixels,
        editor_snapshot: &EditorSnapshot,
        visible_row_range: Range<DisplayRow>,
        scroll_top: ScrollOffset,
        scroll_bottom: ScrollOffset,
        line_layouts: &[LineWithInvisibles],
        line_height: Pixels,
        scroll_position: gpui::Point<ScrollOffset>,
        scroll_pixel_position: gpui::Point<ScrollPixelOffset>,
        newest_selection_head: Option<DisplayPoint>,
        editor_width: Pixels,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(AnyElement, gpui::Point<Pixels>)> {
        if self.mode().is_minimap() {
            return None;
        }
        let active_edit_prediction = self.active_edit_prediction.as_ref()?;

        if self.edit_prediction_visible_in_cursor_popover(true) {
            return None;
        }

        match &active_edit_prediction.completion {
            EditPrediction::MoveWithin { target, .. } => {
                let target_display_point = target.to_display_point(editor_snapshot);

                if self.edit_prediction_requires_modifier() {
                    if !self.edit_prediction_preview_is_active() {
                        return None;
                    }

                    self.render_edit_prediction_modifier_jump_popover(
                        text_bounds,
                        content_origin,
                        visible_row_range,
                        line_layouts,
                        line_height,
                        scroll_pixel_position,
                        newest_selection_head,
                        target_display_point,
                        window,
                        cx,
                    )
                } else {
                    self.render_edit_prediction_eager_jump_popover(
                        text_bounds,
                        content_origin,
                        editor_snapshot,
                        visible_row_range,
                        scroll_top,
                        scroll_bottom,
                        line_height,
                        scroll_pixel_position,
                        target_display_point,
                        editor_width,
                        window,
                        cx,
                    )
                }
            }
            EditPrediction::Edit {
                display_mode: EditDisplayMode::Inline,
                ..
            } => None,
            EditPrediction::Edit {
                display_mode: EditDisplayMode::TabAccept,
                edits,
                ..
            } => {
                let range = &edits.first()?.0;
                let target_display_point = range.end.to_display_point(editor_snapshot);

                self.render_edit_prediction_end_of_line_popover(
                    "Accept",
                    editor_snapshot,
                    visible_row_range,
                    target_display_point,
                    line_height,
                    scroll_pixel_position,
                    content_origin,
                    editor_width,
                    window,
                    cx,
                )
            }
            EditPrediction::Edit {
                edits,
                edit_preview,
                display_mode: EditDisplayMode::DiffPopover,
                snapshot,
                ..
            } => self.render_edit_prediction_diff_popover(
                text_bounds,
                content_origin,
                right_margin,
                editor_snapshot,
                visible_row_range,
                line_layouts,
                line_height,
                scroll_position,
                scroll_pixel_position,
                newest_selection_head,
                editor_width,
                style,
                edits,
                edit_preview,
                snapshot,
                window,
                cx,
            ),
            EditPrediction::MoveOutside { snapshot, .. } => {
                let mut element = self
                    .render_edit_prediction_jump_outside_popover(snapshot, window, cx)
                    .into_any();

                let size = element.layout_as_root(AvailableSpace::min_size(), window, cx);
                let origin_x = text_bounds.size.width - size.width - px(30.);
                let origin = text_bounds.origin + gpui::Point::new(origin_x, px(16.));
                element.prepaint_at(origin, window, cx);

                Some((element, origin))
            }
        }
    }

    pub(crate) fn edit_prediction_cursor_popover_height(&self) -> Pixels {
        px(30.)
    }

    pub(crate) fn render_edit_prediction_cursor_popover(
        &self,
        min_width: Pixels,
        max_width: Pixels,
        cursor_point: Point,
        style: &EditorStyle,
        window: &mut Window,
        cx: &mut Context<Editor>,
    ) -> Option<AnyElement> {
        let provider = self.edit_prediction_provider.as_ref()?;
        let icons = Self::get_prediction_provider_icons(&self.edit_prediction_provider, cx);

        let is_refreshing = provider.provider.is_refreshing(cx);

        pub(crate) fn pending_completion_container(icon: IconName) -> Div {
            h_flex().h_full().flex_1().gap_2().child(Icon::new(icon))
        }

        let completion = match &self.active_edit_prediction {
            Some(prediction) => {
                if !self.has_visible_completions_menu() {
                    pub(crate) const RADIUS: Pixels = px(6.);
                    pub(crate) const BORDER_WIDTH: Pixels = px(1.);
                    let keybind_display = self.edit_prediction_keybind_display(
                        EditPredictionKeybindSurface::CursorPopoverCompact,
                        window,
                        cx,
                    );

                    return Some(
                        h_flex()
                            .elevation_2(cx)
                            .border(BORDER_WIDTH)
                            .border_color(cx.theme().colors().border)
                            .when(keybind_display.missing_accept_keystroke, |el| {
                                el.border_color(cx.theme().status().error)
                            })
                            .rounded(RADIUS)
                            .rounded_tl(px(0.))
                            .overflow_hidden()
                            .child(div().px_1p5().child(match &prediction.completion {
                                EditPrediction::MoveWithin { target, snapshot } => {
                                    use text::ToPoint as _;
                                    if target.text_anchor_in(&snapshot).to_point(snapshot).row
                                        > cursor_point.row
                                    {
                                        Icon::new(icons.down)
                                    } else {
                                        Icon::new(icons.up)
                                    }
                                }
                                EditPrediction::MoveOutside { .. } => {
                                    // TODO [zeta2] custom icon for external jump?
                                    Icon::new(icons.base)
                                }
                                EditPrediction::Edit { .. } => Icon::new(icons.base),
                            }))
                            .child(
                                h_flex()
                                    .gap_1()
                                    .py_1()
                                    .px_2()
                                    .rounded_r(RADIUS - BORDER_WIDTH)
                                    .border_l_1()
                                    .border_color(cx.theme().colors().border)
                                    .bg(Self::edit_prediction_line_popover_bg_color(cx))
                                    .when(keybind_display.show_hold_label, |el| {
                                        el.child(
                                            Label::new("Hold")
                                                .size(LabelSize::Small)
                                                .when(
                                                    keybind_display.missing_accept_keystroke,
                                                    |el| el.strikethrough(),
                                                )
                                                .line_height_style(LineHeightStyle::UiLabel),
                                        )
                                    })
                                    .id("edit_prediction_cursor_popover_keybind")
                                    .when(keybind_display.missing_accept_keystroke, |el| {
                                        let status_colors = cx.theme().status();

                                        el.bg(status_colors.error_background)
                                            .border_color(status_colors.error.opacity(0.6))
                                            .child(Icon::new(IconName::Info).color(Color::Error))
                                            .cursor_default()
                                            .hoverable_tooltip(move |_window, cx| {
                                                cx.new(|_| MissingEditPredictionKeybindingTooltip)
                                                    .into()
                                            })
                                    })
                                    .when_some(
                                        keybind_display.displayed_keystroke.as_ref(),
                                        |el, compact_keystroke| {
                                            el.child(self.render_edit_prediction_popover_keystroke(
                                                compact_keystroke,
                                                Color::Default,
                                                cx,
                                            ))
                                        },
                                    ),
                            )
                            .into_any(),
                    );
                }

                self.render_edit_prediction_cursor_popover_preview(
                    prediction,
                    cursor_point,
                    style,
                    cx,
                )?
            }

            None if is_refreshing => match &self.stale_edit_prediction_in_menu {
                Some(stale_completion) => self.render_edit_prediction_cursor_popover_preview(
                    stale_completion,
                    cursor_point,
                    style,
                    cx,
                )?,

                None => pending_completion_container(icons.base)
                    .child(Label::new("...").size(LabelSize::Small)),
            },

            None => pending_completion_container(icons.base)
                .child(Label::new("...").size(LabelSize::Small)),
        };

        let completion = if is_refreshing || self.active_edit_prediction.is_none() {
            completion
                .with_animation(
                    "loading-completion",
                    Animation::new(Duration::from_secs(2))
                        .repeat()
                        .with_easing(pulsating_between(0.4, 0.8)),
                    |label, delta| label.opacity(delta),
                )
                .into_any_element()
        } else {
            completion.into_any_element()
        };

        let has_completion = self.active_edit_prediction.is_some();
        let keybind_display = self.edit_prediction_keybind_display(
            EditPredictionKeybindSurface::CursorPopoverExpanded,
            window,
            cx,
        );

        Some(
            h_flex()
                .min_w(min_width)
                .max_w(max_width)
                .flex_1()
                .elevation_2(cx)
                .border_color(cx.theme().colors().border)
                .child(
                    div()
                        .flex_1()
                        .py_1()
                        .px_2()
                        .overflow_hidden()
                        .child(completion),
                )
                .when_some(
                    keybind_display.displayed_keystroke.as_ref(),
                    |el, keystroke| {
                        let key_color = if !has_completion {
                            Color::Muted
                        } else {
                            Color::Default
                        };

                        if keybind_display.action == EditPredictionKeybindAction::Preview {
                            el.child(
                                h_flex()
                                    .h_full()
                                    .border_l_1()
                                    .rounded_r_lg()
                                    .border_color(cx.theme().colors().border)
                                    .bg(Self::edit_prediction_line_popover_bg_color(cx))
                                    .gap_1()
                                    .py_1()
                                    .px_2()
                                    .child(self.render_edit_prediction_popover_keystroke(
                                        keystroke, key_color, cx,
                                    ))
                                    .child(Label::new("Preview").into_any_element())
                                    .opacity(if has_completion { 1.0 } else { 0.4 }),
                            )
                        } else {
                            el.child(
                                h_flex()
                                    .h_full()
                                    .border_l_1()
                                    .rounded_r_lg()
                                    .border_color(cx.theme().colors().border)
                                    .bg(Self::edit_prediction_line_popover_bg_color(cx))
                                    .gap_1()
                                    .py_1()
                                    .px_2()
                                    .child(self.render_edit_prediction_popover_keystroke(
                                        keystroke, key_color, cx,
                                    ))
                                    .opacity(if has_completion { 1.0 } else { 0.4 }),
                            )
                        }
                    },
                )
                .into_any(),
        )
    }
}
