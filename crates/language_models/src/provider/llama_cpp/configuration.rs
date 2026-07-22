use super::*;

pub(super) struct ConfigurationView {
    api_key_editor: Entity<InputField>,
    api_url_editor: Entity<InputField>,
    context_window_editor: Entity<InputField>,
    state: Entity<State>,
}

impl ConfigurationView {
    pub fn new(state: Entity<State>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let api_key_editor = cx.new(|cx| InputField::new(window, cx, "sk-...").label("API key"));

        let api_url_editor = cx.new(|cx| {
            let input = InputField::new(window, cx, LLAMA_CPP_API_URL).label("API URL");
            input.set_text(&LlamaCppLanguageModelProvider::api_url(cx), window, cx);
            input
        });

        let context_window_editor = cx.new(|cx| {
            let input = InputField::new(window, cx, "8192").label("Context Window");
            if let Some(context_window) = LlamaCppLanguageModelProvider::settings(cx).context_window
            {
                input.set_text(&context_window.to_string(), window, cx);
            }
            input
        });

        cx.observe(&state, |_, _, cx| {
            cx.notify();
        })
        .detach();

        Self {
            api_key_editor,
            api_url_editor,
            context_window_editor,
            state,
        }
    }

    fn retry_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_api_url = LlamaCppLanguageModelProvider::has_custom_url(cx);
        let has_api_key = self
            .state
            .read_with(cx, |state, _| state.api_key_state.has_key());
        if !has_api_url {
            self.save_api_url(cx);
        }
        if !has_api_key {
            self.save_api_key(&Default::default(), window, cx);
        }

