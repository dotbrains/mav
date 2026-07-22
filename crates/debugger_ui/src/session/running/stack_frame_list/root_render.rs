use super::*;

impl Render for StackFrameList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .track_focus(&self.focus_handle)
            .size_full()
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::confirm))
            .when_some(self.error.clone(), |el, error| {
                el.child(
                    h_flex()
                        .bg(cx.theme().status().warning_background)
                        .border_b_1()
                        .border_color(cx.theme().status().warning_border)
                        .pl_1()
                        .child(Icon::new(IconName::Warning).color(Color::Warning))
                        .gap_2()
                        .child(
                            Label::new(error)
                                .size(LabelSize::Small)
                                .color(Color::Warning),
                        ),
                )
            })
            .child(self.render_list(window, cx))
            .vertical_scrollbar_for(&self.list_state, window, cx)
    }
}
