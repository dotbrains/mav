use super::*;

impl ThreadView {
    pub(super) fn render_feedback_feedback_editor(
        editor: Entity<Editor>,
        cx: &Context<Self>,
    ) -> Div {
        h_flex()
            .key_context("AgentFeedbackMessageEditor")
            .on_action(cx.listener(move |this, _: &menu::Cancel, _, cx| {
                this.thread_feedback.dismiss_comments();
                cx.notify();
            }))
            .on_action(cx.listener(move |this, _: &menu::Confirm, _window, cx| {
                this.submit_feedback_message(cx);
            }))
            .p_2()
            .mb_2()
            .mx_5()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().editor_background)
            .child(div().w_full().child(editor))
            .child(
                h_flex()
                    .child(
                        IconButton::new("dismiss-feedback-message", IconName::Close)
                            .icon_color(Color::Error)
                            .icon_size(IconSize::XSmall)
                            .shape(ui::IconButtonShape::Square)
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.thread_feedback.dismiss_comments();
                                cx.notify();
                            })),
                    )
                    .child(
                        IconButton::new("submit-feedback-message", IconName::Return)
                            .icon_size(IconSize::XSmall)
                            .shape(ui::IconButtonShape::Square)
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.submit_feedback_message(cx);
                            })),
                    ),
            )
    }
}