        self.state.update(cx, |state, cx| {
            state.restart_fetch_models_task(cx);
        });
    }

    fn save_api_key(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let api_key = self.api_key_editor.read(cx).text(cx).trim().to_string();
        if api_key.is_empty() {
            return;
        }

        // A URL change can cause the editor to be shown again.
        self.api_key_editor
            .update(cx, |input, cx| input.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(Some(api_key), cx))
                .await
        })
        .detach_and_log_err(cx);
    }

    fn reset_api_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.api_key_editor
            .update(cx, |input, cx| input.set_text("", window, cx));

        let state = self.state.clone();
        cx.spawn_in(window, async move |_, cx| {
            state
                .update(cx, |state, cx| state.set_api_key(None, cx))
                .await
        })
        .detach_and_log_err(cx);

        cx.notify();
    }

    fn save_api_url(&self, cx: &mut Context<Self>) {
        let api_url = self.api_url_editor.read(cx).text(cx).trim().to_string();
        let current_url = LlamaCppLanguageModelProvider::api_url(cx);
        if !api_url.is_empty() && &api_url != &current_url {
            let fs = <dyn Fs>::global(cx);
            update_settings_file(fs, cx, move |settings, _| {
                settings
                    .language_models
                    .get_or_insert_default()
                    .llama_cpp
                    .get_or_insert_default()
                    .api_url = Some(api_url);
            });
        }
    }

    fn reset_api_url(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.api_url_editor
            .update(cx, |input, cx| input.set_text("", window, cx));
        let fs = <dyn Fs>::global(cx);
        update_settings_file(fs, cx, |settings, _cx| {
            if let Some(settings) = settings
                .language_models
                .as_mut()
                .and_then(|models| models.llama_cpp.as_mut())
            {
                settings.api_url = Some(LLAMA_CPP_API_URL.into());
            }
        });
        cx.notify();
    }

    fn save_context_window(&mut self, cx: &mut Context<Self>) {
        let context_window_str = self
            .context_window_editor
            .read(cx)
            .text(cx)
            .trim()
            .to_string();
        let current_context_window = LlamaCppLanguageModelProvider::settings(cx).context_window;

        if let Ok(context_window) = context_window_str.parse::<u64>() {
            if Some(context_window) != current_context_window {
                let fs = <dyn Fs>::global(cx);
                update_settings_file(fs, cx, move |settings, _| {
                    settings
                        .language_models
                        .get_or_insert_default()
                        .llama_cpp
                        .get_or_insert_default()
                        .context_window = Some(context_window);
                });
            }
        } else if context_window_str.is_empty() && current_context_window.is_some() {
            let fs = <dyn Fs>::global(cx);
            update_settings_file(fs, cx, move |settings, _| {
                settings
                    .language_models
                    .get_or_insert_default()
                    .llama_cpp
                    .get_or_insert_default()
                    .context_window = None;
            });
        }
    }

    fn reset_context_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_window_editor
            .update(cx, |input, cx| input.set_text("", window, cx));
        let fs = <dyn Fs>::global(cx);
        update_settings_file(fs, cx, |settings, _cx| {
            if let Some(settings) = settings
                .language_models
                .as_mut()
                .and_then(|models| models.llama_cpp.as_mut())
            {
                settings.context_window = None;
            }
        });
        cx.notify();
    }

    fn render_instructions(cx: &App) -> Div {
        v_flex()
            .gap_2()
            .child(Label::new(
                "Run open models locally with llama.cpp's built-in server, or connect to a \
                remote llama.cpp server.",
            ))
            .child(Label::new("To use a local llama.cpp server:"))
            .child(
                List::new()
                    .child(
                        ListBulletItem::new("")
                            .child(Label::new("Install llama.cpp from"))
                            .child(ButtonLink::new("llama.app", LLAMA_CPP_DOWNLOAD_URL)),
                    )
                    .child(
                        ListBulletItem::new("")
                            .child(Label::new("Start the server in router mode:"))
                            .child(Label::new("llama serve").inline_code(cx)),
                    )
                    .child(ListBulletItem::new(
                        "Click 'Connect' below to start using llama.cpp in Mav",
                    )),
            )
            .child(Label::new(
                "Alternatively, you can connect to a remote llama.cpp server by specifying its \
                URL and API key (set with --api-key, may not be required):",
            ))
    }

    fn render_api_key_editor(&self, cx: &Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let env_var_set = state.api_key_state.is_from_env_var();
        let configured_card_label = if env_var_set {
            format!("API key set in {API_KEY_ENV_VAR_NAME} environment variable.")
        } else {
            "API key configured".to_string()
        };

        if !state.api_key_state.has_key() {
            v_flex()
                .on_action(cx.listener(Self::save_api_key))
                .child(self.api_key_editor.clone())
                .child(
                    Label::new(format!(
                        "You can also set the {API_KEY_ENV_VAR_NAME} environment variable and restart Mav."
                    ))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                )
                .into_any_element()
        } else {
            ConfiguredApiCard::new(configured_card_label)
                .disabled(env_var_set)
                .on_click(cx.listener(|this, _, window, cx| this.reset_api_key(window, cx)))
                .when(env_var_set, |this| {
                    this.tooltip_label(format!(
                        "To reset your API key, unset the {API_KEY_ENV_VAR_NAME} environment variable."
                    ))
                })
                .into_any_element()
        }
    }

    fn render_context_window_editor(&self, cx: &Context<Self>) -> Div {
        let settings = LlamaCppLanguageModelProvider::settings(cx);
        let custom_context_window_set = settings.context_window.is_some();

        if custom_context_window_set {
            h_flex()
                .p_3()
                .justify_between()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().elevated_surface_background)
                .child(
                    h_flex()
                        .gap_2()
                        .child(Icon::new(IconName::Check).color(Color::Success))
                        .child(v_flex().gap_1().child(Label::new(format!(
                            "Context Window: {}",
                            settings.context_window.unwrap_or_default()
                        )))),
                )
                .child(
                    Button::new("reset-context-window", "Reset")
                        .label_size(LabelSize::Small)
                        .start_icon(Icon::new(IconName::Undo).size(IconSize::Small))
                        .layer(ElevationIndex::ModalSurface)
                        .on_click(
                            cx.listener(|this, _, window, cx| {
                                this.reset_context_window(window, cx)
                            }),
                        ),
                )
        } else {
            v_flex()
                .on_action(
                    cx.listener(|this, _: &menu::Confirm, _window, cx| {
                        this.save_context_window(cx)
                    }),
                )
                .child(self.context_window_editor.clone())
                .child(
                    Label::new("Default: discovered from the server")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
        }
    }

    fn render_api_url_editor(&self, cx: &Context<Self>) -> Div {
        let api_url = LlamaCppLanguageModelProvider::api_url(cx);
        let custom_api_url_set = api_url != LLAMA_CPP_API_URL;

        if custom_api_url_set {
            h_flex()
                .p_3()
                .justify_between()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().elevated_surface_background)
                .child(
                    h_flex()
                        .gap_2()
                        .child(Icon::new(IconName::Check).color(Color::Success))
                        .child(v_flex().gap_1().child(Label::new(api_url))),
                )
                .child(
                    Button::new("reset-api-url", "Reset API URL")
                        .label_size(LabelSize::Small)
                        .start_icon(Icon::new(IconName::Undo).size(IconSize::Small))
                        .layer(ElevationIndex::ModalSurface)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.reset_api_url(window, cx)),
                        ),
                )
        } else {
            v_flex()
                .on_action(cx.listener(|this, _: &menu::Confirm, _window, cx| {
                    this.save_api_url(cx);
                    cx.notify();
                }))
                .gap_2()
                .child(self.api_url_editor.clone())
        }
    }
}

