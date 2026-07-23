use super::*;

impl AgentRegistryPage {
    fn render_search(&self, cx: &mut Context<Self>) -> Div {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("BufferSearchBar");

        h_flex()
            .key_context(key_context)
            .h_8()
            .min_w(rems_from_px(384.))
            .flex_1()
            .pl_1p5()
            .pr_2()
            .gap_2()
            .border_1()
            .border_color(cx.theme().colors().border)
            .rounded_md()
            .child(Icon::new(IconName::MagnifyingGlass).color(Color::Muted))
            .child(self.render_text_input(&self.query_editor, cx))
    }

    fn render_text_input(
        &self,
        editor: &Entity<Editor>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: if editor.read(cx).read_only(cx) {
                cx.theme().colors().text_disabled
            } else {
                cx.theme().colors().text
            },
            font_family: settings.ui_font.family.clone(),
            font_features: settings.ui_font.features.clone(),
            font_fallbacks: settings.ui_font.fallbacks.clone(),
            font_size: rems(0.875).into(),
            font_weight: settings.ui_font.weight,
            line_height: relative(1.3),
            ..Default::default()
        };

        EditorElement::new(
            editor,
            EditorStyle {
                background: cx.theme().colors().editor_background,
                local_player: cx.theme().players().local(),
                text: text_style,
                ..Default::default()
            },
        )
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_search = self.search_query(cx).is_some();
        let registry_store = self.registry_store.read(cx);
        let is_fetching = registry_store.is_fetching();
        let fetch_error = registry_store.fetch_error();

        let message = if is_fetching {
            "Loading registry..."
        } else if fetch_error.is_some() {
            "Failed to load the agent registry. Please check your connection and try again."
        } else {
            match self.filter {
                RegistryFilter::All => {
                    if has_search {
                        "No agents match your search."
                    } else {
                        "No agents available."
                    }
                }
                RegistryFilter::Installed => {
                    if has_search {
                        "No installed agents match your search."
                    } else {
                        "No installed agents."
                    }
                }
                RegistryFilter::NotInstalled => {
                    if has_search {
                        "No uninstalled agents match your search."
                    } else {
                        "No uninstalled agents."
                    }
                }
            }
        };

