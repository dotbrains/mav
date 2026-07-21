use super::*;

impl ThreadView {
    pub(super) fn activity_bar_bg(&self, cx: &Context<Self>) -> Hsla {
        let editor_bg_color = cx.theme().colors().editor_background;
        let active_color = cx.theme().colors().element_selected;
        editor_bg_color.blend(active_color.opacity(0.3))
    }

    pub(super) fn render_activity_bar(
        &self,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let thread = self.thread.read(cx);
        let action_log = thread.action_log();
        let telemetry = ActionLogTelemetry::from(thread);
        let changed_buffers = action_log.read(cx).changed_buffers(cx).collect::<Vec<_>>();
        let plan = thread.plan();
        let queue_is_empty = !self.has_queued_messages();

        let awaiting_permission = self
            .render_main_agent_awaiting_permission(window, cx)
            .or_else(|| self.render_subagents_awaiting_permission(cx));
        let has_awaiting_permission = awaiting_permission.is_some();

        if changed_buffers.is_empty()
            && plan.is_empty()
            && queue_is_empty
            && !has_awaiting_permission
        {
            return None;
        }

        // Temporarily always enable ACP edit controls. This is temporary, to lessen the
        // impact of a nasty bug that causes them to sometimes be disabled when they shouldn't
        // be, which blocks you from being able to accept or reject edits. This switches the
        // bug to be that sometimes it's enabled when it shouldn't be, which at least doesn't
        // block you from using the panel.
        let pending_edits = false;

        let plan_expanded = self.plan_expanded;
        let edits_expanded = self.edits_expanded;
        let queue_expanded = self.queue_expanded;

        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        // Drop shadows have no opaque surface to blend into on a transparent
        // window, so they render as a dark halo; only apply them when opaque.
        let opaque_window =
            cx.theme().window_background_appearance() == gpui::WindowBackgroundAppearance::Opaque;

        h_flex()
            .w_full()
            .px_2()
            .justify_center()
            .child(
                v_flex()
                    .when_some(max_content_width, |this, max_w| this.flex_basis(max_w))
                    .when(max_content_width.is_none(), |this| this.w_full())
                    .flex_shrink_1()
                    .flex_grow_0()
                    .max_w_full()
                    .bg(self.activity_bar_bg(cx))
                    .border_1()
                    .border_b_0()
                    .border_color(cx.theme().colors().border)
                    .rounded_t_md()
                    .when(opaque_window, |this| {
                        this.shadow(vec![
                            gpui::BoxShadow::new(px(1.), px(-1.), gpui::black().opacity(0.12))
                                .blur_radius(px(2.)),
                        ])
                    })
                    .when_some(awaiting_permission, |this, element| this.child(element))
                    .when(
                        has_awaiting_permission
                            && (!plan.is_empty() || !changed_buffers.is_empty() || !queue_is_empty),
                        |this| this.child(Divider::horizontal().color(DividerColor::Border)),
                    )
                    .when(!plan.is_empty(), |this| {
                        this.child(self.render_plan_summary(plan, window, cx))
                            .when(plan_expanded, |parent| {
                                parent.child(self.render_plan_entries(plan, window, cx))
                            })
                    })
                    .when(!plan.is_empty() && !changed_buffers.is_empty(), |this| {
                        this.child(Divider::horizontal().color(DividerColor::Border))
                    })
                    .when(
                        !changed_buffers.is_empty() && thread.parent_session_id().is_none(),
                        |this| {
                            this.child(self.render_edits_summary(
                                &changed_buffers,
                                edits_expanded,
                                pending_edits,
                                cx,
                            ))
                            .when(edits_expanded, |parent| {
                                parent.child(self.render_edited_files(
                                    action_log,
                                    telemetry.clone(),
                                    &changed_buffers,
                                    pending_edits,
                                    cx,
                                ))
                            })
                        },
                    )
                    .when(!queue_is_empty, |this| {
                        this.when(!plan.is_empty() || !changed_buffers.is_empty(), |this| {
                            this.child(Divider::horizontal().color(DividerColor::Border))
                        })
                        .child(self.render_message_queue_summary(window, cx))
                        .when(queue_expanded, |parent| {
                            parent.child(self.render_message_queue_entries(window, cx))
                        })
                    }),
            )
            .into_any()
            .into()
    }
}
