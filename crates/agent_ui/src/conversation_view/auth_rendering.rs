use super::*;

impl ConversationView {
    pub(super) fn render_auth_required_state(
        &self,
        connection: &Rc<dyn AgentConnection>,
        description: Option<&Entity<Markdown>>,
        configuration_view: Option<&AnyView>,
        pending_auth_method: Option<&acp::AuthMethodId>,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let auth_methods = connection.auth_methods();

        let agent_display_name = self
            .agent_server_store
            .read(cx)
            .agent_display_name(&self.agent.agent_id())
            .unwrap_or_else(|| self.agent.agent_id().0);

        let show_fallback_description = auth_methods.len() > 1
            && configuration_view.is_none()
            && description.is_none()
            && pending_auth_method.is_none();

        let auth_buttons = || {
            h_flex().justify_end().flex_wrap().gap_1().children(
                connection
                    .auth_methods()
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(ix, method)| {
                        let (method_id, name) = (method.id().0.clone(), method.name().to_string());
                        let agent_telemetry_id = connection.telemetry_id();

                        Button::new(method_id.clone(), name)
                            .label_size(LabelSize::Small)
                            .map(|this| {
                                if ix == 0 {
                                    this.style(ButtonStyle::Tinted(TintColor::Accent))
                                } else {
                                    this.style(ButtonStyle::Outlined)
                                }
                            })
                            .when_some(method.description(), |this, description| {
                                this.tooltip(Tooltip::text(description.to_string()))
                            })
                            .on_click({
                                cx.listener(move |this, _, window, cx| {
                                    telemetry::event!(
                                        "Authenticate Agent Started",
                                        agent = agent_telemetry_id,
                                        method = method_id
                                    );

                                    this.authenticate(
                                        acp::AuthMethodId::new(method_id.clone()),
                                        window,
                                        cx,
                                    )
                                })
                            })
                    }),
            )
        };

        if pending_auth_method.is_some() {
            return Callout::new()
                .icon(IconName::Info)
                .title(format!("Authenticating to {}…", agent_display_name))
                .actions_slot(
                    Icon::new(IconName::ArrowCircle)
                        .size(IconSize::Small)
                        .color(Color::Muted)
                        .with_rotate_animation(2)
                        .into_any_element(),
                )
                .into_any_element();
        }

        Callout::new()
            .icon(IconName::Info)
            .title(format!("Authenticate to {}", agent_display_name))
            .when(auth_methods.len() == 1, |this| {
                this.actions_slot(auth_buttons())
            })
            .description_slot(
                v_flex()
                    .text_ui(cx)
                    .map(|this| {
                        if show_fallback_description {
                            this.child(
                                Label::new("Choose one of the following authentication options:")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        } else {
                            this.children(
                                configuration_view
                                    .cloned()
                                    .map(|view| div().w_full().child(view)),
                            )
                            .children(description.map(|desc| {
                                self.render_markdown(
                                    desc.clone(),
                                    MarkdownStyle::themed(MarkdownFont::Agent, window, cx),
                                    cx,
                                )
                            }))
                        }
                    })
                    .when(auth_methods.len() > 1, |this| {
                        this.gap_1().child(auth_buttons())
                    }),
            )
            .into_any_element()
    }
}
