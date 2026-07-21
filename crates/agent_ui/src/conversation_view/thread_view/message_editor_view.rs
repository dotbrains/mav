use super::*;

impl ThreadView {
    pub(crate) fn render_message_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.is_subagent() {
            return div().into_any_element();
        }

        let focus_handle = self.message_editor.focus_handle(cx);
        let editor_bg_color = cx.theme().colors().editor_background;

        let editor_expanded = self.editor_expanded;
        let (expand_icon, expand_tooltip) = if editor_expanded {
            (IconName::Minimize, "Minimize Message Editor")
        } else {
            (IconName::Maximize, "Expand Message Editor")
        };

        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        let has_messages = self.list_state.item_count() > 0;
        let compact_editor = has_messages || self.is_draft(cx);
        let fills_container = !compact_editor || editor_expanded;
        let draft_agent_selector = self
            .is_draft(cx)
            .then(|| self.render_draft_agent_selector(cx));

        h_flex()
            .py_2()
            .bg(editor_bg_color)
            .justify_center()
            .on_action(cx.listener(Self::handle_message_editor_move_up))
            .map(|this| {
                if compact_editor {
                    this.on_action(cx.listener(Self::expand_message_editor))
                        .flex_none()
                        .border_t_1()
                        .border_color(cx.theme().colors().border)
                        .when(editor_expanded, |this| this.h(vh(0.8, window)))
                } else {
                    this.flex_1().size_full()
                }
            })
            .child(
                v_flex()
                    .when_some(max_content_width, |this, max_w| this.flex_basis(max_w))
                    .when(max_content_width.is_none(), |this| this.w_full())
                    .when(fills_container, |this| this.h_full())
                    .px_2()
                    .flex_shrink_1()
                    .flex_grow_0()
                    .justify_between()
                    .gap_2()
                    .child(
                        v_flex()
                            .relative()
                            .w_full()
                            .min_h_0()
                            .when(fills_container, |this| this.flex_1())
                            .pt_1()
                            .pr_2p5()
                            .child(self.message_editor.clone())
                            .when(has_messages, |this| {
                                this.child(
                                    h_flex()
                                        .absolute()
                                        .top_0()
                                        .right_0()
                                        .opacity(0.5)
                                        .hover(|s| s.opacity(1.0))
                                        .child(
                                            IconButton::new("toggle-height", expand_icon)
                                                .icon_size(IconSize::Small)
                                                .icon_color(Color::Muted)
                                                .tooltip({
                                                    move |_window, cx| {
                                                        Tooltip::for_action_in(
                                                            expand_tooltip,
                                                            &ExpandMessageEditor,
                                                            &focus_handle,
                                                            cx,
                                                        )
                                                    }
                                                })
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.expand_message_editor(
                                                        &ExpandMessageEditor,
                                                        window,
                                                        cx,
                                                    );
                                                })),
                                        ),
                                )
                            }),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_none()
                            .flex_wrap()
                            .justify_between()
                            .child(
                                h_flex()
                                    .gap_0p5()
                                    .child(self.render_add_context_button(cx))
                                    .child(self.render_follow_toggle(cx))
                                    .children(self.render_fast_mode_control(cx))
                                    .children(self.render_thinking_control(cx)),
                            )
                            .child(
                                h_flex()
                                    .flex_wrap()
                                    .gap_1()
                                    .children(self.render_token_usage(cx))
                                    .children(self.profile_selector.clone())
                                    .map(|this| match self.config_options_view.clone() {
                                        Some(config_view) => this.child(config_view),
                                        None => this
                                            .children(self.mode_selector.clone())
                                            .children(self.model_selector.clone()),
                                    })
                                    .children(draft_agent_selector)
                                    .child(self.render_send_button(cx)),
                            ),
                    ),
            )
            .into_any()
    }
}