impl Render for ConfigurationView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_authenticated = self.state.read(cx).is_authenticated();

        v_flex()
            .gap_2()
            .child(Self::render_instructions(cx))
            .child(self.render_api_url_editor(cx))
            .child(self.render_context_window_editor(cx))
            .child(self.render_api_key_editor(cx))
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .w_full()
                            .gap_2()
                            .map(|this| {
                                if is_authenticated {
                                    this.child(
                                        Button::new("llama-cpp-webui", "Open WebUI")
                                            .style(ButtonStyle::Subtle)
                                            .end_icon(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::XSmall)
                                                    .color(Color::Muted),
                                            )
                                            .on_click(move |_, _, cx| {
                                                let url =
                                                    LlamaCppLanguageModelProvider::api_url(cx);
                                                cx.open_url(&url);
                                            })
                                            .into_any_element(),
                                    )
                                    .child(
                                        Button::new("llama-cpp-site", "llama.cpp")
                                            .style(ButtonStyle::Subtle)
                                            .end_icon(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::XSmall)
                                                    .color(Color::Muted),
                                            )
                                            .on_click(move |_, _, cx| {
                                                cx.open_url(LLAMA_CPP_DOWNLOAD_URL)
                                            })
                                            .into_any_element(),
                                    )
                                } else {
                                    this.child(
                                        Button::new("download_llama_cpp_button", "Get llama.cpp")
                                            .style(ButtonStyle::Subtle)
                                            .end_icon(
                                                Icon::new(IconName::ArrowUpRight)
                                                    .size(IconSize::XSmall)
                                                    .color(Color::Muted),
                                            )
                                            .on_click(move |_, _, cx| {
                                                cx.open_url(LLAMA_CPP_DOWNLOAD_URL)
                                            })
                                            .into_any_element(),
                                    )
                                }
                            })
                            .child(
                                Button::new("view-models", "Browse GGUF Models")
                                    .style(ButtonStyle::Subtle)
                                    .end_icon(
                                        Icon::new(IconName::ArrowUpRight)
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .on_click(move |_, _, cx| cx.open_url(LLAMA_CPP_MODELS_URL)),
                            ),
                    )
                    .map(|this| {
                        if is_authenticated {
                            this.child(
                                ButtonLike::new("connected")
                                    .disabled(true)
                                    .cursor_style(CursorStyle::Arrow)
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .child(Icon::new(IconName::Check).color(Color::Success))
                                            .child(Label::new("Connected"))
                                            .into_any_element(),
                                    )
                                    .child(
                                        IconButton::new("refresh-models", IconName::RotateCcw)
                                            .tooltip(Tooltip::text("Refresh Models"))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.state.update(cx, |state, _| {
                                                    state.fetched_models.clear();
                                                });
                                                this.retry_connection(window, cx);
                                            })),
                                    ),
                            )
                        } else {
                            this.child(
                                Button::new("retry_llama_cpp_models", "Connect")
                                    .start_icon(
                                        Icon::new(IconName::PlayOutlined).size(IconSize::XSmall),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.retry_connection(window, cx)
                                    })),
                            )
                        }
                    }),
            )
    }
}
