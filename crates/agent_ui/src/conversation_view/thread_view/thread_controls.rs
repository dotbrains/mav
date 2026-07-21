use super::*;

impl ThreadView {
    pub(super) fn render_thread_controls(
        &self,
        thread: &Entity<AcpThread>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_generating = matches!(thread.read(cx).status(), ThreadStatus::Generating);
        if is_generating {
            return Empty.into_any_element();
        }

        let open_as_markdown = IconButton::new("open-as-markdown", IconName::FileMarkdown)
            .shape(ui::IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .icon_color(Color::Ignored)
            .tooltip(Tooltip::text("Open Thread as Markdown"))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(workspace) = this.workspace.upgrade() {
                    this.open_thread_as_markdown(workspace, window, cx)
                        .detach_and_log_err(cx);
                }
            }));

        let scroll_to_recent_user_prompt =
            IconButton::new("scroll_to_recent_user_prompt", IconName::ForwardArrow)
                .shape(ui::IconButtonShape::Square)
                .icon_size(IconSize::Small)
                .icon_color(Color::Ignored)
                .tooltip(Tooltip::text("Scroll To Most Recent User Prompt"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.scroll_to_most_recent_user_prompt(cx);
                }));

        let scroll_to_top = IconButton::new("scroll_to_top", IconName::ArrowUp)
            .shape(ui::IconButtonShape::Square)
            .icon_size(IconSize::Small)
            .icon_color(Color::Ignored)
            .tooltip(Tooltip::text("Scroll To Top"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.scroll_to_top(cx);
            }));

        let show_stats = AgentSettings::get_global(cx).show_turn_stats;
        let last_turn_clock = show_stats
            .then(|| {
                self.turn_fields
                    .last_turn_duration
                    .filter(|&duration| duration > STOPWATCH_THRESHOLD)
                    .map(|duration| {
                        Label::new(duration_alt_display(duration))
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                    })
            })
            .flatten();

        let last_turn_tokens_label = last_turn_clock
            .is_some()
            .then(|| {
                self.turn_fields
                    .last_turn_tokens
                    .filter(|&tokens| tokens > TOKEN_THRESHOLD)
                    .map(|tokens| {
                        Label::new(format!("{} tokens", crate::humanize_token_count(tokens)))
                            .size(LabelSize::Small)
                            .color(Color::Muted)
                    })
            })
            .flatten();

        let mut container = h_flex()
            .w_full()
            .py_2()
            .px_5()
            .gap_px()
            .opacity(0.6)
            .hover(|s| s.opacity(1.))
            .justify_end()
            .when(
                last_turn_tokens_label.is_some() || last_turn_clock.is_some(),
                |this| {
                    this.child(
                        h_flex()
                            .gap_1()
                            .px_1()
                            .when_some(last_turn_tokens_label, |this, label| this.child(label))
                            .when_some(last_turn_clock, |this, label| this.child(label)),
                    )
                },
            );

        let enable_thread_feedback = util::maybe!({
            let project = thread.read(cx).project().read(cx);
            let user_store = project.user_store();
            if let Some(configuration) = user_store.read(cx).current_organization_configuration() {
                if !configuration.is_agent_thread_feedback_enabled {
                    return false;
                }
            }

            AgentSettings::get_global(cx).enable_feedback
                && self.thread.read(cx).connection().telemetry().is_some()
        });

        if enable_thread_feedback {
            let feedback = self.thread_feedback.feedback;

            let tooltip_meta = || {
                SharedString::new(
                    "Rating the thread sends all of your current conversation to the Mav team.",
                )
            };

            container = container
                .child(
                    IconButton::new("feedback-thumbs-up", IconName::ThumbsUp)
                        .shape(ui::IconButtonShape::Square)
                        .icon_size(IconSize::Small)
                        .icon_color(match feedback {
                            Some(ThreadFeedback::Positive) => Color::Accent,
                            _ => Color::Ignored,
                        })
                        .tooltip(move |window, cx| match feedback {
                            Some(ThreadFeedback::Positive) => {
                                Tooltip::text("Thanks for your feedback!")(window, cx)
                            }
                            _ => Tooltip::with_meta("Helpful Response", None, tooltip_meta(), cx),
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.handle_feedback_click(ThreadFeedback::Positive, window, cx);
                        })),
                )
                .child(
                    IconButton::new("feedback-thumbs-down", IconName::ThumbsDown)
                        .shape(ui::IconButtonShape::Square)
                        .icon_size(IconSize::Small)
                        .icon_color(match feedback {
                            Some(ThreadFeedback::Negative) => Color::Accent,
                            _ => Color::Ignored,
                        })
                        .tooltip(move |window, cx| match feedback {
                            Some(ThreadFeedback::Negative) => Tooltip::text(
                                "We appreciate your feedback and will use it to improve in the future.",
                            )(window, cx),
                            _ => Tooltip::with_meta("Not Helpful Response", None, tooltip_meta(), cx),
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.handle_feedback_click(ThreadFeedback::Negative, window, cx);
                        })),
                );
        }

        container
            .child(open_as_markdown)
            .child(scroll_to_recent_user_prompt)
            .child(scroll_to_top)
            .into_any_element()
    }
}
