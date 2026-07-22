use super::*;

impl RemoteServerProjects {
    fn render_create_dev_container(
        &self,
        state: &CreateRemoteDevContainer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        match &state.progress {
            DevContainerCreationProgress::Error(message) => {
                let view = Navigable::new(
                    div()
                        .child(
                            div().track_focus(&self.focus_handle(cx)).size_full().child(
                                v_flex().py_1().child(
                                    ListItem::new("Error")
                                        .inset(true)
                                        .selectable(false)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .start_slot(
                                            Icon::new(IconName::XCircle).color(Color::Error),
                                        )
                                        .child(Label::new("Error Creating Dev Container:"))
                                        .child(Label::new(message).buffer_font(cx)),
                                ),
                            ),
                        )
                        .child(ListSeparator)
                        .child(
                            div()
                                .id("devcontainer-see-log")
                                .track_focus(&state.view_logs_entry.focus_handle)
                                .on_action(cx.listener(|_, _: &menu::Confirm, window, cx| {
                                    window.dispatch_action(Box::new(OpenLog), cx);
                                    cx.emit(DismissEvent);
                                    cx.notify();
                                }))
                                .child(
                                    ListItem::new("li-devcontainer-see-log")
                                        .toggle_state(
                                            state
                                                .view_logs_entry
                                                .focus_handle
                                                .contains_focused(window, cx),
                                        )
                                        .inset(true)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .start_slot(
                                            Icon::new(IconName::File)
                                                .color(Color::Muted)
                                                .size(IconSize::Small),
                                        )
                                        .child(Label::new("Open Mav Log"))
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            window.dispatch_action(Box::new(OpenLog), cx);
                                            cx.emit(DismissEvent);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .id("devcontainer-go-back")
                                .track_focus(&state.back_entry.focus_handle)
                                .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                    this.cancel(&menu::Cancel, window, cx);
                                    cx.notify();
                                }))
                                .child(
                                    ListItem::new("li-devcontainer-go-back")
                                        .toggle_state(
                                            state
                                                .back_entry
                                                .focus_handle
                                                .contains_focused(window, cx),
                                        )
                                        .inset(true)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .start_slot(
                                            Icon::new(IconName::Exit)
                                                .color(Color::Muted)
                                                .size(IconSize::Small),
                                        )
                                        .child(Label::new("Exit"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.cancel(&menu::Cancel, window, cx);
                                            cx.notify();
                                        })),
                                ),
                        )
                        .into_any_element(),
                )
                .entry(state.view_logs_entry.clone())
                .entry(state.back_entry.clone());
                view.render(window, cx).into_any_element()
            }
            DevContainerCreationProgress::SelectingConfig => {
                self.render_config_selection(window, cx).into_any_element()
            }
            DevContainerCreationProgress::Creating => {
                self.focus_handle(cx).focus(window, cx);
                div()
                    .track_focus(&self.focus_handle(cx))
                    .size_full()
                    .child(
                        v_flex()
                            .pb_1()
                            .child(
                                ModalHeader::new().child(
                                    Headline::new("Dev Containers").size(HeadlineSize::XSmall),
                                ),
                            )
                            .child(ListSeparator)
                            .child(
                                ListItem::new("creating")
                                    .inset(true)
                                    .spacing(ui::ListItemSpacing::Sparse)
                                    .disabled(true)
                                    .start_slot(
                                        Icon::new(IconName::ArrowCircle)
                                            .color(Color::Muted)
                                            .with_rotate_animation(2),
                                    )
                                    .child(
                                        h_flex()
                                            .opacity(0.6)
                                            .gap_1()
                                            .child(Label::new("Creating Dev Container"))
                                            .child(LoadingLabel::new("")),
                                    ),
                            ),
                    )
                    .into_any_element()
            }
        }
    }

    fn render_config_selection(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(picker) = &self.dev_container_picker else {
            return div().into_any_element();
        };

        let content = v_flex().pb_1().child(picker.clone().into_any_element());

        picker.focus_handle(cx).focus(window, cx);

        content.into_any_element()
    }
}
