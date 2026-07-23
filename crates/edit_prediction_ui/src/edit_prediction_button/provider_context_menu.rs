use super::*;

impl EditPredictionButton {
    pub(crate) fn build_copilot_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let all_language_settings = all_language_settings(None, cx);
        let next_edit_suggestions = all_language_settings
            .edit_predictions
            .copilot
            .enable_next_edit_suggestions
            .unwrap_or(true);
        let copilot_config = copilot_chat::CopilotChatConfiguration {
            enterprise_uri: all_language_settings
                .edit_predictions
                .copilot
                .enterprise_uri
                .clone(),
        };
        let settings_url = copilot_settings_url(copilot_config.enterprise_uri.as_deref());

        ContextMenu::build(window, cx, |menu, window, cx| {
            let menu = self.build_language_settings_menu(menu, window, cx);
            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Copilot, cx);

            let menu = self.add_configure_providers_item(menu);
            let menu = menu
                .separator()
                .item(
                    ContextMenuEntry::new("Copilot: Next Edit Suggestions")
                        .toggleable(IconPosition::Start, next_edit_suggestions)
                        .handler({
                            let fs = self.fs.clone();
                            move |_, cx| {
                                update_settings_file(fs.clone(), cx, move |settings, _| {
                                    settings
                                        .project
                                        .all_languages
                                        .edit_predictions
                                        .get_or_insert_default()
                                        .copilot
                                        .get_or_insert_default()
                                        .enable_next_edit_suggestions =
                                        Some(!next_edit_suggestions);
                                });
                            }
                        }),
                )
                .separator()
                .link(
                    "Go to Copilot Settings",
                    OpenBrowser { url: settings_url }.boxed_clone(),
                )
                .action("Sign Out", copilot::SignOut.boxed_clone());
            menu
        })
    }

    pub(crate) fn build_codestral_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |menu, window, cx| {
            let menu = self.build_language_settings_menu(menu, window, cx);
            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Codestral, cx);

            let menu = self.add_configure_providers_item(menu);
            menu
        })
    }

    pub(crate) fn build_edit_prediction_context_menu(
        &self,
        provider: EditPredictionProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |mut menu, window, cx| {
            let user = self.user_store.read(cx).current_user();

            let needs_sign_in = user.is_none()
                && matches!(
                    provider,
                    EditPredictionProvider::None | EditPredictionProvider::Mav
                );

            if needs_sign_in {
                menu = menu
                    .custom_row(move |_window, cx| {
                        let description = indoc! {
                            "You get 2,000 accepted suggestions at every keystroke for free, \
                            powered by Zeta, our open-source, open-data model"
                        };

                        v_flex()
                            .max_w_64()
                            .h(rems_from_px(148.))
                            .child(render_zeta_tab_animation(cx))
                            .child(Label::new("Edit Prediction"))
                            .child(
                                Label::new(description)
                                    .color(Color::Muted)
                                    .size(LabelSize::Small),
                            )
                            .into_any_element()
                    })
                    .separator()
                    .entry("Sign In & Start Using", None, |window, cx| {
                        telemetry::event!(
                            "Edit Prediction Menu Action",
                            action = "sign_in",
                            provider = "mav",
                        );
                        let client = Client::global(cx);
                        window
                            .spawn(cx, async move |cx| {
                                client
                                    .sign_in_with_optional_connect(true, &cx)
                                    .await
                                    .log_err();
                            })
                            .detach();
                    })
                    .link_with_handler(
                        "Learn More",
                        OpenBrowser {
                            url: mav_urls::edit_prediction_docs(cx).into(),
                        }
                        .boxed_clone(),
                        |_window, _cx| {
                            telemetry::event!(
                                "Edit Prediction Menu Action",
                                action = "view_docs",
                                source = "upsell",
                            );
                        },
                    )
                    .separator();
            } else {
                let mercury_payment_required = matches!(provider, EditPredictionProvider::Mercury)
                    && edit_prediction::EditPredictionStore::try_global(cx).is_some_and(
                        |ep_store| ep_store.read(cx).mercury_has_payment_required_error(),
                    );

                if mercury_payment_required {
                    menu = menu
                        .header("Mercury")
                        .item(ContextMenuEntry::new("Free tier limit reached").disabled(true))
                        .item(
                            ContextMenuEntry::new(
                                "Upgrade to a paid plan to continue using the service",
                            )
                            .disabled(true),
                        )
                        .separator();
                }

                if let Some(usage) = self
                    .edit_prediction_provider
                    .as_ref()
                    .and_then(|provider| provider.usage(cx))
                {
                    menu = menu.header("Usage");
                    menu = menu
                        .custom_entry(
                            move |_window, cx| {
                                let used_percentage = match usage.limit {
                                    UsageLimit::Limited(limit) => {
                                        Some((usage.amount as f32 / limit as f32) * 100.)
                                    }
                                    UsageLimit::Unlimited => None,
                                };

                                h_flex()
                                    .flex_1()
                                    .gap_1p5()
                                    .children(used_percentage.map(|percent| {
                                        ProgressBar::new("usage", percent, 100., cx)
                                    }))
                                    .child(
                                        Label::new(match usage.limit {
                                            UsageLimit::Limited(limit) => {
                                                format!("{} / {limit}", usage.amount)
                                            }
                                            UsageLimit::Unlimited => {
                                                format!("{} / ∞", usage.amount)
                                            }
                                        })
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                    )
                                    .into_any_element()
                            },
                            move |_, cx| cx.open_url(&mav_urls::account_url(cx)),
                        )
                        .when(usage.over_limit(), |menu| -> ContextMenu {
                            menu.entry("Subscribe to increase your limit", None, |_window, cx| {
                                telemetry::event!(
                                    "Edit Prediction Menu Action",
                                    action = "upsell_clicked",
                                    reason = "usage_limit",
                                );
                                cx.open_url(&mav_urls::account_url(cx))
                            })
                        })
                        .separator();
                } else if self.user_store.read(cx).account_too_young() {
                    menu = menu
                        .custom_entry(
                            |_window, _cx| {
                                Label::new("Your GitHub account is less than 30 days old.")
                                    .size(LabelSize::Small)
                                    .color(Color::Warning)
                                    .into_any_element()
                            },
                            |_window, cx| cx.open_url(&mav_urls::account_url(cx)),
                        )
                        .entry("Upgrade to Mav Pro or contact us.", None, |_window, cx| {
                            telemetry::event!(
                                "Edit Prediction Menu Action",
                                action = "upsell_clicked",
                                reason = "account_age",
                            );
                            cx.open_url(&mav_urls::account_url(cx))
                        })
                        .separator();
                } else if self.user_store.read(cx).has_overdue_invoices() {
                    menu = menu
                        .custom_entry(
                            |_window, _cx| {
                                Label::new("You have an outstanding invoice")
                                    .size(LabelSize::Small)
                                    .color(Color::Warning)
                                    .into_any_element()
                            },
                            |_window, cx| {
                                cx.open_url(&mav_urls::account_url(cx))
                            },
                        )
                        .entry(
                            "Check your payment status or contact us at billing-support@mav.dev to continue using this feature.",
                            None,
                            |_window, cx| {
                                cx.open_url(&mav_urls::account_url(cx))
                            },
                        )
                        .separator();
                }
            }

            if !needs_sign_in {
                menu = self.build_language_settings_menu(menu, window, cx);
            }
            menu = self.add_provider_switching_section(menu, provider, cx);

            if cx.is_staff() {
                if let Some(store) = EditPredictionStore::try_global(cx) {
                    store.update(cx, |store, cx| {
                        store.refresh_available_experiments(cx);
                    });
                    let store = store.read(cx);
                    let experiments = store.available_experiments().to_vec();
                    let preferred = store.preferred_experiment().map(|s| s.to_owned());
                    let active = store.active_experiment().map(|s| s.to_owned());

                    let preferred_for_submenu = preferred.clone();
                    menu = menu
                        .separator()
                        .submenu("Experiment", move |menu, _window, _cx| {
                            let mut menu = menu.toggleable_entry(
                                "Default",
                                preferred_for_submenu.is_none(),
                                IconPosition::Start,
                                None,
                                {
                                    move |_window, cx| {
                                        if let Some(store) = EditPredictionStore::try_global(cx) {
                                            store.update(cx, |store, _cx| {
                                                store.set_preferred_experiment(None);
                                            });
                                        }
                                    }
                                },
                            );
                            for experiment in &experiments {
                                let is_selected = active.as_deref() == Some(experiment.as_str())
                                    || preferred.as_deref() == Some(experiment.as_str());
                                let experiment_name = experiment.clone();
                                menu = menu.toggleable_entry(
                                    experiment.clone(),
                                    is_selected,
                                    IconPosition::Start,
                                    None,
                                    move |_window, cx| {
                                        if let Some(store) = EditPredictionStore::try_global(cx) {
                                            store.update(cx, |store, _cx| {
                                                store.set_preferred_experiment(Some(
                                                    experiment_name.clone(),
                                                ));
                                            });
                                        }
                                    },
                                );
                            }
                            menu
                        });
                }
            }

            let menu = self.add_configure_providers_item(menu);
            menu
        })
    }
}
