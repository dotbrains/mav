use super::*;

impl ThreadView {
    /// Returns the model to offer as a downgrade target when the current model
    /// requires data retention consent (e.g. Opus 4.8 for Fable).
    fn data_retention_fallback_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        let thread = self.as_native_thread(cx)?;
        let model = thread.read(cx).model()?.clone();
        let fallback_id = model.refusal_fallback_model_id()?;
        LanguageModelRegistry::read_global(cx)
            .available_models(cx)
            .find(|fallback| {
                fallback.provider_id() == model.provider_id()
                    && fallback.id().0.as_ref() == fallback_id
            })
    }

    pub(super) fn render_data_retention_consent_error(&self, cx: &mut Context<Self>) -> Callout {
        let fallback_model = self.data_retention_fallback_model(cx);

        Callout::new()
            .severity(Severity::Warning)
            .icon(IconName::Warning)
            .title(format!(
                "Note: {} cannot be offered with Zero Data Retention.",
                self.current_model_name(cx)
            ))
            .description_slot(
                h_flex()
                    .gap_1()
                    .child(
                        Label::new("Anthropic will retain inference logs.")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Button::new("data-retention-learn-more", "Learn More")
                            .label_size(LabelSize::Small)
                            .on_click(|_, _, cx| {
                                cx.open_url(DATA_RETENTION_LEARN_MORE_URL);
                            }),
                    ),
            )
            .actions_slot(
                h_flex()
                    .gap_0p5()
                    .when_some(fallback_model, |this, fallback| {
                        this.child(
                            Button::new(
                                "switch-data-retention-fallback",
                                format!("Switch to {}", fallback.name().0),
                            )
                            .label_size(LabelSize::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.switch_to_data_retention_fallback_and_resend(cx);
                            })),
                        )
                    })
                    .child(
                        Button::new("accept-data-retention", "Accept")
                            .label_size(LabelSize::Small)
                            .style(ButtonStyle::Tinted(TintColor::Warning))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.accept_data_retention_and_resend(cx);
                            })),
                    ),
            )
            .dismiss_action(self.dismiss_error_button(cx))
    }

    fn accept_data_retention_and_resend(&mut self, cx: &mut Context<Self>) {
        let fs = self.thread.read(cx).project().read(cx).fs().clone();
        // Resume the failed turn only once the in-memory settings reflect
        // consent, otherwise the resent request would be rejected again.
        let completion = update_settings_file_with_completion(fs, cx, |settings, _| {
            settings
                .telemetry
                .get_or_insert_default()
                .anthropic_retention = Some(true);
        });
        cx.spawn(async move |this, cx| {
            completion.await??;
            this.update(cx, |this, cx| this.retry_generation(cx))?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    fn switch_to_data_retention_fallback_and_resend(&mut self, cx: &mut Context<Self>) {
        let Some(fallback) = self.data_retention_fallback_model(cx) else {
            return;
        };
        let model_id = acp_thread::AgentModelId::new(format!(
            "{}/{}",
            fallback.provider_id().0,
            fallback.id().0
        ));
        let session_id = self.thread.read(cx).session_id().clone();
        let Some(selector) = self
            .thread
            .read(cx)
            .connection()
            .model_selector(&session_id)
        else {
            return;
        };
        let select = selector.select_model(model_id, cx);
        cx.spawn(async move |this, cx| {
            select.await?;
            this.update(cx, |this, cx| this.retry_generation(cx))?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}
