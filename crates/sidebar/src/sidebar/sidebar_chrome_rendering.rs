use super::*;

impl Sidebar {
    pub(super) fn render_no_results(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("sidebar-no-results")
            .p_4()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                Label::new("No threads yet")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
    }

    pub(super) fn render_empty_state(&self, cx: &mut Context<Self>) -> impl IntoElement {
        ProjectEmptyState::new(
            "Sidebar",
            self.focus_handle(cx),
            KeyBinding::for_action(&workspace::Open::default(), cx),
        )
        .on_open_project(|_, window, cx| {
            let side = match SidebarSettings::get_global(cx).side() {
                SidebarSide::Left => "left",
                SidebarSide::Right => "right",
            };
            telemetry::event!("Sidebar Add Project Clicked", side = side);
            window.dispatch_action(
                Open {
                    create_new_window: Some(false),
                }
                .boxed_clone(),
                cx,
            );
        })
        .on_clone_repo(|_, window, cx| {
            window.dispatch_action(git::Clone.boxed_clone(), cx);
        })
    }

    pub(super) fn render_sidebar_header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let sidebar_side = self.side(cx);
        let sidebar_state = SidebarRenderState {
            open: true,
            side: sidebar_side,
        };
        let sidebar_on_left = sidebar_side == SidebarSide::Left;
        let sidebar_on_right = sidebar_side == SidebarSide::Right;
        let not_fullscreen = !window.is_fullscreen();
        let traffic_lights = cfg!(target_os = "macos") && not_fullscreen && sidebar_on_left;
        let left_window_controls = !cfg!(target_os = "macos") && not_fullscreen && sidebar_on_left;
        let right_window_controls =
            !cfg!(target_os = "macos") && not_fullscreen && sidebar_on_right;
        let traffic_light_buttons = if traffic_lights {
            self.multi_workspace.upgrade().and_then(|multi_workspace| {
                render_sidebar_header_controls_with_state(multi_workspace, sidebar_state, cx)
            })
        } else {
            None
        };
        let left_header_buttons = if !traffic_lights && !sidebar_on_right {
            self.multi_workspace.upgrade().and_then(|multi_workspace| {
                render_sidebar_header_controls_with_state(multi_workspace, sidebar_state, cx)
            })
        } else {
            None
        };
        let right_header_buttons = if !traffic_lights && sidebar_on_right {
            self.multi_workspace.upgrade().and_then(|multi_workspace| {
                render_sidebar_header_controls_with_state(multi_workspace, sidebar_state, cx)
            })
        } else {
            None
        };

