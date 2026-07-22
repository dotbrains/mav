use super::*;

impl Render for MessageNotification {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self
            .auto_hide
            .as_mut()
            .is_some_and(|auto_hide| auto_hide.refresh_animation())
        {
            cx.emit(DismissEvent);
        }

        if self.needs_animation_frame() {
            window.request_animation_frame();
        }

        let opacity = self.opacity();
        let has_auto_hide = self.auto_hide.is_some();
        let entity = window.current_view();
        let show_suppress_button = self.show_suppress_button;
        let show_close_button = self.show_close_button;
        let suppress = show_suppress_button && window.modifiers().shift;
        let (close_id, close_icon) = if suppress {
            ("suppress", IconName::Minimize)
        } else {
            ("close", IconName::Close)
        };

        let main_content = (self.build_content)(window, cx);
        let line_height = window.line_height();

        let copy_text = self.copy_text.clone();
        let header_actions = h_flex()
            .flex_shrink_0()
            .gap_1()
            .when_some(copy_text, |el, text| {
                el.child(
                    CopyButton::new("copy-notification-message", text)
                        .tooltip_label("Copy Message"),
                )
            })
            .when(show_close_button, |el| {
                el.child(
                    IconButton::new(close_id, close_icon)
                        .tooltip(move |_window, cx| {
                            if suppress {
                                Tooltip::with_meta(
                                    "Suppress",
                                    Some(&SuppressNotification),
                                    "Click to Close",
                                    cx,
                                )
                            } else if show_suppress_button {
                                Tooltip::with_meta(
                                    "Close",
                                    Some(&menu::Cancel),
                                    "Shift-click to Suppress",
                                    cx,
                                )
                            } else {
                                Tooltip::for_action("Close", &menu::Cancel, cx)
                            }
                        })
                        .on_click(cx.listener(move |_, _, _, cx| {
                            if suppress {
                                cx.emit(SuppressEvent);
                            } else {
                                cx.emit(DismissEvent);
                            }
                        })),
                )
            });

        let has_suffix = self.primary_message.is_some()
            || self.secondary_message.is_some()
            || self.more_info_message.is_some();

        let suffix = h_flex()
            .gap_1()
            .children(self.primary_message.iter().map(|message| {
                Button::new(("notification-primary", cx.entity_id()), message.clone())
                    .when_some(self.button_style, |button, style| button.style(style))
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(on_click) = this.primary_on_click.as_ref() {
                            (on_click)(window, cx)
                        };
                        this.dismiss(cx)
                    }))
                    .when_some(self.primary_icon, |button, icon| {
                        let element = Icon::new(icon.name)
                            .size(IconSize::Small)
                            .color(self.primary_icon_color.unwrap_or(Color::Muted));
                        match icon.position {
                            IconPosition::Start => button.start_icon(element),
                            IconPosition::End => button.end_icon(element),
                        }
                    })
            }))
            .children(self.secondary_message.iter().map(|message| {
                Button::new(("notification-secondary", cx.entity_id()), message.clone())
                    .when_some(self.button_style, |button, style| button.style(style))
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(on_click) = this.secondary_on_click.as_ref() {
                            (on_click)(window, cx)
                        };
                        this.dismiss(cx)
                    }))
                    .when_some(self.secondary_icon, |button, icon| {
                        let element = Icon::new(icon.name)
                            .size(IconSize::Small)
                            .color(self.secondary_icon_color.unwrap_or(Color::Muted));
                        match icon.position {
                            IconPosition::Start => button.start_icon(element),
                            IconPosition::End => button.end_icon(element),
                        }
                    })
            }))
            .child(
                h_flex().w_full().justify_end().children(
                    self.more_info_message
                        .iter()
                        .zip(self.more_info_url.iter())
                        .map(|(message, url)| {
                            let url = url.clone();
                            Button::new(message.clone(), message.clone())
                                .label_size(LabelSize::Small)
                                .end_icon(
                                    Icon::new(IconName::ArrowUpRight)
                                        .size(IconSize::Indicator)
                                        .color(Color::Muted),
                                )
                                .on_click(cx.listener(move |_, _, _, cx| {
                                    cx.open_url(&url);
                                }))
                        }),
                ),
            );

        // Wrap the icon to vertically align with the first line of the primary
        // message (mirrors `ui::Callout`'s alignment pattern). The body, secondary
        // text and suffix all share a single column to the right of the icon so
        // they line up under one another even when an icon is present.
        let body = h_flex()
            .gap_2()
            .items_start()
            .when_some(
                self.content_icon.zip(self.content_icon_color),
                |el, (icon, color)| {
                    el.child(
                        h_flex()
                            .h(line_height)
                            .justify_center()
                            .child(Icon::new(icon).size(IconSize::Small).color(color)),
                    )
                },
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .child(
                                        div()
                                            .id("message-notification-content")
                                            .max_h(vh(0.6, window))
                                            .overflow_y_scroll()
                                            .track_scroll(&self.scroll_handle.clone())
                                            .child(main_content),
                                    )
                                    .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                            )
                            .when_some(self.secondary_content.clone(), |el, secondary| {
                                el.child(
                                    Label::new(secondary)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                            }),
                    )
                    .when(has_suffix, |this| this.child(suffix)),
            );

        div()
            .id("message-notification-wrapper")
            .opacity(opacity)
            .child(
                v_flex()
                    .id(("notification-frame", entity))
                    .occlude()
                    .when(has_auto_hide, |this| {
                        this.on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                            this.on_hover_changed(*hovered, cx);
                        }))
                    })
                    .when(show_close_button, |this| {
                        this.on_modifiers_changed(move |_, _, cx| cx.notify(entity))
                    })
                    .p_3()
                    .gap_2()
                    .elevation_3(cx)
                    .child(
                        h_flex()
                            .gap_4()
                            .justify_between()
                            .items_start()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .when_some(self.title.clone(), |div, title| {
                                        div.child(Label::new(title))
                                    })
                                    .child(div().max_w_96().child(body)),
                            )
                            .child(header_actions),
                    ),
            )
    }
}
