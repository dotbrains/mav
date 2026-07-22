use super::*;

impl ExtensionsPage {
    pub(super) fn update_settings(
        &mut self,
        selection: &ToggleState,

        cx: &mut Context<Self>,
        callback: impl 'static + Send + Fn(&mut SettingsContent, bool),
    ) {
        if let Some(workspace) = self.workspace.upgrade() {
            let fs = workspace.read(cx).app_state().fs.clone();
            let selection = *selection;
            settings::update_settings_file(fs, cx, move |settings, _| {
                let value = match selection {
                    ToggleState::Unselected => false,
                    ToggleState::Selected => true,
                    _ => return,
                };

                callback(settings, value)
            });
        }
    }

    pub(super) fn refresh_feature_upsells(&mut self, cx: &mut Context<Self>) {
        let Some(search) = self.search_query(cx) else {
            self.upsells.clear();
            return;
        };

        if let Some(id) = search.strip_prefix("id:") {
            self.upsells.clear();

            let upsell = match id.to_lowercase().as_str() {
                "ruff" => Some(Feature::ExtensionRuff),
                "basedpyright" => Some(Feature::ExtensionBasedpyright),
                "ty" => Some(Feature::ExtensionTy),
                _ => None,
            };

            if let Some(upsell) = upsell {
                self.upsells.insert(upsell);
            }

            return;
        }

        let search = search.to_lowercase();
        let search_terms = search
            .split_whitespace()
            .map(|term| term.trim())
            .collect::<Vec<_>>();

        for (feature, keywords) in keywords_by_feature() {
            if keywords
                .iter()
                .any(|keyword| search_terms.contains(keyword))
            {
                self.upsells.insert(*feature);
            } else {
                self.upsells.remove(feature);
            }
        }
    }

    pub(super) fn render_feature_upsell_banner(
        &self,
        label: SharedString,
        docs_url: SharedString,
        vim: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let docs_url_button = Button::new("open_docs", "View Documentation")
            .end_icon(Icon::new(IconName::ArrowUpRight).size(IconSize::Small))
            .on_click({
                move |_event, _window, cx| {
                    telemetry::event!(
                        "Documentation Viewed",
                        source = "Feature Upsell",
                        url = docs_url,
                    );
                    cx.open_url(&docs_url)
                }
            });

        div()
            .pt_4()
            .px_4()
            .child(
                Banner::new()
                    .severity(Severity::Success)
                    .child(Label::new(label).mt_0p5())
                    .map(|this| {
                        if vim {
                            this.action_slot(
                                h_flex()
                                    .gap_1()
                                    .child(docs_url_button)
                                    .child(Divider::vertical().color(ui::DividerColor::Border))
                                    .child(
                                        h_flex()
                                            .pl_1()
                                            .gap_1()
                                            .child(Label::new("Enable Vim mode"))
                                            .child(
                                                Switch::new(
                                                    "enable-vim",
                                                    if VimModeSetting::get_global(cx).0 {
                                                        ui::ToggleState::Selected
                                                    } else {
                                                        ui::ToggleState::Unselected
                                                    },
                                                )
                                                .on_click(cx.listener(
                                                    move |this, selection, _, cx| {
                                                        telemetry::event!(
                                                            "Vim Mode Toggled",
                                                            source = "Feature Upsell"
                                                        );
                                                        this.update_settings(
                                                            selection,
                                                            cx,
                                                            |setting, value| {
                                                                setting.vim_mode = Some(value)
                                                            },
                                                        );
                                                    },
                                                )),
                                            ),
                                    ),
                            )
                        } else {
                            this.action_slot(docs_url_button)
                        }
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_feature_upsells(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = v_flex();

        for feature in &self.upsells {
            let banner = match feature {
                Feature::AgentClaude => self.render_feature_upsell_banner(
                    "Claude Agent support is built-in to Mav!".into(),
                    "https://mav.dev/docs/ai/external-agents#claude-agent".into(),
                    false,
                    cx,
                ),
                Feature::AgentCodex => self.render_feature_upsell_banner(
                    "Codex CLI support is built-in to Mav!".into(),
                    "https://mav.dev/docs/ai/external-agents#codex-cli".into(),
                    false,
                    cx,
                ),
                Feature::AgentGemini => self.render_feature_upsell_banner(
                    "Gemini CLI support is built-in to Mav!".into(),
                    "https://mav.dev/docs/ai/external-agents#gemini-cli".into(),
                    false,
                    cx,
                ),
                Feature::ExtensionBasedpyright => self.render_feature_upsell_banner(
                    "Basedpyright (Python language server) support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/python#basedpyright".into(),
                    false,
                    cx,
                ),
                Feature::ExtensionRuff => self.render_feature_upsell_banner(
                    "Ruff (linter for Python) support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/python#code-formatting--linting".into(),
                    false,
                    cx,
                ),
                Feature::ExtensionTailwind => self.render_feature_upsell_banner(
                    "Tailwind CSS support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/tailwindcss".into(),
                    false,
                    cx,
                ),
                Feature::ExtensionTy => self.render_feature_upsell_banner(
                    "Ty (Python language server) support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/python".into(),
                    false,
                    cx,
                ),
                Feature::Git => self.render_feature_upsell_banner(
                    "Mav comes with basic Git support—more features are coming in the future."
                        .into(),
                    "https://mav.dev/docs/git".into(),
                    false,
                    cx,
                ),
                Feature::LanguageBash => self.render_feature_upsell_banner(
                    "Shell support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/bash".into(),
                    false,
                    cx,
                ),
                Feature::LanguageC => self.render_feature_upsell_banner(
                    "C support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/c".into(),
                    false,
                    cx,
                ),
                Feature::LanguageCpp => self.render_feature_upsell_banner(
                    "C++ support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/cpp".into(),
                    false,
                    cx,
                ),
                Feature::LanguageGo => self.render_feature_upsell_banner(
                    "Go support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/go".into(),
                    false,
                    cx,
                ),
                Feature::LanguagePython => self.render_feature_upsell_banner(
                    "Python support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/python".into(),
                    false,
                    cx,
                ),
                Feature::LanguageReact => self.render_feature_upsell_banner(
                    "React support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/typescript".into(),
                    false,
                    cx,
                ),
                Feature::LanguageRust => self.render_feature_upsell_banner(
                    "Rust support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/rust".into(),
                    false,
                    cx,
                ),
                Feature::LanguageTypescript => self.render_feature_upsell_banner(
                    "Typescript support is built-in to Mav!".into(),
                    "https://mav.dev/docs/languages/typescript".into(),
                    false,
                    cx,
                ),
                Feature::OpenIn => self.render_feature_upsell_banner(
                    "Mav supports linking to a source line on GitHub and others.".into(),
                    "https://mav.dev/docs/git#git-integrations".into(),
                    false,
                    cx,
                ),
                Feature::Vim => self.render_feature_upsell_banner(
                    "Vim support is built-in to Mav!".into(),
                    "https://mav.dev/docs/vim".into(),
                    true,
                    cx,
                ),
            };
            container = container.child(banner);
        }

        container
    }
}
