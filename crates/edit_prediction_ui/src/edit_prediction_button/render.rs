use super::*;

impl Render for EditPredictionButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Return empty div if AI is disabled
        if DisableAiSettings::get_global(cx).disable_ai {
            return div().hidden();
        }

        let language_settings = all_language_settings(None, cx);

        match language_settings.edit_predictions.provider {
            EditPredictionProvider::Copilot => {
                let Some(copilot) = EditPredictionStore::try_global(cx)
                    .and_then(|store| store.read(cx).copilot_for_project(&self.project.upgrade()?))
                else {
                    return div().hidden();
                };
                let status = copilot.read(cx).status();

                let enabled = self.editor_enabled.unwrap_or(false);

                let icon = match status {
                    Status::Error(_) => IconName::CopilotError,
                    Status::Authorized => {
                        if enabled {
                            IconName::Copilot
                        } else {
                            IconName::CopilotDisabled
                        }
                    }
                    _ => IconName::CopilotInit,
                };

                if let Status::Error(e) = status {
                    return div().child(
                        IconButton::new("copilot-error", icon)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(move |_, _, window, cx| {
                                if let Some(workspace) = Workspace::for_window(window, cx) {
                                    workspace.update(cx, |workspace, cx| {
                                        let copilot = copilot.clone();
                                        workspace.show_toast(
                                            Toast::new(
                                                NotificationId::unique::<CopilotErrorToast>(),
                                                format!("Copilot can't be started: {}", e),
                                            )
                                            .on_click(
                                                "Reinstall Copilot",
                                                move |window, cx| {
                                                    copilot_ui::reinstall_and_sign_in(
                                                        copilot.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                },
                                            ),
                                            cx,
                                        );
                                    });
                                }
                            }))
                            .tooltip(|_window, cx| {
                                Tooltip::for_action("GitHub Copilot", &ToggleMenu, cx)
                            }),
                    );
                }
                let this = cx.weak_entity();
                let project = self.project.clone();
                let file = self.file.clone();
                let language = self.language.clone();
                div().child(
                    PopoverMenu::new("copilot")
                        .on_open({
                            let file = file.clone();
                            let language = language;
                            let project = project.clone();
                            Rc::new(move |_window, cx| {
                                emit_edit_prediction_menu_opened(
                                    "copilot", &file, &language, &project, cx,
                                );
                            })
                        })
                        .menu(move |window, cx| {
                            let current_status = EditPredictionStore::try_global(cx)
                                .and_then(|store| {
                                    store.read(cx).copilot_for_project(&project.upgrade()?)
                                })?
                                .read(cx)
                                .status();
                            match current_status {
                                Status::Authorized => this.update(cx, |this, cx| {
                                    this.build_copilot_context_menu(window, cx)
                                }),
                                _ => this.update(cx, |this, cx| {
                                    this.build_copilot_start_menu(window, cx)
                                }),
                            }
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("copilot-icon", icon),
                            |_window, cx| Tooltip::for_action("GitHub Copilot", &ToggleMenu, cx),
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::Codestral => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let has_api_key = codestral::codestral_api_key(cx).is_some();
                let this = cx.weak_entity();
                let file = self.file.clone();
                let language = self.language.clone();
                let project = self.project.clone();

                let tooltip_meta = if has_api_key {
                    "Powered by Codestral"
                } else {
                    "Missing API key for Codestral"
                };

                div().child(
                    PopoverMenu::new("codestral")
                        .on_open({
                            let file = file.clone();
                            let language = language;
                            let project = project;
                            Rc::new(move |_window, cx| {
                                emit_edit_prediction_menu_opened(
                                    "codestral",
                                    &file,
                                    &language,
                                    &project,
                                    cx,
                                );
                            })
                        })
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_codestral_context_menu(window, cx)
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("codestral-icon", IconName::AiMistral)
                                .shape(IconButtonShape::Square)
                                .when(!has_api_key, |this| {
                                    this.indicator(Indicator::dot().color(Color::Error))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                })
                                .when(has_api_key && !enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                            move |_window, cx| {
                                Tooltip::with_meta(
                                    "Edit Prediction",
                                    Some(&ToggleMenu),
                                    tooltip_meta,
                                    cx,
                                )
                            },
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::OpenAiCompatibleApi => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let this = cx.weak_entity();

                div().child(
                    PopoverMenu::new("openai-compatible-api")
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_edit_prediction_context_menu(
                                    EditPredictionProvider::OpenAiCompatibleApi,
                                    window,
                                    cx,
                                )
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger(
                            IconButton::new("openai-compatible-api-icon", IconName::AiOpenAiCompat)
                                .shape(IconButtonShape::Square)
                                .when(!enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::Ollama => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let this = cx.weak_entity();

                div().child(
                    PopoverMenu::new("ollama")
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_edit_prediction_context_menu(
                                    EditPredictionProvider::Ollama,
                                    window,
                                    cx,
                                )
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("ollama-icon", IconName::AiOllama)
                                .shape(IconButtonShape::Square)
                                .when(!enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                            move |_window, cx| {
                                let settings = all_language_settings(None, cx);
                                let tooltip_meta = match settings.edit_predictions.ollama.as_ref() {
                                    Some(settings) if !settings.model.trim().is_empty() => {
                                        format!("Powered by Ollama ({})", settings.model)
                                    }
                                    _ => {
                                        "Ollama model not configured — configure a model before use"
                                            .to_string()
                                    }
                                };

                                Tooltip::with_meta(
                                    "Edit Prediction",
                                    Some(&ToggleMenu),
                                    tooltip_meta,
                                    cx,
                                )
                            },
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            provider @ (EditPredictionProvider::Mav | EditPredictionProvider::Mercury) => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let file = self.file.clone();
                let language = self.language.clone();
                let project = self.project.clone();
                let provider_name: &'static str = match provider {
                    EditPredictionProvider::Mav => "mav",
                    _ => "unknown",
                };
                let icons = self
                    .edit_prediction_provider
                    .as_ref()
                    .map(|p| p.icons(cx))
                    .unwrap_or_else(|| {
                        edit_prediction_types::EditPredictionIconSet::new(IconName::MavPredict)
                    });

                let ep_icon;
                let tooltip_meta;
                let mut missing_token = false;

                match provider {
                    EditPredictionProvider::Mercury => {
                        ep_icon = if enabled { icons.base } else { icons.disabled };
                        let mercury_has_error =
                            edit_prediction::EditPredictionStore::try_global(cx).is_some_and(
                                |ep_store| ep_store.read(cx).mercury_has_payment_required_error(),
                            );
                        missing_token = edit_prediction::EditPredictionStore::try_global(cx)
                            .is_some_and(|ep_store| !ep_store.read(cx).has_mercury_api_token(cx));
                        tooltip_meta = if missing_token {
                            "Missing API key for Mercury"
                        } else if mercury_has_error {
                            "Mercury free tier limit reached"
                        } else {
                            "Powered by Mercury"
                        };
                    }
                    _ => {
                        ep_icon = if enabled { icons.base } else { icons.disabled };
                        tooltip_meta = "Powered by Zeta"
                    }
                };

                if edit_prediction::should_show_upsell_modal(cx) {
                    let tooltip_meta = if self.user_store.read(cx).current_user().is_some() {
                        "Choose a Plan"
                    } else {
                        "Configure a Provider"
                    };

                    return div().child(
                        IconButton::new("mav-predict-pending-button", ep_icon)
                            .shape(IconButtonShape::Square)
                            .indicator(Indicator::dot().color(Color::Muted))
                            .indicator_border_color(Some(cx.theme().colors().status_bar_background))
                            .tooltip(move |_window, cx| {
                                Tooltip::with_meta("Edit Predictions", None, tooltip_meta, cx)
                            })
                            .on_click(cx.listener(move |_, _, window, cx| {
                                telemetry::event!(
                                    "Pending ToS Clicked",
                                    source = "Edit Prediction Status Button"
                                );
                                window.dispatch_action(
                                    mav_actions::OpenMavPredictOnboarding.boxed_clone(),
                                    cx,
                                );
                            })),
                    );
                }

                let mut over_limit = false;

                if let Some(usage) = self
                    .edit_prediction_provider
                    .as_ref()
                    .and_then(|provider| provider.usage(cx))
                {
                    over_limit = usage.over_limit()
                }

                let show_editor_predictions = self.editor_show_predictions;
                let user = self.user_store.read(cx).current_user();

                let mercury_has_error = matches!(provider, EditPredictionProvider::Mercury)
                    && edit_prediction::EditPredictionStore::try_global(cx).is_some_and(
                        |ep_store| ep_store.read(cx).mercury_has_payment_required_error(),
                    );

                let indicator_color = if missing_token || mercury_has_error {
                    Some(Color::Error)
                } else if enabled && (!show_editor_predictions || over_limit) {
                    Some(if over_limit {
                        Color::Error
                    } else {
                        Color::Muted
                    })
                } else {
                    None
                };

                let mav_cloud_needs_sign_in =
                    matches!(provider, EditPredictionProvider::Mav) && user.is_none();
                let provider_unavailable =
                    missing_token || mercury_has_error || mav_cloud_needs_sign_in;

                let icon_button = IconButton::new("mav-predict-pending-button", ep_icon)
                    .shape(IconButtonShape::Square)
                    .when_some(indicator_color, |this, color| {
                        this.indicator(Indicator::dot().color(color))
                            .indicator_border_color(Some(cx.theme().colors().status_bar_background))
                    })
                    .when(!self.popover_menu_handle.is_deployed(), |element| {
                        element.tooltip(move |_window, cx| {
                            let description = if !enabled {
                                "Disabled For This File"
                            } else if mav_cloud_needs_sign_in {
                                "Sign In Or Configure a Provider"
                            } else if provider_unavailable || show_editor_predictions {
                                tooltip_meta
                            } else {
                                "Enable to Use"
                            };

                            Tooltip::with_meta(
                                "Edit Prediction",
                                Some(&ToggleMenu),
                                description,
                                cx,
                            )
                        })
                    });

                let this = cx.weak_entity();

                let mut popover_menu = PopoverMenu::new("edit-prediction")
                    .on_open({
                        let file = file.clone();
                        let language = language;
                        let project = project;
                        Rc::new(move |_window, cx| {
                            emit_edit_prediction_menu_opened(
                                provider_name,
                                &file,
                                &language,
                                &project,
                                cx,
                            );
                        })
                    })
                    .map(|popover_menu| {
                        let this = this.clone();
                        popover_menu.menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_edit_prediction_context_menu(provider, window, cx)
                            })
                            .ok()
                        })
                    })
                    .anchor(Anchor::BottomRight)
                    .with_handle(self.popover_menu_handle.clone());

                let is_refreshing = self
                    .edit_prediction_provider
                    .as_ref()
                    .is_some_and(|provider| provider.is_refreshing(cx));

                if is_refreshing {
                    popover_menu = popover_menu.trigger(
                        icon_button.with_animation(
                            "pulsating-label",
                            Animation::new(Duration::from_secs(2))
                                .repeat()
                                .with_easing(pulsating_between(0.2, 1.0)),
                            |icon_button, delta| icon_button.alpha(delta),
                        ),
                    );
                } else {
                    popover_menu = popover_menu.trigger(icon_button);
                }

                div().child(popover_menu.into_any_element())
            }

            EditPredictionProvider::None => div().hidden(),
        }
    }
}