        h_flex()
            .relative()
            .flex_none()
            .h(Tab::container_height(cx))
            .bg(cx.theme().colors().tab_bar_background)
            .when(left_window_controls, |this| {
                this.children(Self::render_left_window_controls(window, cx))
            })
            .when(traffic_lights, |this| {
                this.child(ui::utils::traffic_light_spacer_with_child(
                    cx,
                    false,
                    traffic_light_buttons,
                ))
            })
            .map(|this| {
                if !traffic_lights && !left_window_controls {
                    this.pl_1p5()
                } else {
                    this
                }
            })
            .when(!right_window_controls, |this| this.pr_1p5())
            .gap_1()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .border_b_1()
                    .border_color(cx.theme().colors().border),
            )
            .when_some(left_header_buttons, |this, buttons| this.child(buttons))
            .child(div().flex_1())
            .when_some(right_header_buttons, |this, buttons| this.child(buttons))
            .when(right_window_controls, |this| {
                this.children(Self::render_right_window_controls(window, cx))
            })
    }

    pub(super) fn render_left_window_controls(window: &Window, cx: &mut App) -> Option<AnyElement> {
        platform_title_bar::render_left_window_controls(
            title_bar::sidebar_button_layout(cx).or_else(|| cx.button_layout()),
            Box::new(CloseWindow),
            window,
        )
    }

    pub(super) fn render_right_window_controls(
        window: &Window,
        cx: &mut App,
    ) -> Option<AnyElement> {
        platform_title_bar::render_right_window_controls(
            title_bar::sidebar_button_layout(cx).or_else(|| cx.button_layout()),
            Box::new(CloseWindow),
            window,
        )
    }

    pub(super) fn active_agent_conversation_view(
        &self,
        cx: &App,
    ) -> Option<Entity<ConversationView>> {
        self.active_workspace(cx)?
            .read(cx)
            .active_item_as::<AgentThreadItem>(cx)
            .map(|item| item.read(cx).conversation_view())
    }

    pub(super) fn active_project_agents_md_exists(&self, cx: &App) -> bool {
        let Some(workspace) = self.active_workspace(cx) else {
            return false;
        };
        let project = workspace.read(cx).project().clone();
        let Ok(rel_path) = util::rel_path::RelPath::unix("AGENTS.md") else {
            return false;
        };
        project
            .read(cx)
            .visible_worktrees(cx)
            .next()
            .and_then(|worktree| {
                let worktree = worktree.read(cx);
                worktree
                    .entry_for_path(rel_path)
                    .is_some_and(|entry| entry.is_file())
                    .then_some(())
            })
            .is_some()
    }

    pub(super) fn render_agent_options_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let on_right = self.side(cx) == SidebarSide::Right;
        let active_conversation_view = self.active_agent_conversation_view(cx);
        let can_regenerate_thread_title =
            active_conversation_view
                .as_ref()
                .is_some_and(|conversation_view| {
                    let conversation_view = conversation_view.read(cx);
                    conversation_view.has_user_submitted_prompt(cx)
                        && conversation_view
                            .as_native_thread(cx)
                            .is_some_and(|thread| !thread.read(cx).is_generating_title())
                });
        let has_auth_methods = active_conversation_view
            .as_ref()
            .is_some_and(|conversation_view| conversation_view.read(cx).has_auth_methods());
        let supports_logout = active_conversation_view
            .as_ref()
            .is_some_and(|conversation_view| conversation_view.read(cx).supports_logout());
        let global_agents_md_loaded = UserAgentsMd::global(cx)
            .and_then(|md| md.content())
            .is_some();
        let project_agents_md_exists = self.active_project_agents_md_exists(cx);
        let focus_handle = self.focus_handle.clone();
        let sidebar = cx.weak_entity();

        PopoverMenu::new("agent-sidebar-options-menu")
            .trigger_with_tooltip(
                IconButton::new("agent-sidebar-options-menu", IconName::Robot)
                    .icon_size(IconSize::Small),
                Tooltip::text("Agent Menu"),
            )
            .anchor(if on_right {
                gpui::Anchor::BottomRight
            } else {
                gpui::Anchor::BottomLeft
            })
            .attach(if on_right {
                gpui::Anchor::TopRight
            } else {
                gpui::Anchor::TopLeft
            })
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-4.0),
            })
            .with_handle(self.agent_options_menu_handle.clone())
            .menu(move |window, cx| {
                let active_conversation_view = active_conversation_view.clone();
                let sidebar = sidebar.clone();
                let focus_handle = focus_handle.clone();
                Some(ContextMenu::build(
                    window,
                    cx,
                    move |mut menu, _window, _| {
                        menu = menu.context(focus_handle.clone());

                        if can_regenerate_thread_title {
                            menu = menu.header("Current Thread");
                            if let Some(conversation_view) = active_conversation_view.clone() {
                                menu = menu
                                    .entry("Regenerate Thread Title", None, {
                                        let sidebar = sidebar.clone();
                                        move |_window, cx| {
                                            let result = conversation_view.update(
                                                cx,
                                                |conversation_view, cx| {
                                                    conversation_view.regenerate_thread_title(cx)
                                                },
                                            );
                                            if matches!(
                                                result,
                                                ThreadTitleRegenerationResult::NoModel
                                            ) {
                                                sidebar
                                                .update(cx, |sidebar, cx| {
                                                    if let Some(workspace) =
                                                        sidebar.active_workspace(cx)
                                                    {
                                                        Self::show_no_thread_summary_model_toast(
                                                            workspace, cx,
                                                        );
                                                    }
                                                })
                                                .ok();
                                            }
                                        }
                                    })
                                    .separator();
                            }
                        }

                        menu = menu
                            .header("MCP Servers")
                            .action("Add Custom Server…", Box::new(AddContextServer::local()))
                            .action("Add Remote Server…", Box::new(AddContextServer::remote()))
                            .action(
                                "Install New Servers…",
                                Box::new(mav_actions::Extensions {
                                    category_filter: Some(
                                        mav_actions::ExtensionCategoryFilter::ContextServers,
                                    ),
                                    id: None,
                                }),
                            )
                            .separator()
                            .header("Context")
                            .action("Skills", Box::new(ManageSkills));

                        if global_agents_md_loaded || project_agents_md_exists {
                            if global_agents_md_loaded {
                                menu = menu
                                    .action("Open Global Rules", Box::new(OpenGlobalAgentsMdRules));
                            }
                            if project_agents_md_exists {
                                menu = menu.action(
                                    "Open Project Rules",
                                    Box::new(OpenProjectAgentsMdRules),
                                );
                            }
                            menu = menu.separator();
                        }

                        menu = menu
                            .action("Profiles", Box::new(ManageProfiles::default()))
                            .action("Settings", Box::new(OpenSettings))
                            .separator()
                            .action("Toggle Sidebar", Box::new(ToggleSidebar));

                        if has_auth_methods || supports_logout {
                            menu = menu.separator();
                        }
                        if has_auth_methods {
                            if let Some(conversation_view) = active_conversation_view.clone() {
                                menu = menu.entry("Reauthenticate", None, move |window, cx| {
                                    conversation_view.update(cx, |conversation_view, cx| {
                                        conversation_view.reauthenticate(window, cx)
                                    });
                                });
                            }
                        }
                        if supports_logout {
                            if let Some(conversation_view) = active_conversation_view.clone() {
                                menu = menu.entry("Log Out", None, move |window, cx| {
                                    conversation_view.update(cx, |conversation_view, cx| {
                                        conversation_view.logout(window, cx)
                                    });
                                });
                            }
                        }

                        menu
                    },
                ))
            })
    }

    pub(super) fn render_sidebar_bottom_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_archive = matches!(self.view, SidebarView::Archive(..));
        let on_right = self.side(cx) == SidebarSide::Right;

        v_flex()
            .p_1()
            .gap_1()
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(self.sidebar_chrome.clone())
            .child(
                h_flex()
                    .gap_1()
                    .when(on_right, |this| this.flex_row_reverse())
                    .child(self.render_agent_options_menu(cx))
                    .child(
                        IconButton::new("history", IconName::Clock)
                            .icon_size(IconSize::Small)
                            .toggle_state(is_archive)
                            .tooltip(move |_, cx| {
                                let label = if is_archive {
                                    "Hide Thread History"
                                } else {
                                    "Show Thread History"
                                };
                                Tooltip::for_action(label, &ToggleThreadHistory, cx)
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_archive(&ToggleThreadHistory, window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(self.render_recent_projects_button(cx)),
            )
    }
}
