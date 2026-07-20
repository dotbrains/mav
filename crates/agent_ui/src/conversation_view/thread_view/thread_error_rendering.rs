use super::*;

impl ThreadView {
    pub(crate) fn render_thread_error(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let callout = match self.thread_error.as_ref()? {
            ThreadError::Other { message, .. } => {
                self.render_any_thread_error(message.clone(), window, cx)
            }
            ThreadError::Refusal => self.render_refusal_error(cx),
            ThreadError::DataRetentionConsentRequired => {
                self.render_data_retention_consent_error(cx)
            }
            ThreadError::AuthenticationRequired(error) => {
                self.render_authentication_required_error(error.clone(), cx)
            }
            ThreadError::PaymentRequired => self.render_payment_required_error(cx),
            ThreadError::RateLimitExceeded { provider } => self.render_error_callout(
                "Rate Limit Reached",
                format!(
                    "{provider}'s rate limit was reached. Mav will retry automatically. \
                    You can also wait a moment and try again."
                )
                .into(),
                true,
                true,
                cx,
            ),
            ThreadError::ServerOverloaded { provider } => self.render_error_callout(
                "Provider Unavailable",
                format!(
                    "{provider}'s servers are temporarily unavailable. Mav will retry \
                    automatically. If the problem persists, check the provider's status page."
                )
                .into(),
                true,
                true,
                cx,
            ),
            ThreadError::PromptTooLarge => self.render_prompt_too_large_error(cx),
            ThreadError::NoCredentials { provider } => {
                let message = Self::provider_by_name(provider, cx)
                    .map(|provider| provider.missing_credentials_error_message())
                    .unwrap_or_else(|| {
                        format!("No credentials are configured for {provider}.").into()
                    });
                self.render_error_callout("Credentials Missing", message, false, true, cx)
            }
            ThreadError::StreamError { provider } => self.render_error_callout(
                "Connection Interrupted",
                format!(
                    "The connection to {provider}'s API was interrupted. Mav will retry \
                    automatically. If the problem persists, check your network connection."
                )
                .into(),
                true,
                true,
                cx,
            ),
            ThreadError::AuthenticationFailed { provider } => {
                let message = Self::provider_by_name(provider, cx)
                    .map(|provider| provider.authentication_error_message())
                    .unwrap_or_else(|| format!("Could not authenticate with {provider}.").into());
                self.render_error_callout("Authentication Failed", message, false, false, cx)
            }
            ThreadError::PermissionDenied { provider, message } => {
                let message: SharedString = message.clone().unwrap_or_else(|| {
                    format!("{provider} rejected the request due to insufficient permissions.")
                        .into()
                });

                self.render_error_callout("Permission Denied", message, false, false, cx)
            }
            ThreadError::RequestFailed => self.render_error_callout(
                "Request Failed",
                "The request could not be completed after multiple attempts. \
                Try again in a moment."
                    .into(),
                true,
                false,
                cx,
            ),
            ThreadError::MaxOutputTokens => self.render_error_callout(
                "Output Limit Reached",
                "The model stopped because it reached its maximum output length. \
                You can ask it to continue where it left off."
                    .into(),
                false,
                false,
                cx,
            ),
            ThreadError::NoModelSelected => self.render_error_callout(
                "No Model Selected",
                "Select a model from the model picker below to get started.".into(),
                false,
                false,
                cx,
            ),
            ThreadError::ApiError { provider } => self.render_error_callout(
                "API Error",
                format!(
                    "{provider}'s API returned an unexpected error. \
                    If the problem persists, try switching models or restarting Mav."
                )
                .into(),
                true,
                true,
                cx,
            ),
        };

        Some(div().child(callout.border_position(self.callout_border_position())))
    }

