use super::*;

impl ThreadView {
    fn is_subagent_canceled_or_failed(&self, cx: &App) -> bool {
        let Some(parent_session_id) = self.parent_session_id.as_ref() else {
            return false;
        };

        let my_session_id = self.thread.read(cx).session_id().clone();

        self.server_view
            .upgrade()
            .and_then(|sv| sv.read(cx).thread_view(parent_session_id))
            .is_some_and(|parent_view| {
                parent_view
                    .read(cx)
                    .thread
                    .read(cx)
                    .tool_call_for_subagent(&my_session_id)
                    .is_some_and(|tc| {
                        matches!(
                            tc.status,
                            ToolCallStatus::Canceled
                                | ToolCallStatus::Failed
                                | ToolCallStatus::Rejected
                        )
                    })
            })
    }

    pub(crate) fn render_subagent_titlebar(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if self.parent_session_id.is_none() {
            return None;
        }
        let parent_session_id = self.thread.read(cx).parent_session_id()?.clone();

        let server_view = self.server_view.clone();
        let thread = self.thread.clone();
        let is_done = thread.read(cx).status() == ThreadStatus::Idle;
        let is_canceled_or_failed = self.is_subagent_canceled_or_failed(cx);

        let max_content_width = AgentSettings::get_global(cx).max_content_width;

        Some(
            h_flex()
                .w_full()
                .h(Tab::container_height(cx))
                .border_b_1()
                .when(is_done && is_canceled_or_failed, |this| {
                    this.border_dashed()
                })
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().editor_background.opacity(0.2))
                .child(
                    h_flex()
                        .size_full()
                        .when_some(max_content_width, |this, max_w| this.max_w(max_w).mx_auto())
                        .pl_2()
                        .pr_1()
                        .flex_shrink_0()
                        .justify_between()
                        .gap_1()
                        .child(
                            h_flex()
                                .flex_1()
                                .gap_2()
                                .child(
                                    Icon::new(IconName::ForwardArrowUp)
                                        .size(IconSize::Small)
                                        .color(Color::Muted),
                                )
                                .child(self.title_editor.clone())
                                .when(is_done && is_canceled_or_failed, |this| {
                                    this.child(Icon::new(IconName::Close).color(Color::Error))
                                })
                                .when(is_done && !is_canceled_or_failed, |this| {
                                    this.child(Icon::new(IconName::Check).color(Color::Success))
                                }),
                        )
                        .child(
                            h_flex()
                                .gap_0p5()
                                .when(!is_done, |this| {
                                    this.child(
                                        IconButton::new("stop_subagent", IconName::Stop)
                                            .icon_size(IconSize::Small)
                                            .icon_color(Color::Error)
                                            .tooltip(Tooltip::text("Stop Subagent"))
                                            .on_click(move |_, _, cx| {
                                                thread.update(cx, |thread, cx| {
                                                    thread.cancel(cx).detach();
                                                });
                                            }),
                                    )
                                })
                                .child(
                                    IconButton::new("minimize_subagent", IconName::Dash)
                                        .icon_size(IconSize::Small)
                                        .tooltip(Tooltip::text("Minimize Subagent"))
                                        .on_click(move |_, window, cx| {
                                            let _ = server_view.update(cx, |server_view, cx| {
                                                server_view.navigate_to_thread(
                                                    parent_session_id.clone(),
                                                    window,
                                                    cx,
                                                );
                                            });
                                        }),
                                ),
                        ),
                ),
        )
    }
}
