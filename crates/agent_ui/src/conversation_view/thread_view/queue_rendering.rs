use super::*;

impl ThreadView {
    pub(super) fn render_message_queue_summary(
        &self,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let queue_count = self.message_queue.len();
        let title: SharedString = if queue_count == 1 {
            "1 Queued Message".into()
        } else {
            format!("{} Queued Messages", queue_count).into()
        };

        h_flex()
            .p_1()
            .w_full()
            .gap_1()
            .justify_between()
            .when(self.queue_expanded, |this| {
                this.border_b_1().border_color(cx.theme().colors().border)
            })
            .child(
                h_flex()
                    .id("queue_summary")
                    .gap_1()
                    .child(Disclosure::new("queue_disclosure", self.queue_expanded))
                    .child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.queue_expanded = !this.queue_expanded;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("clear_queue", "Clear All")
                    .label_size(LabelSize::Small)
                    .key_binding(
                        KeyBinding::for_action(&ClearMessageQueue, cx)
                            .map(|kb| kb.size(rems_from_px(12.))),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_queue(cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn clear_queue(&mut self, cx: &mut Context<Self>) {
        self.message_queue.clear();
        self.sync_queue_flag_to_native_thread(cx);
        cx.notify();
    }

    fn render_queue_steer_button(
        &self,
        entry_id: QueueEntryId,
        index: usize,
        is_next: bool,
        steer_on: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let focus_handle = self.message_editor.focus_handle(cx);

        Button::new(("steer", index), "Steer")
            .label_size(LabelSize::Small)
            .toggle_state(steer_on)
            .selected_style(ButtonStyle::Tinted(TintColor::Accent))
            .when(is_next, |this| {
                this.key_binding(
                    KeyBinding::for_action_in(&ToggleSteerFirstQueuedMessage, &focus_handle, cx)
                        .map(|kb| kb.size(rems_from_px(12.))),
                )
            })
            .tooltip(move |_window, cx| {
                Tooltip::with_meta(
                    "Steer",
                    None,
                    "Interrupt the agent at its next step to send this message. \
                     When off, queued messages wait for the agent to finish.",
                    cx,
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_queue_entry_steer(entry_id, cx);
            }))
    }

    pub(super) fn render_message_queue_entries(
        &self,
        _window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let message_editor = self.message_editor.read(cx);
        let focus_handle = message_editor.focus_handle(cx);

        let queue_len = self.message_queue.len();
        let can_fast_track = self.message_queue.can_fast_track();
        let is_native = self.as_native_thread(cx).is_some();

        v_flex()
            .id("message_queue_list")
            .max_h_40()
            .overflow_y_scroll()
            .children(self.message_queue.iter().enumerate().map(|(index, entry)| {
                let entry_id = entry.id;
                let editor = &entry.editor;
                let is_next = index == 0;
                let (icon_color, tooltip_text) = if is_next {
                    (Color::Accent, "Next in Queue")
                } else {
                    (Color::Muted, "In Queue")
                };

                let editor_focused = editor.focus_handle(cx).is_focused(_window);
                let keybinding_size = rems_from_px(12.);
                let steer_on = entry.steer;

                let min_width = rems_from_px(160.);

                h_flex()
                    .group("queue_entry")
                    .w_full()
                    .p_1p5()
                    .gap_1()
                    .bg(cx.theme().colors().editor_background)
                    .when(index < queue_len - 1, |this| {
                        this.border_b_1()
                            .border_color(cx.theme().colors().border_variant)
                    })
                    .child(
                        div()
                            .id("next_in_queue")
                            .child(
                                Icon::new(IconName::Circle)
                                    .size(IconSize::Small)
                                    .color(icon_color),
                            )
                            .tooltip(Tooltip::text(tooltip_text)),
                    )
                    .child(editor.clone())
                    .child(if editor_focused {
                        h_flex()
                            .gap_1()
                            .min_w(min_width)
                            .justify_end()
                            .child(
                                IconButton::new(("edit", index), IconName::Pencil)
                                    .icon_size(IconSize::Small)
                                    .tooltip(|_window, cx| {
                                        Tooltip::with_meta(
                                            "Edit Queued Message",
                                            None,
                                            "Type anything to edit",
                                            cx,
                                        )
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_queued_message_to_main_editor(
                                            entry_id, None, None, window, cx,
                                        );
                                    })),
                            )
                            .when(is_native, |row| {
                                row.child(self.render_queue_steer_button(
                                    entry_id, index, is_next, steer_on, cx,
                                ))
                            })
                            .child(
                                Button::new(("send_now_focused", index), "Send Now")
                                    .label_size(LabelSize::Small)
                                    .style(ButtonStyle::Outlined)
                                    .key_binding(
                                        KeyBinding::for_action_in(
                                            &SendImmediately,
                                            &editor.focus_handle(cx),
                                            cx,
                                        )
                                        .map(|kb| kb.size(keybinding_size)),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.send_queued_message_now(entry_id, window, cx);
                                    })),
                            )
                    } else {
                        h_flex()
                            .when(!is_next, |this| this.visible_on_hover("queue_entry"))
                            .gap_1()
                            .min_w(min_width)
                            .justify_end()
                            .child(
                                IconButton::new(("delete", index), IconName::Trash)
                                    .icon_size(IconSize::Small)
                                    .tooltip({
                                        let focus_handle = focus_handle.clone();
                                        move |_window, cx| {
                                            if is_next {
                                                Tooltip::for_action_in(
                                                    "Remove Message from Queue",
                                                    &RemoveFirstQueuedMessage,
                                                    &focus_handle,
                                                    cx,
                                                )
                                            } else {
                                                Tooltip::simple("Remove Message from Queue", cx)
                                            }
                                        }
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_from_queue(entry_id, cx);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                IconButton::new(("edit", index), IconName::Pencil)
                                    .icon_size(IconSize::Small)
                                    .tooltip({
                                        let focus_handle = focus_handle.clone();
                                        move |_window, cx| {
                                            if is_next {
                                                Tooltip::for_action_in(
                                                    "Edit",
                                                    &EditFirstQueuedMessage,
                                                    &focus_handle,
                                                    cx,
                                                )
                                            } else {
                                                Tooltip::simple("Edit", cx)
                                            }
                                        }
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.move_queued_message_to_main_editor(
                                            entry_id, None, None, window, cx,
                                        );
                                    })),
                            )
                            .when(is_native, |row| {
                                row.child(self.render_queue_steer_button(
                                    entry_id, index, is_next, steer_on, cx,
                                ))
                            })
                            .child(
                                Button::new(("send_now", index), "Send Now")
                                    .label_size(LabelSize::Small)
                                    .when(is_next, |this| this.style(ButtonStyle::Outlined))
                                    .when(is_next && message_editor.is_empty(cx), |this| {
                                        let action: Box<dyn gpui::Action> = if can_fast_track {
                                            Box::new(Chat)
                                        } else {
                                            Box::new(SendNextQueuedMessage)
                                        };

                                        this.key_binding(
                                            KeyBinding::for_action_in(
                                                action.as_ref(),
                                                &focus_handle.clone(),
                                                cx,
                                            )
                                            .map(|kb| kb.size(keybinding_size)),
                                        )
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.send_queued_message_now(entry_id, window, cx);
                                    })),
                            )
                    })
            }))
            .into_any_element()
    }
}