    fn render_refusal_error(&self, cx: &mut Context<'_, Self>) -> Callout {
        let model_or_agent_name = self.current_model_name(cx);
        let refusal_message = format!(
            "{} refused to respond to this prompt. \
            This can happen when a model believes the prompt violates its content policy \
            or safety guidelines, so rephrasing it can sometimes address the issue.",
            model_or_agent_name
        );

        Callout::new()
            .severity(Severity::Error)
            .title("Request Refused")
            .icon(IconName::XCircle)
            .description(refusal_message.clone())
            .actions_slot(self.create_copy_button(&refusal_message))
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn render_authentication_required_error(
        &self,
        error: SharedString,
        cx: &mut Context<Self>,
    ) -> Callout {
        Callout::new()
            .severity(Severity::Error)
            .title("Authentication Required")
            .icon(IconName::XCircle)
            .description(error.clone())
            .actions_slot(
                h_flex()
                    .gap_0p5()
                    .child(self.authenticate_button(cx))
                    .child(self.create_copy_button(error)),
            )
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn render_payment_required_error(&self, cx: &mut Context<Self>) -> Callout {
        const ERROR_MESSAGE: &str =
            "You reached your free usage limit. Upgrade to Mav Pro for more prompts.";

        Callout::new()
            .severity(Severity::Error)
            .icon(IconName::XCircle)
            .title("Free Usage Exceeded")
            .description(ERROR_MESSAGE)
            .actions_slot(
                h_flex()
                    .gap_0p5()
                    .child(self.upgrade_button(cx))
                    .child(self.create_copy_button(ERROR_MESSAGE)),
            )
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn render_error_callout(
        &self,
        title: &'static str,
        message: SharedString,
        show_retry: bool,
        show_copy: bool,
        cx: &mut Context<Self>,
    ) -> Callout {
        let can_resume = show_retry && self.thread.read(cx).can_retry(cx);
        let show_actions = can_resume || show_copy;

        Callout::new()
            .severity(Severity::Error)
            .icon(IconName::XCircle)
            .title(title)
            .description(message.clone())
            .when(show_actions, |callout| {
                callout.actions_slot(
                    h_flex()
                        .gap_0p5()
                        .when(can_resume, |this| this.child(self.retry_button(cx)))
                        .when(show_copy, |this| {
                            this.child(self.create_copy_button(message.clone()))
                        }),
                )
            })
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn render_prompt_too_large_error(&self, cx: &mut Context<Self>) -> Callout {
        const MESSAGE: &str = "This conversation is too long for the model's context window. \
            Start a new thread or remove some attached files to continue.";

        Callout::new()
            .severity(Severity::Error)
            .icon(IconName::XCircle)
            .title("Context Too Large")
            .description(MESSAGE)
            .actions_slot(
                h_flex()
                    .gap_0p5()
                    .child(self.new_thread_button(cx))
                    .child(self.create_copy_button(MESSAGE)),
            )
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn retry_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("retry", "Retry")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Filled)
            .on_click(cx.listener(|this, _, _, cx| {
                this.retry_generation(cx);
            }))
    }

    fn new_thread_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("new_thread", "New Thread")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Filled)
            .on_click(cx.listener(|this, _, window, cx| {
                this.clear_thread_error(cx);
                window.dispatch_action(NewThread.boxed_clone(), cx);
            }))
    }

    fn upgrade_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("upgrade", "Upgrade")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Tinted(ui::TintColor::Accent))
            .on_click(cx.listener({
                move |this, _, _, cx| {
                    this.clear_thread_error(cx);
                    cx.open_url(&mav_urls::upgrade_to_mav_pro_url(cx));
                }
            }))
    }

    fn authenticate_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("authenticate", "Authenticate")
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Filled)
            .on_click(cx.listener({
                move |this, _, window, cx| {
                    let server_view = this.server_view.clone();
                    let agent_name = this.agent_id.clone();

                    this.clear_thread_error(cx);
                    if let Some(message) = this.in_flight_prompt.take() {
                        this.message_editor.update(cx, |editor, cx| {
                            editor.set_message(message, window, cx);
                        });
                    }
                    let connection = this.thread.read(cx).connection().clone();
                    window.defer(cx, |window, cx| {
                        ConversationView::handle_auth_required(
                            server_view,
                            AuthRequired::new(),
                            agent_name,
                            connection,
                            window,
                            cx,
                        );
                    })
                }
            }))
    }

    pub(super) fn current_model_name(&self, cx: &App) -> SharedString {
        if self.as_native_connection(cx).is_some() {
            self.model_selector
                .clone()
                .and_then(|selector| selector.read(cx).active_model(cx))
                .map(|model| model.name.clone())
                .unwrap_or_else(|| SharedString::from("The model"))
        } else {
            self.agent_id.0.clone()
        }
    }

    fn render_any_thread_error(
        &mut self,
        error: SharedString,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Callout {
        let can_resume = self.thread.read(cx).can_retry(cx);

        let markdown = if let Some(markdown) = &self.thread_error_markdown {
            markdown.clone()
        } else {
            let markdown = cx.new(|cx| Markdown::new(error.clone(), None, None, cx));
            self.thread_error_markdown = Some(markdown.clone());
            markdown
        };

        let markdown_style =
            MarkdownStyle::themed(MarkdownFont::Agent, window, cx).with_muted_text(cx);
        let description = self
            .render_markdown(markdown, markdown_style, cx)
            .into_any_element();

        Callout::new()
            .severity(Severity::Error)
            .icon(IconName::XCircle)
            .title("An Error Happened")
            .description_slot(description)
            .actions_slot(
                h_flex()
                    .gap_0p5()
                    .when(can_resume, |this| {
                        this.child(
                            IconButton::new("retry", IconName::RotateCw)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Retry Generation"))
                                .on_click(cx.listener(|this, _, _window, cx| {
                                    this.retry_generation(cx);
                                })),
                        )
                    })
                    .child(self.create_copy_button(error.to_string())),
            )
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn create_copy_button(&self, message: impl Into<String>) -> impl IntoElement {
        let message = message.into();

        CopyButton::new("copy-error-message", message).tooltip_label("Copy Error Message")
    }

    pub(super) fn dismiss_error_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        IconButton::new("dismiss", IconName::Close)
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Dismiss"))
            .on_click(cx.listener({
                move |this, _, _, cx| {
                    this.clear_thread_error(cx);
                    cx.notify();
                }
            }))
    }

    pub(super) fn render_resume_notice(_cx: &Context<Self>) -> AnyElement {
        let description = "This agent does not support viewing previous messages. However, your session will still continue from where you last left off.";

        Callout::new()
            .border_position(CalloutBorderPosition::Bottom)
            .severity(Severity::Info)
            .icon(IconName::Info)
            .title("Resumed Session")
            .description(description)
            .into_any_element()
    }
}
