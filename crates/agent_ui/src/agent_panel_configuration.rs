use super::*;

impl AgentPanel {
    pub(crate) fn open_configuration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // When the agent settings have been moved into the settings UI, the
        // panel no longer shows its own configuration overlay. Instead, route to
        // the settings UI at the LLM providers page (where the model selector's
        // "Configure" button expects to land).
        if cx.has_flag::<AgentSettingsUiFeatureFlag>() {
            window.dispatch_action(
                Box::new(mav_actions::OpenSettingsAt {
                    path: "llm_providers".to_string(),
                    target: None,
                }),
                cx,
            );
            return;
        }

        if matches!(self.overlay_view, Some(OverlayView::Configuration)) {
            self.clear_overlay(true, window, cx);
            return;
        }

        let agent_server_store = self.project.read(cx).agent_server_store().clone();
        let context_server_store = self.project.read(cx).context_server_store();
        let fs = self.fs.clone();

        self.configuration = Some(cx.new(|cx| {
            AgentConfiguration::new(
                fs,
                agent_server_store,
                self.connection_store.clone(),
                context_server_store,
                self.context_server_registry.clone(),
                self.language_registry.clone(),
                self.workspace.clone(),
                window,
                cx,
            )
        }));

        if let Some(configuration) = self.configuration.as_ref() {
            self.configuration_subscription = Some(cx.subscribe_in(
                configuration,
                window,
                Self::handle_agent_configuration_event,
            ));
        }

        self.set_overlay(OverlayView::Configuration, true, window, cx);

        if let Some(configuration) = self.configuration.as_ref() {
            configuration.focus_handle(cx).focus(window, cx);
        }
    }

    fn handle_agent_configuration_event(
        &mut self,
        _entity: &Entity<AgentConfiguration>,
        event: &AssistantConfigurationEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AssistantConfigurationEvent::NewThread(provider) => {
                if LanguageModelRegistry::read_global(cx)
                    .default_model()
                    .is_none_or(|model| model.provider.id() != provider.id())
                    && let Some(model) = provider.default_model(cx)
                {
                    update_settings_file(self.fs.clone(), cx, move |settings, _| {
                        let provider = model.provider_id().0.to_string();
                        let enable_thinking = model.supports_thinking();
                        let effort = model
                            .default_effort_level()
                            .map(|effort| effort.value.to_string());
                        let model = model.id().0.to_string();
                        settings
                            .agent
                            .get_or_insert_default()
                            .set_model(LanguageModelSelection {
                                provider: LanguageModelProviderSetting(provider),
                                model,
                                enable_thinking,
                                effort,
                                speed: None,
                            })
                    });
                }

                self.activate_new_thread(true, AgentThreadSource::AgentPanel, window, cx);
                if let Some((thread, model)) = self
                    .active_native_agent_thread(cx)
                    .zip(provider.default_model(cx))
                {
                    thread.update(cx, |thread, cx| {
                        thread.set_model(model, cx);
                    });
                }
            }
        }
    }
}
