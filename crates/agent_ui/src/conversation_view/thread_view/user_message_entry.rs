use super::*;

impl ThreadView {
    pub(super) fn render_user_message_entry(
        &self,
        entry_ix: usize,
        message: &acp_thread::UserMessage,
        is_indented: bool,
        is_first_indented: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(editor) = self
            .entry_view_state
            .read(cx)
            .entry(entry_ix)
            .and_then(|entry| entry.message_editor())
            .cloned()
        else {
            return Empty.into_any_element();
        };

        let editing = self.editing_message == Some(entry_ix);
        let editor_focus = editor.focus_handle(cx).is_focused(window);
        let focus_border = cx.theme().colors().border_focused;
        let opaque_window =
            cx.theme().window_background_appearance() == gpui::WindowBackgroundAppearance::Opaque;
        let has_checkpoint_button = message
            .checkpoint
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.show);
        let is_subagent = self.is_subagent();
        let can_rewind = self.thread.read(cx).supports_truncate(cx);
        let is_editable = can_rewind && message.client_id.is_some() && !is_subagent;
        let agent_name: SharedString = if is_subagent {
            "subagents".into()
        } else {
            self.agent_id.to_string().into()
        };

        v_flex()
            .id(("user_message", entry_ix))
            .map(|this| {
                if is_first_indented {
                    this.pt_0p5()
                } else {
                    this.pt_2()
                }
            })
            .pb_3()
            .px_2()
            .gap_1p5()
            .w_full()
            .when(is_editable && has_checkpoint_button, |this| {
                this.children(message.client_id.clone().map(|client_id| {
                    h_flex()
                        .px_3()
                        .gap_2()
                        .child(Divider::horizontal())
                        .child(
                            Button::new("restore-checkpoint", "Restore Checkpoint")
                                .start_icon(Icon::new(IconName::Undo).size(IconSize::XSmall).color(Color::Muted))
                                .label_size(LabelSize::XSmall)
                                .color(Color::Muted)
                                .tooltip(Tooltip::text("Restores all files in the project to the content they had at this point in the conversation."))
                                .on_click(cx.listener(move |this, _, _window, cx| {
                                    this.restore_checkpoint(&client_id, cx);
                                }))
                        )
                        .child(Divider::horizontal())
                }))
            })
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .py_3()
                            .px_2()
                            .rounded_md()
                            .bg(cx.theme().colors().editor_background)
                            .border_1()
                            .when(is_indented, |this| {
                                this.py_2()
                                    .px_2()
                                    .when(opaque_window, |this| this.shadow_sm())
                            })
                            .border_color(cx.theme().colors().border)
                            .map(|this| {
                                if !is_editable {
                                    if is_subagent {
                                        return this.border_dashed();
                                    }
                                    return this;
                                }
                                if editing && editor_focus {
                                    return this.border_color(focus_border);
                                }
                                if editing && !editor_focus {
                                    return this.border_dashed();
                                }
                                this.when(opaque_window, |this| this.shadow_md())
                                    .hover(|s| s.border_color(focus_border.opacity(0.8)))
                            })
                            .text_xs()
                            .child(editor.clone().into_any_element()),
                    )
                    .when(editor_focus, |this| {
                        self.render_user_message_entry_controls(
                            this,
                            entry_ix,
                            editor.clone(),
                            is_editable,
                            agent_name.clone(),
                            cx,
                        )
                    }),
            )
            .into_any()
    }

    fn render_user_message_entry_controls(
        &self,
        container: Div,
        entry_ix: usize,
        editor: Entity<MessageEditor>,
        is_editable: bool,
        agent_name: SharedString,
        cx: &Context<Self>,
    ) -> Div {
        let base_container = h_flex()
            .absolute()
            .top_neg_3p5()
            .right_3()
            .gap_1()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().editor_background)
            .overflow_hidden();

        let is_loading_contents = self.is_loading_contents;
        if is_editable {
            container.child(
                base_container
                    .child(
                        IconButton::new("cancel", IconName::Close)
                            .disabled(is_loading_contents)
                            .icon_color(Color::Error)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(Self::cancel_editing)),
                    )
                    .child(if is_loading_contents {
                        div()
                            .id("loading-edited-message-content")
                            .tooltip(Tooltip::text("Loading Added Context…"))
                            .child(loading_contents_spinner(IconSize::XSmall))
                            .into_any_element()
                    } else {
                        IconButton::new("regenerate", IconName::Return)
                            .icon_color(Color::Muted)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text(
                                "Editing will restart the thread from this point.",
                            ))
                            .on_click(cx.listener({
                                let editor = editor.clone();
                                move |this, _, window, cx| {
                                    this.regenerate(entry_ix, editor.clone(), window, cx);
                                }
                            }))
                            .into_any_element()
                    }),
            )
        } else {
            container.child(
                base_container.border_dashed().child(
                    IconButton::new("non_editable", IconName::PencilUnavailable)
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Muted)
                        .style(ButtonStyle::Transparent)
                        .tooltip(Tooltip::element({
                            move |_, _| {
                                v_flex()
                                    .gap_1()
                                    .child(Label::new("Unavailable Editing"))
                                    .child(div().max_w_64().child(
                                        Label::new(format!(
                                            "Editing previous messages is not available for {} yet.",
                                            agent_name
                                        ))
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                    ))
                                    .into_any_element()
                            }
                        })),
                ),
            )
        }
    }
}
