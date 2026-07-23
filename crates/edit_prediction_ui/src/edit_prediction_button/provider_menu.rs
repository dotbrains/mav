use super::*;

impl EditPredictionButton {
    pub(crate) fn add_provider_switching_section(
        &self,
        mut menu: ContextMenu,
        current_provider: EditPredictionProvider,
        cx: &mut App,
    ) -> ContextMenu {
        let organization_configuration = self
            .user_store
            .read(cx)
            .current_organization_configuration();

        let is_mav_provider_disabled = organization_configuration
            .is_some_and(|configuration| !configuration.edit_prediction.is_enabled);

        let available_providers = get_available_providers(cx);

        let providers: Vec<_> = available_providers
            .into_iter()
            .filter(|p| *p != EditPredictionProvider::None)
            .collect();

        if !providers.is_empty() {
            menu = menu.separator().header("Providers");

            for provider in providers {
                let Some(name) = provider.display_name() else {
                    continue;
                };
                let is_current = provider == current_provider;
                let is_disabled_mav_provider =
                    provider == EditPredictionProvider::Mav && is_mav_provider_disabled;
                let fs = self.fs.clone();

                menu = menu.item(
                    ContextMenuEntry::new(name)
                        .toggleable(IconPosition::Start, is_current && !is_disabled_mav_provider)
                        .disabled(is_disabled_mav_provider)
                        .when(is_disabled_mav_provider, |item| {
                            item.documentation_aside(DocumentationSide::Left, move |_cx| {
                                Label::new("Edit predictions are disabled for this organization.")
                                    .into_any_element()
                            })
                        })
                        .handler(move |_, cx| {
                            set_completion_provider(fs.clone(), cx, provider);
                        }),
                )
            }
        }

        menu
    }

    pub(crate) fn add_configure_providers_item(&self, menu: ContextMenu) -> ContextMenu {
        menu.separator().item(
            ContextMenuEntry::new("Configure Providers")
                .icon(IconName::Settings)
                .icon_position(IconPosition::Start)
                .icon_color(Color::Muted)
                .handler(move |window, cx| {
                    telemetry::event!(
                        "Edit Prediction Menu Action",
                        action = "configure_providers",
                    );
                    window.dispatch_action(
                        OpenSettingsAt {
                            path: "edit_predictions.providers".to_string(),
                            target: None,
                        }
                        .boxed_clone(),
                        cx,
                    );
                }),
        )
    }

    pub fn build_copilot_start_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let fs = self.fs.clone();
        let project = self.project.clone();
        ContextMenu::build(window, cx, |menu, _, cx| {
            let menu = menu
                .entry("Sign In to Copilot", None, move |window, cx| {
                    telemetry::event!(
                        "Edit Prediction Menu Action",
                        action = "sign_in",
                        provider = "copilot",
                    );
                    if let Some(copilot) = EditPredictionStore::try_global(cx).and_then(|store| {
                        store.update(cx, |this, cx| {
                            this.start_copilot_for_project(&project.upgrade()?, cx)
                        })
                    }) {
                        copilot_ui::initiate_sign_in(copilot, window, cx);
                    }
                })
                .entry("Disable Copilot", None, {
                    let fs = fs.clone();
                    move |_window, cx| {
                        telemetry::event!(
                            "Edit Prediction Menu Action",
                            action = "disable_provider",
                            provider = "copilot",
                        );
                        hide_copilot(fs.clone(), cx)
                    }
                });

            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Copilot, cx);
            let menu = self.add_configure_providers_item(menu);
            menu
        })
    }
}
