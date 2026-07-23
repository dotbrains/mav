use super::*;

pub(super) struct ConfigurationView {
    pub(super) state: Entity<State>,
    pub(super) http_client: Arc<dyn HttpClient>,
}

impl Render for ConfigurationView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);

        if state.is_authenticated() {
            let label = state
                .email()
                .map(|e| format!("Signed in as {e}"))
                .unwrap_or_else(|| "Signed in".to_string());

            let weak_state = self.state.downgrade();

            return v_flex()
                .child(
                    ConfiguredApiCard::new(SharedString::from(label))
                        .button_label("Sign Out")
                        .on_click(cx.listener(move |_this, _, _window, cx| {
                            do_sign_out(&weak_state, cx).detach_and_log_err(cx);
                        })),
                )
                .into_any_element();
        }

        let last_auth_error = state.last_auth_error.clone();
        let provider_state = self.state.clone();
        let http_client = self.http_client.clone();

        let is_signing_in = state.is_signing_in();
        let button_label = if is_signing_in {
            "Signing in…"
        } else {
            "Sign in to use ChatGPT Subscription"
        };

        v_flex()
            .gap_2()
            .child(Label::new(
                "Sign in with your ChatGPT Plus or Pro subscription to use OpenAI models in Mav's agent.",
            ))
            .child(
                Button::new("sign-in", button_label)
                    .full_width()
                    .style(ButtonStyle::Outlined)
                    .loading(is_signing_in)
                    .disabled(is_signing_in)
                    .when(!is_signing_in, |this| {
                        this.start_icon(
                            Icon::new(IconName::AiOpenAi)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .on_click(move |_, _window, cx| {
                        do_sign_in(&provider_state, &http_client, cx);
                    }),
            )
            .when_some(last_auth_error, |this, error| {
                this.child(
                    h_flex()
                        .gap_1()
                        .justify_center()
                        .child(
                            Icon::new(IconName::XCircle)
                                .color(Color::Error)
                                .size(IconSize::Small),
                        )
                        .child(Label::new(error).color(Color::Muted)),
                )
            })
            .into_any_element()
    }
}
