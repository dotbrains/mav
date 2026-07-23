use super::*;

impl EditPredictionButton {
    pub fn build_language_settings_menu(
        &self,
        mut menu: ContextMenu,
        window: &Window,
        cx: &mut App,
    ) -> ContextMenu {
        let fs = self.fs.clone();
        let line_height = window.line_height();

        menu = menu.header("Show Edit Predictions For");

        let language_state = self.language.as_ref().map(|language| {
            (
                language.clone(),
                LanguageSettings::resolve(None, Some(&language.name()), cx).show_edit_predictions,
            )
        });

        if let Some(editor_focus_handle) = self.editor_focus_handle.clone() {
            let entry = ContextMenuEntry::new("This Buffer")
                .toggleable(IconPosition::Start, self.editor_show_predictions)
                .action(Box::new(editor::actions::ToggleEditPrediction))
                .handler(move |window, cx| {
                    editor_focus_handle.dispatch_action(
                        &editor::actions::ToggleEditPrediction,
                        window,
                        cx,
                    );
                });

            match language_state.clone() {
                Some((language, false)) => {
                    menu = menu.item(entry.disabled(true).documentation_aside(
                        DocumentationSide::Left,
                        move |_cx| {
                            Label::new(format!(
                                "Edit predictions are disabled for {}",
                                language.name()
                            ))
                            .into_any_element()
                        },
                    ));
                }
                Some(_) | None => menu = menu.item(entry),
            }
        }

        if let Some((language, language_enabled)) = language_state {
            let fs = fs.clone();
            let language_name = language.name();

            menu = menu.toggleable_entry(
                language_name.clone(),
                language_enabled,
                IconPosition::Start,
                None,
                move |_, cx| {
                    telemetry::event!(
                        "Edit Prediction Setting Changed",
                        setting = "language",
                        language = language_name.to_string(),
                        enabled = !language_enabled,
                    );
                    toggle_show_edit_predictions_for_language(language.clone(), fs.clone(), cx)
                },
            );
        }

        let settings = AllLanguageSettings::get_global(cx);

        let globally_enabled = settings.show_edit_predictions(None, cx);
        let entry = ContextMenuEntry::new("All Files")
            .toggleable(IconPosition::Start, globally_enabled)
            .action(workspace::ToggleEditPrediction.boxed_clone())
            .handler(|window, cx| {
                window.dispatch_action(workspace::ToggleEditPrediction.boxed_clone(), cx)
            });
        menu = menu.item(entry);

        let provider = settings.edit_predictions.provider;
        let current_mode = settings.edit_predictions_mode();
        let subtle_mode = matches!(current_mode, EditPredictionsMode::Subtle);
        let eager_mode = matches!(current_mode, EditPredictionsMode::Eager);

        menu = menu
                .separator()
                .header("Display Modes")
                .item(
                    ContextMenuEntry::new("Eager")
                        .toggleable(IconPosition::Start, eager_mode)
                        .documentation_aside(DocumentationSide::Left, move |_| {
                            Label::new("Display predictions inline when there are no language server completions available.").into_any_element()
                        })
                        .handler({
                            let fs = fs.clone();
                            move |_, cx| {
                                telemetry::event!(
                                    "Edit Prediction Setting Changed",
                                    setting = "mode",
                                    value = "eager",
                                );
                                toggle_edit_prediction_mode(fs.clone(), EditPredictionsMode::Eager, cx)
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new("Subtle")
                        .toggleable(IconPosition::Start, subtle_mode)
                        .documentation_aside(DocumentationSide::Left, move |_| {
                            Label::new("Display predictions inline only when holding a modifier key (alt by default).").into_any_element()
                        })
                        .handler({
                            let fs = fs.clone();
                            move |_, cx| {
                                telemetry::event!(
                                    "Edit Prediction Setting Changed",
                                    setting = "mode",
                                    value = "subtle",
                                );
                                toggle_edit_prediction_mode(fs.clone(), EditPredictionsMode::Subtle, cx)
                            }
                        }),
                );

        menu = menu.separator().header("Privacy");

        if matches!(provider, EditPredictionProvider::Mav) {
            if let Some(provider) = &self.edit_prediction_provider {
                let data_collection = provider.data_collection_state(cx);

                if data_collection.is_supported() {
                    let provider = provider.clone();
                    let enabled = data_collection.is_enabled();
                    let is_open_source = data_collection.is_project_open_source();
                    let is_collecting = data_collection.is_enabled();
                    let (icon_name, icon_color) = if is_open_source && is_collecting {
                        (IconName::Check, Color::Success)
                    } else {
                        (IconName::Check, Color::Accent)
                    };

                    menu = menu.item(
                        ContextMenuEntry::new("Training Data Collection")
                            .toggleable(IconPosition::Start, data_collection.is_enabled())
                            .icon(icon_name)
                            .icon_color(icon_color)
                            .disabled(!provider.can_toggle_data_collection(cx))
                            .documentation_aside(DocumentationSide::Left, move |cx| {
                                let (msg, label_color, icon_name, icon_color) = match (is_open_source, is_collecting) {
                                    (true, true) => (
                                        "Project identified as open source, and you're sharing data.",
                                        Color::Default,
                                        IconName::Check,
                                        Color::Success,
                                    ),
                                    (true, false) => (
                                        "Project identified as open source, but you're not sharing data.",
                                        Color::Muted,
                                        IconName::Close,
                                        Color::Muted,
                                    ),
                                    (false, true) => (
                                        "Project not identified as open source. No data captured.",
                                        Color::Muted,
                                        IconName::Close,
                                        Color::Muted,
                                    ),
                                    (false, false) => (
                                        "Project not identified as open source, and setting turned off.",
                                        Color::Muted,
                                        IconName::Close,
                                        Color::Muted,
                                    ),
                                };
                                v_flex()
                                    .gap_2()
                                    .child(
                                        Label::new(indoc!{
                                            "Help us improve our open dataset model by sharing data from open source repositories. \
                                            Mav must detect a license file in your repo for this setting to take effect. \
                                            Files with sensitive data and secrets are excluded by default."
                                        })
                                    )
                                    .child(
                                        h_flex()
                                            .items_start()
                                            .pt_2()
                                            .pr_1()
                                            .flex_1()
                                            .gap_1p5()
                                            .border_t_1()
                                            .border_color(cx.theme().colors().border_variant)
                                            .child(h_flex().flex_shrink_0().h(line_height).child(Icon::new(icon_name).size(IconSize::XSmall).color(icon_color)))
                                            .child(div().child(msg).w_full().text_sm().text_color(label_color.color(cx)))
                                    )
                                    .into_any_element()
                            })
                            .handler(move |_, cx| {
                                provider.toggle_data_collection(cx);

                                if !enabled {
                                    telemetry::event!(
                                        "Data Collection Enabled",
                                        source = "Edit Prediction Status Menu"
                                    );
                                } else {
                                    telemetry::event!(
                                        "Data Collection Disabled",
                                        source = "Edit Prediction Status Menu"
                                    );
                                }
                            })
                    );

                    if is_collecting && !is_open_source {
                        menu = menu.item(
                            ContextMenuEntry::new("No data captured.")
                                .disabled(true)
                                .icon(IconName::Close)
                                .icon_color(Color::Error)
                                .icon_size(IconSize::Small),
                        );
                    }
                }
            }
        }

        menu = menu.item(
            ContextMenuEntry::new("Configure Excluded Files")
                .icon(IconName::Lock)
                .icon_color(Color::Muted)
                .documentation_aside(DocumentationSide::Left, |_| {
                    Label::new(indoc!{"
                        Open your settings to add sensitive paths for which Mav will never predict edits."}).into_any_element()
                })
                .handler(move |window, cx| {
                    telemetry::event!(
                        "Edit Prediction Menu Action",
                        action = "configure_excluded_files",
                    );
                    if let Some(workspace) = Workspace::for_window(window, cx) {
                        let workspace = workspace.downgrade();
                        window
                            .spawn(cx, async |cx| {
                                open_disabled_globs_setting_in_editor(
                                    workspace,
                                    cx,
                                ).await
                            })
                            .detach_and_log_err(cx);
                    }
                }),
        ).item(
            ContextMenuEntry::new("View Docs")
                .icon(IconName::FileGeneric)
                .icon_color(Color::Muted)
                .handler(move |_, cx| {
                    telemetry::event!(
                        "Edit Prediction Menu Action",
                        action = "view_docs",
                    );
                    cx.open_url(PRIVACY_DOCS);
                })
        );

        if !self.editor_enabled.unwrap_or(true) {
            let icons = self
                .edit_prediction_provider
                .as_ref()
                .map(|p| p.icons(cx))
                .unwrap_or_else(|| {
                    edit_prediction_types::EditPredictionIconSet::new(IconName::MavPredict)
                });
            menu = menu.item(
                ContextMenuEntry::new("This file is excluded.")
                    .disabled(true)
                    .icon(icons.disabled)
                    .icon_size(IconSize::Small),
            );
        }

        if let Some(editor_focus_handle) = self.editor_focus_handle.clone() {
            menu = menu
                .separator()
                .header("Actions")
                .entry(
                    "Predict Edit at Cursor",
                    Some(Box::new(ShowEditPrediction)),
                    {
                        let editor_focus_handle = editor_focus_handle.clone();
                        move |window, cx| {
                            telemetry::event!(
                                "Edit Prediction Menu Action",
                                action = "predict_at_cursor",
                            );
                            editor_focus_handle.dispatch_action(&ShowEditPrediction, window, cx);
                        }
                    },
                )
                .context(editor_focus_handle)
                .when(
                    cx.has_flag::<PredictEditsRatePredictionsFeatureFlag>(),
                    |this| this.action("Rate Predictions", RatePredictions.boxed_clone()),
                );
        }

        menu
    }
}
