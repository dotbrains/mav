use super::*;

impl Render for ConversationView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(match &self.server_state {
                ServerState::Loading {
                    draft: Some(draft), ..
                } => self.render_loading_draft(draft, cx),
                ServerState::Loading { .. } => {
                    let label_text = self
                        .loading_status
                        .clone()
                        .unwrap_or_else(|| "Loading…".into());
                    v_flex()
                        .flex_1()
                        .size_full()
                        .items_center()
                        .justify_center()
                        .child(
                            Label::new(label_text).color(Color::Muted).with_animation(
                                "loading-agent-label",
                                Animation::new(Duration::from_secs(2))
                                    .repeat()
                                    .with_easing(pulsating_between(0.3, 0.7)),
                                |label, delta| label.alpha(delta),
                            ),
                        )
                        .into_any()
                }
                ServerState::LoadError { error: e, .. } => v_flex()
                    .flex_1()
                    .size_full()
                    .items_center()
                    .justify_end()
                    .child(self.render_load_error(e, window, cx))
                    .into_any(),
                ServerState::Connected(ConnectedServerState {
                    connection,
                    auth_state:
                        AuthState::Unauthenticated {
                            description,
                            configuration_view,
                            pending_auth_method,
                            _subscription,
                        },
                    ..
                }) => v_flex()
                    .flex_1()
                    .size_full()
                    .justify_end()
                    .child(self.render_auth_required_state(
                        connection,
                        description.as_ref(),
                        configuration_view.as_ref(),
                        pending_auth_method.as_ref(),
                        window,
                        cx,
                    ))
                    .into_any_element(),
                ServerState::Connected(connected) => {
                    if let Some(view) = connected.active_view() {
                        view.clone().into_any_element()
                    } else {
                        debug_panic!("This state should never be reached");
                        div().into_any_element()
                    }
                }
            })
    }
}
