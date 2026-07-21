use super::*;

impl ThreadView {
    pub(super) fn render_draft_agent_selector(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(project) = self.project.upgrade() else {
            return div().into_any_element();
        };

        let selected_agent = self
            .server_view
            .upgrade()
            .map(|server_view| server_view.read(cx).agent_key().clone())
            .unwrap_or_else(|| Agent::from(self.agent_id.clone()));

        let agent_server_store = project.read(cx).agent_server_store().clone();
        let is_via_collab = project.read(cx).is_via_collab();

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
        let server_view = self.server_view.clone();
        let id_suffix = self.root_thread_id.to_key_string();
        let menu_id = SharedString::from(format!("draft-agent-selector-{id_suffix}"));
        let trigger_id = SharedString::from(format!("draft-agent-selector-trigger-{id_suffix}"));
        let (color, icon) = if self.draft_agent_selector_menu_handle.is_deployed() {
            (Color::Accent, IconName::ChevronUp)
        } else {
            (Color::Muted, IconName::ChevronDown)
        };

        PopoverMenu::new(menu_id)
            .trigger_with_tooltip(
                Button::new(trigger_id, selected_agent_label)
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
            .with_handle(self.draft_agent_selector_menu_handle.clone())
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
                                let server_view = server_view.clone();
                                move |window, cx| {
                                    server_view
                                        .update(cx, |server_view, cx| {
                                            server_view.switch_draft_agent_to(
                                                Agent::NativeAgent,
                                                window,
                                                cx,
                                            );
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
                                    let server_view = server_view.clone();
                                    move |window, cx| {
                                        server_view
                                            .update(cx, |server_view, cx| {
                                                server_view.switch_draft_agent_to(
                                                    Agent::Custom { id: id.clone() },
                                                    window,
                                                    cx,
                                                );
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
            .into_any_element()
    }
}
