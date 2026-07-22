use super::*;

impl Render for Console {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query_focus_handle = self.query_bar.focus_handle(cx);
        self.update_output(window, cx);

        v_flex()
            .track_focus(&self.focus_handle)
            .key_context("DebugConsole")
            .on_action(cx.listener(Self::evaluate))
            .on_action(cx.listener(Self::watch_expression))
            .size_full()
            .border_2()
            .bg(cx.theme().colors().editor_background)
            .child(self.render_console(cx))
            .when(self.is_running(cx), |this| {
                this.child(Divider::horizontal()).child(
                    h_flex()
                        .on_action(cx.listener(Self::previous_query))
                        .on_action(cx.listener(Self::next_query))
                        .p_1()
                        .gap_1()
                        .bg(cx.theme().colors().editor_background)
                        .child(self.render_query_bar(cx))
                        .child(SplitButton::new(
                            ui::ButtonLike::new_rounded_all(ElementId::Name(
                                "split-button-left-confirm-button".into(),
                            ))
                            .on_click(move |_, window, cx| {
                                window.dispatch_action(Box::new(Confirm), cx)
                            })
                            .layer(ui::ElevationIndex::ModalSurface)
                            .size(ui::ButtonSize::Compact)
                            .child(Label::new("Evaluate"))
                            .tooltip({
                                let query_focus_handle = query_focus_handle.clone();

                                move |_window, cx| {
                                    Tooltip::for_action_in(
                                        "Evaluate",
                                        &Confirm,
                                        &query_focus_handle,
                                        cx,
                                    )
                                }
                            }),
                            self.render_submit_menu(
                                ElementId::Name("split-button-right-confirm-button".into()),
                                Some(query_focus_handle.clone()),
                                cx,
                            )
                            .into_any_element(),
                        )),
                )
            })
    }
}
