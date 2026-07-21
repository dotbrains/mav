use super::*;

impl ConversationView {
    pub(super) fn render_loading_draft(
        &self,
        draft: &LoadingDraft,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor_bg_color = cx.theme().colors().editor_background;
        let max_content_width = AgentSettings::get_global(cx).max_content_width;
        let loading_text = self
            .loading_status
            .clone()
            .unwrap_or_else(|| "Agent is loading...".into());

        v_flex()
            .bg(editor_bg_color)
            .flex_1()
            .size_full()
            .child(div().w_full().min_h_0().flex_1())
            .child(
                h_flex()
                    .py_2()
                    .justify_center()
                    .flex_none()
                    .border_t_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        v_flex()
                            .when_some(max_content_width, |this, max_w| this.flex_basis(max_w))
                            .when(max_content_width.is_none(), |this| this.w_full())
                            .px_2()
                            .flex_shrink_1()
                            .flex_grow_0()
                            .justify_between()
                            .gap_2()
                            .child(
                                v_flex()
                                    .relative()
                                    .w_full()
                                    .min_h_0()
                                    .pt_1()
                                    .pr_2p5()
                                    .child(draft.message_editor.clone()),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .flex_none()
                                    .justify_between()
                                    .child(
                                        Label::new(loading_text)
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        h_flex()
                                            .gap_1()
                                            .child(
                                                self.render_loading_draft_agent_selector(draft, cx),
                                            )
                                            .child(
                                                IconButton::new("send-message", IconName::Send)
                                                    .style(ButtonStyle::Filled)
                                                    .disabled(true)
                                                    .icon_color(Color::Muted)
                                                    .tooltip(Tooltip::text(
                                                        "Agent is still loading",
                                                    )),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_loading_draft_agent_selector(
        &self,
        draft: &LoadingDraft,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected_agent = self.connection_key.clone();
        let agent_server_store = self.project.read(cx).agent_server_store().clone();
        let is_via_collab = self.project.read(cx).is_via_collab();

        let (selected_agent_custom_icon, selected_agent_label) = {
            let store = agent_server_store.read(cx);
            let registry_store = AgentRegistryStore::try_global(cx);
            let registry_store_ref = registry_store.as_ref().map(|store| store.read(cx));

            if let Agent::Custom { id } = &selected_agent {
                (
                    store.agent_icon(id).or_else(|| {
                        registry_store_ref
                            .as_ref()
                            .and_then(|store| store.agent(id))
                            .and_then(|agent| agent.icon_path().cloned())
                    }),
                    store
                        .agent_display_name(id)
                        .or_else(|| {
                            registry_store_ref
                                .as_ref()
                                .and_then(|store| store.agent(id))
                                .map(|agent| agent.name().clone())
                        })
                        .unwrap_or_else(|| selected_agent.label()),
                )
            } else {
                (None, selected_agent.label())
            }
        };

        let has_custom_icon = selected_agent_custom_icon.is_some();
        let selected_agent_builtin_icon = selected_agent.icon();
        let this = cx.weak_entity();
        let (color, icon) = if draft.agent_selector_menu_handle.is_deployed() {
            (Color::Accent, IconName::ChevronUp)
        } else {
            (Color::Muted, IconName::ChevronDown)
        };

        PopoverMenu::new("loading-draft-agent-selector")
            .trigger_with_tooltip(
                Button::new("loading-draft-agent-selector-trigger", selected_agent_label)
                    .label_size(LabelSize::Small)
                    .color(color)
                    .when_some(selected_agent_custom_icon, |this, icon_path| {
                        this.start_icon(
                            Icon::from_external_svg(icon_path)
                                .color(color)
                                .size(IconSize::XSmall),
                        )
                    })
                    .when(!has_custom_icon, |this| {
                        this.when_some(selected_agent_builtin_icon, |this, icon| {
                            this.start_icon(Icon::new(icon).color(color).size(IconSize::XSmall))
                        })
                    })
                    .end_icon(Icon::new(icon).color(Color::Muted).size(IconSize::XSmall)),
                Tooltip::text("Select Agent"),
            )
            .anchor(gpui::Anchor::BottomRight)
            .with_handle(draft.agent_selector_menu_handle.clone())
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            })
            .menu(move |window, cx| {
                struct AgentMenuItem {
                    id: AgentId,
                    display_name: SharedString,
                    icon_path: Option<SharedString>,
                }

                let agent_items = {
                    let agent_server_store = agent_server_store.read(cx);
                    let registry_store = AgentRegistryStore::try_global(cx);
                    let registry_store_ref = registry_store.as_ref().map(|store| store.read(cx));

                    agent_server_store
                        .external_agents()
                        .map(|agent_id| {
                            let display_name = agent_server_store
                                .agent_display_name(agent_id)
                                .or_else(|| {
                                    registry_store_ref
                                        .as_ref()
                                        .and_then(|store| store.agent(agent_id))
                                        .map(|agent| agent.name().clone())
                                })
                                .unwrap_or_else(|| agent_id.0.clone());
                            let icon_path = agent_server_store.agent_icon(agent_id).or_else(|| {
                                registry_store_ref
                                    .as_ref()
                                    .and_then(|store| store.agent(agent_id))
                                    .and_then(|agent| agent.icon_path().cloned())
                            });
                            AgentMenuItem {
                                id: agent_id.clone(),
                                display_name,
                                icon_path,
                            }
                        })
                        .sorted_unstable_by_key(|item| item.display_name.to_lowercase())
                        .collect::<Vec<_>>()
                };

                Some(ContextMenu::build(window, cx, |mut menu, _window, _cx| {
                    menu = menu.item(
                        ContextMenuEntry::new("Mav Agent")
                            .icon(IconName::MavAgent)
                            .icon_color(Color::Muted)
                            .handler({
                                let this = this.clone();
                                move |window, cx| {
                                    this.update(cx, |this, cx| {
                                        let agent = Agent::NativeAgent;
                                        if let Some((server, thread_store)) =
                                            this.server_for_agent(&agent, cx)
                                        {
                                            this.switch_draft_agent(
                                                agent,
                                                server,
                                                thread_store,
                                                window,
                                                cx,
                                            );
                                        }
                                    })
                                    .ok();
                                }
                            }),
                    );

                    if !agent_items.is_empty() {
                        menu = menu.separator().header("External Agents");
                    }
                    for AgentMenuItem {
                        id,
                        display_name,
                        icon_path,
                    } in agent_items
                    {
                        let mut entry = ContextMenuEntry::new(display_name);

                        if let Some(icon_path) = icon_path {
                            entry = entry.custom_icon_svg(icon_path);
                        } else {
                            entry = entry.icon(IconName::Sparkle);
                        }

                        menu = menu.item(
                            entry
                                .icon_color(Color::Muted)
                                .disabled(is_via_collab)
                                .handler({
                                    let this = this.clone();
                                    move |window, cx| {
                                        let agent = Agent::Custom { id: id.clone() };
                                        this.update(cx, |this, cx| {
                                            if let Some((server, thread_store)) =
                                                this.server_for_agent(&agent, cx)
                                            {
                                                this.switch_draft_agent(
                                                    agent,
                                                    server,
                                                    thread_store,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        })
                                        .ok();
                                    }
                                }),
                        );
                    }

                    menu.separator().item(
                        ContextMenuEntry::new("Add More Agents")
                            .icon(IconName::Plus)
                            .icon_color(Color::Muted)
                            .handler(|window, cx| {
                                window.dispatch_action(Box::new(mav_actions::AcpRegistry), cx)
                            }),
                    )
                }))
            })
    }
}