        h_flex()
            .py_4()
            .min_w_0()
            .w_full()
            .gap_1p5()
            .items_start()
            .when(fetch_error.is_some(), |this| {
                this.child(
                    Icon::new(IconName::Warning)
                        .size(IconSize::Small)
                        .color(Color::Warning),
                )
            })
            .child(
                v_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1()
                    .child(Label::new(message))
                    .when_some(fetch_error.clone(), |this, fetch_error| {
                        this.child(
                            Label::new(fetch_error)
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
            .when_some(fetch_error, |this, _| {
                let registry_store = self.registry_store.clone();
                this.child(
                    Button::new("retry-agent-registry", "Retry")
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Compact)
                        .on_click(move |_, _, cx| {
                            registry_store.update(cx, |store, cx| store.refresh(cx));
                        }),
                )
            })
    }

    fn render_agents(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AgentRegistryCard> {
        range
            .map(|index| {
                let Some(agent_index) = self.filtered_registry_indices.get(index).copied() else {
                    return self.render_missing_agent();
                };
                let Some(agent) = self.registry_agents.get(agent_index) else {
                    return self.render_missing_agent();
                };
                self.render_registry_agent(agent, cx)
            })
            .collect()
    }

    fn render_missing_agent(&self) -> AgentRegistryCard {
        AgentRegistryCard::new().child(
            Label::new("Missing registry entry.")
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
    }

    fn render_registry_agent(
        &self,
        agent: &RegistryAgent,
        cx: &mut Context<Self>,
    ) -> AgentRegistryCard {
        let install_status = self.install_status(agent.id().as_ref());
        let supports_current_platform = agent.supports_current_platform();

        let icon = match agent.icon_path() {
            Some(icon_path) => Icon::from_external_svg(icon_path.clone()),
            None => Icon::new(IconName::Sparkle),
        }
        .size(IconSize::Medium)
        .color(Color::Muted);

        let install_button =
            self.install_button(agent, install_status, supports_current_platform, cx);

        let repository_button = agent.repository().map(|repository| {
            let repository_for_tooltip = repository.clone();
            let repository_for_click = repository.to_string();

            IconButton::new(
                SharedString::from(format!("agent-repo-{}", agent.id())),
                IconName::Github,
            )
            .icon_size(IconSize::Small)
            .tooltip(move |_, cx| {
                Tooltip::with_meta(
                    "Visit Agent Repository",
                    None,
                    repository_for_tooltip.clone(),
                    cx,
                )
            })
            .on_click(move |_, _, cx| {
                cx.open_url(&repository_for_click);
            })
        });

        let website_button = agent.website().map(|website| {
            let website = website.clone();
            let website_for_click = website.clone();
            IconButton::new(
                SharedString::from(format!("agent-website-{}", agent.id())),
                IconName::Link,
            )
            .icon_size(IconSize::Small)
            .tooltip(move |_, cx| {
                Tooltip::with_meta("Visit Agent Website", None, website.clone(), cx)
            })
            .on_click(move |_, _, cx| {
                cx.open_url(&website_for_click);
            })
        });

        AgentRegistryCard::new()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(icon)
                            .child(Headline::new(agent.name().clone()).size(HeadlineSize::Small))
                            .child(Label::new(format!("v{}", agent.version())).color(Color::Muted))
                            .when(!supports_current_platform, |this| {
                                this.child(
                                    Label::new("Not supported on this platform")
                                        .size(LabelSize::Small)
                                        .color(Color::Warning),
                                )
                            }),
                    )
                    .child(install_button),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_between()
                    .child(
                        Label::new(agent.description().clone())
                            .size(LabelSize::Small)
                            .truncate(),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Label::new(format!("ID: {}", agent.id()))
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                            .when_some(repository_button, |this, button| this.child(button))
                            .when_some(website_button, |this, button| this.child(button)),
                    ),
            )
    }

    fn install_button(
        &self,
        agent: &RegistryAgent,
        install_status: RegistryInstallStatus,
        supports_current_platform: bool,
        cx: &mut Context<Self>,
    ) -> Button {
        let button_id = SharedString::from(format!("install-agent-{}", agent.id()));

        if !supports_current_platform {
            return Button::new(button_id, "Unavailable")
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true);
        }

        match install_status {
            RegistryInstallStatus::NotInstalled => {
                let fs = <dyn Fs>::global(cx);
                let agent_id = agent.id().to_string();
                Button::new(button_id, "Install")
                    .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                    .start_icon(
                        Icon::new(IconName::Download)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .on_click(move |_, _, cx| {
                        let agent_id = agent_id.clone();
                        update_settings_file(fs.clone(), cx, move |settings, _| {
                            let agent_servers = settings.agent_servers.get_or_insert_default();
                            agent_servers.entry(agent_id).or_insert_with(|| {
                                settings::CustomAgentServerSettings::Registry {
                                    default_mode: None,
                                    env: Default::default(),
                                    default_config_options: HashMap::default(),
                                    favorite_config_option_values: HashMap::default(),
                                }
                            });
                        });
                    })
            }
            RegistryInstallStatus::InstalledRegistry => {
                let fs = <dyn Fs>::global(cx);
                let agent_id = agent.id().to_string();
                Button::new(button_id, "Remove")
                    .style(ButtonStyle::OutlinedGhost)
                    .on_click(move |_, _, cx| {
                        let agent_id = agent_id.clone();
                        update_settings_file(fs.clone(), cx, move |settings, _| {
                            let Some(agent_servers) = settings.agent_servers.as_mut() else {
                                return;
                            };
                            if let Some(entry) = agent_servers.get(agent_id.as_str())
                                && matches!(
                                    entry,
                                    settings::CustomAgentServerSettings::Registry { .. }
                                )
                            {
                                agent_servers.remove(agent_id.as_str());
                            }
                        });
                    })
            }
            RegistryInstallStatus::InstalledCustom => Button::new(button_id, "Installed")
                .style(ButtonStyle::OutlinedGhost)
                .disabled(true),
        }
    }
}

impl Render for AgentRegistryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().colors().editor_background)
            .child(
                v_flex()
                    .p_4()
                    .gap_4()
                    .border_b_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1p5()
                            .justify_between()
                            .child(Headline::new("ACP Registry").size(HeadlineSize::Large))
                            .child(
                                Button::new("learn-more", "Learn More")
                                    .style(ButtonStyle::Outlined)
                                    .size(ButtonSize::Medium)
                                    .end_icon(
                                        Icon::new(IconName::ArrowUpRight)
                                            .size(IconSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .on_click(move |_, _, cx| {
                                        cx.open_url(&mav_urls::acp_registry_blog(cx))
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .flex_wrap()
                            .gap_2()
                            .child(self.render_search(cx))
                            .child(
                                div().child(
                                    ToggleButtonGroup::single_row(
                                        "registry-filter-buttons",
                                        [
                                            ToggleButtonSimple::new(
                                                "All",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::All;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Installed",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::Installed;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                            ToggleButtonSimple::new(
                                                "Not Installed",
                                                cx.listener(|this, _event, _, cx| {
                                                    this.filter = RegistryFilter::NotInstalled;
                                                    this.filter_registry_agents(cx);
                                                    this.scroll_to_top(cx);
                                                }),
                                            ),
                                        ],
                                    )
                                    .style(ToggleButtonGroupStyle::Outlined)
                                    .size(ToggleButtonGroupSize::Custom(rems_from_px(30.)))
                                    .label_size(LabelSize::Default)
                                    .auto_width()
                                    .selected_index(match self.filter {
                                        RegistryFilter::All => 0,
                                        RegistryFilter::Installed => 1,
                                        RegistryFilter::NotInstalled => 2,
                                    })
                                    .into_any_element(),
                                ),
                            ),
                    ),
            )
            .child(v_flex().px_4().size_full().overflow_y_hidden().map(|this| {
                let count = self.filtered_registry_indices.len();
                if count == 0 {
                    this.child(self.render_empty_state(cx)).into_any_element()
                } else {
                    let scroll_handle = &self.list;
                    this.child(
                        uniform_list("registry-entries", count, cx.processor(Self::render_agents))
                            .flex_grow_1()
                            .pb_4()
                            .track_scroll(scroll_handle),
                    )
                    .vertical_scrollbar_for(scroll_handle, window, cx)
                    .into_any_element()
                }
            }))
    }
}
