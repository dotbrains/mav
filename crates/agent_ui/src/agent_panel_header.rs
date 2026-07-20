use super::*;

impl AgentPanel {
    fn is_title_editor_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        match self.visible_surface() {
            VisibleSurface::AgentThread(conversation_view) => conversation_view
                .read(cx)
                .root_thread_view()
                .is_some_and(|view| view.read(cx).title_editor.read(cx).is_focused(window)),
            VisibleSurface::Terminal(_) => self
                .active_terminal_id()
                .and_then(|id| self.terminals.get(&id))
                .and_then(|terminal| terminal.title_editor.as_ref())
                .is_some_and(|editor| editor.read(cx).is_focused(window)),
            _ => false,
        }
    }

    pub(super) fn should_show_title_edit(&self, window: &Window, cx: &Context<Self>) -> bool {
        matches!(
            self.visible_surface(),
            VisibleSurface::AgentThread(_) | VisibleSurface::Terminal(_)
        ) && self.has_open_project(cx)
            && !self.is_title_editor_focused(window, cx)
    }

    pub(super) fn render_title_view(&self, window: &mut Window, cx: &Context<Self>) -> AnyElement {
        let content = match self.visible_surface() {
            VisibleSurface::AgentThread(conversation_view) => {
                let server_view_ref = conversation_view.read(cx);
                let native_thread = server_view_ref.as_native_thread(cx);
                let is_generating_title = native_thread
                    .as_ref()
                    .is_some_and(|thread| thread.read(cx).is_generating_title());
                let title_generation_failed = native_thread
                    .as_ref()
                    .is_some_and(|thread| thread.read(cx).has_failed_title_generation());

                if let Some(title_editor) = server_view_ref
                    .root_thread_view()
                    .map(|r| r.read(cx).title_editor.clone())
                {
                    if is_generating_title {
                        Label::new(server_view_ref.title(cx))
                            .color(Color::Muted)
                            .truncate()
                            .with_animation(
                                "generating_title",
                                Animation::new(Duration::from_secs(2))
                                    .repeat()
                                    .with_easing(pulsating_between(0.4, 0.8)),
                                |label, delta| label.alpha(delta),
                            )
                            .into_any_element()
                    } else {
                        let editable_title = div()
                            .flex_1()
                            .on_action({
                                let conversation_view = conversation_view.downgrade();
                                move |_: &menu::Confirm, window, cx| {
                                    if let Some(conversation_view) = conversation_view.upgrade() {
                                        conversation_view.focus_handle(cx).focus(window, cx);
                                    }
                                }
                            })
                            .on_action({
                                let conversation_view = conversation_view.downgrade();
                                move |_: &editor::actions::Cancel, window, cx| {
                                    if let Some(conversation_view) = conversation_view.upgrade() {
                                        conversation_view.focus_handle(cx).focus(window, cx);
                                    }
                                }
                            })
                            .child(title_editor);

                        if title_generation_failed {
                            h_flex()
                                .w_full()
                                .gap_1()
                                .child(editable_title)
                                .child(
                                    IconButton::new("retry-thread-title", IconName::XCircle)
                                        .icon_color(Color::Error)
                                        .icon_size(IconSize::Small)
                                        .tooltip(Tooltip::text("Title generation failed. Retry"))
                                        .on_click({
                                            let conversation_view = conversation_view.clone();
                                            let workspace = self.workspace.clone();
                                            move |_event, _window, cx| {
                                                Self::handle_regenerate_thread_title(
                                                    conversation_view.clone(),
                                                    workspace.clone(),
                                                    cx,
                                                );
                                            }
                                        }),
                                )
                                .into_any_element()
                        } else {
                            editable_title.w_full().into_any_element()
                        }
                    }
                } else {
                    Label::new(conversation_view.read(cx).title(cx))
                        .color(Color::Muted)
                        .truncate()
                        .into_any_element()
                }
            }
            VisibleSurface::Terminal(_) => {
                if let Some((terminal_id, title_editor, title)) =
                    self.active_terminal_id().and_then(|terminal_id| {
                        self.terminals.get(&terminal_id).map(|terminal| {
                            (
                                terminal_id,
                                terminal.title_editor.clone(),
                                terminal.title(cx),
                            )
                        })
                    })
                {
                    if let Some(title_editor) = title_editor {
                        div()
                            .flex_1()
                            .on_action(cx.listener(move |this, _: &menu::Confirm, window, cx| {
                                this.stop_editing_terminal_title(terminal_id, true, window, cx);
                            }))
                            .on_action(cx.listener(
                                move |this, _: &editor::actions::Cancel, window, cx| {
                                    this.stop_editing_terminal_title(terminal_id, true, window, cx);
                                },
                            ))
                            .child(title_editor)
                            .into_any_element()
                    } else {
                        div()
                            .id("terminal-title")
                            .flex_1()
                            .cursor_text()
                            .overflow_x_scroll()
                            .child(Label::new(title).color(Color::Muted).single_line())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit_terminal_title(terminal_id, window, cx);
                            }))
                            .into_any_element()
                    }
                } else {
                    Label::new("Terminal").into_any_element()
                }
            }
            VisibleSurface::Configuration(_) => {
                Label::new("Settings").truncate().into_any_element()
            }
            VisibleSurface::Uninitialized => Label::new("Agent").truncate().into_any_element(),
        };

        let toolbar_bg = cx.theme().colors().tab_bar_background;
        let gradient_overlay = GradientFade::new(toolbar_bg, toolbar_bg, toolbar_bg)
            .width(px(64.0))
            .right(px(0.0))
            .gradient_stop(0.75);
        // The fade gradient renders as a visible patch on transparent windows
        // (the title already truncates).
        let opaque_window =
            cx.theme().window_background_appearance() == gpui::WindowBackgroundAppearance::Opaque;

        h_flex()
            .key_context("TitleEditor")
            .group("title_editor")
            .flex_grow_1()
            .w_full()
            .min_w_0()
            .max_w_full()
            .overflow_x_hidden()
            .child(content)
            .when(self.should_show_title_edit(window, cx), |this| {
                this.when(opaque_window, |this| this.child(gradient_overlay))
                    .child(
                        h_flex()
                            .visible_on_hover("title_editor")
                            .absolute()
                            .right_0()
                            .h_full()
                            .bg(cx.theme().colors().tab_bar_background)
                            .child(
                                IconButton::new("edit_tile", IconName::Pencil)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Edit Thread Title")),
                            ),
                    )
            })
            .into_any()
    }

    fn show_no_thread_summary_model_toast(workspace: Entity<Workspace>, cx: &mut App) {
        workspace.update(cx, |workspace, cx| {
            let toast = StatusToast::new(
                "No model is configured for summarizing thread titles.",
                cx,
                |this, _cx| {
                    this.icon(
                        Icon::new(IconName::Warning)
                            .size(IconSize::Small)
                            .color(Color::Warning),
                    )
                    .dismiss_button(true)
                },
            );
            workspace.toggle_status_toast(toast, cx);
        });
    }

    fn handle_regenerate_thread_title(
        conversation_view: Entity<ConversationView>,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) {
        match Self::regenerate_conversation_thread_title(conversation_view, cx) {
            ThreadTitleRegenerationResult::NoModel => {
                if let Some(workspace) = workspace.upgrade() {
                    Self::show_no_thread_summary_model_toast(workspace, cx);
                }
            }
            ThreadTitleRegenerationResult::NotOpen
            | ThreadTitleRegenerationResult::Started
            | ThreadTitleRegenerationResult::AlreadyGenerating => {}
        }
    }

    pub(super) fn render_panel_options_menu(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let focus_handle = self.focus_handle(cx);
        // Resolve menu shortcuts at the thread root; the active editor can
        // shadow panel-level commands such as ManageSkills.
        let menu_action_context = match &self.base_view {
            BaseView::AgentThread { conversation_view } => conversation_view
                .read(cx)
                .active_thread()
                .map(|thread| thread.read(cx).focus_handle.clone())
                .unwrap_or_else(|| focus_handle.clone()),
            _ => focus_handle.clone(),
        };
        let showing_terminal = matches!(self.visible_surface(), VisibleSurface::Terminal(_));

        let conversation_view = match &self.base_view {
            BaseView::AgentThread { conversation_view } => Some(conversation_view.clone()),
            _ => None,
        };

        let can_regenerate_thread_title =
            conversation_view.as_ref().is_some_and(|conversation_view| {
                let conversation_view = conversation_view.read(cx);
                conversation_view.has_user_submitted_prompt(cx)
                    && conversation_view
                        .as_native_thread(cx)
                        .is_some_and(|thread| !thread.read(cx).is_generating_title())
            });

        let has_auth_methods = match &self.base_view {
            BaseView::AgentThread { conversation_view } => {
                conversation_view.read(cx).has_auth_methods()
            }
            _ => false,
        };
        let supports_logout = self
            .active_conversation_view()
            .is_some_and(|conversation_view| conversation_view.read(cx).supports_logout());

        let project_agents_md_path = project_agents_md_path(&self.project, true, cx);

        let global_agents_md_loaded = UserAgentsMd::global(cx)
            .and_then(|md| md.content())
            .is_some();

        let workspace = self.workspace.clone();

        PopoverMenu::new("agent-options-menu")
            .trigger_with_tooltip(
                IconButton::new("agent-options-menu", IconName::Ellipsis)
                    .icon_size(IconSize::Small),
                move |_window, cx| {
                    Tooltip::for_action_in(
                        "Toggle Agent Menu",
                        &ToggleOptionsMenu,
                        &focus_handle,
                        cx,
                    )
                },
            )
            .anchor(Anchor::TopRight)
            .with_handle(self.agent_panel_menu_handle.clone())
            .menu({
                move |window, cx| {
                    Some(ContextMenu::build(window, cx, |mut menu, _window, _| {
                        menu = menu.context(menu_action_context.clone());

                        if can_regenerate_thread_title {
                            menu = menu.header("Current Thread");

                            if let Some(conversation_view) = conversation_view.as_ref() {
                                menu = menu
                                    .entry("Regenerate Thread Title", None, {
                                        let conversation_view = conversation_view.clone();
                                        let workspace = workspace.clone();
                                        move |_, cx| {
                                            Self::handle_regenerate_thread_title(
                                                conversation_view.clone(),
                                                workspace.clone(),
                                                cx,
                                            );
                                        }
                                    })
                                    .separator();
                            }
                        }

                        if !showing_terminal {
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

                            if project_agents_md_path.is_some() || global_agents_md_loaded {
                                if global_agents_md_loaded {
                                    let workspace = workspace.clone();

                                    menu = menu.custom_entry(
                                        |_window, _cx| {
                                            h_flex()
                                                .w_full()
                                                .gap_1()
                                                .child(Label::new("Open Global Rules"))
                                                .child(
                                                    Label::new("(AGENTS.md)")
                                                        .color(Color::Muted)
                                                        .size(LabelSize::Small),
                                                )
                                                .into_any_element()
                                        },
                                        move |window, cx| {
                                            workspace
                                                .update(cx, |workspace, cx| {
                                                    open_global_rules(workspace, window, cx);
                                                })
                                                .log_err();
                                        },
                                    );
                                }

                                if project_agents_md_path.is_some() {
                                    let workspace = workspace.clone();
                                    menu = menu.custom_entry(
                                        |_window, _cx| {
                                            h_flex()
                                                .w_full()
                                                .gap_1()
                                                .child(Label::new("Open Project Rules"))
                                                .child(
                                                    Label::new("(AGENTS.md)")
                                                        .color(Color::Muted)
                                                        .size(LabelSize::Small),
                                                )
                                                .into_any_element()
                                        },
                                        move |window, cx| {
                                            workspace
                                                .update(cx, |workspace, cx| {
                                                    open_project_rules(workspace, window, cx);
                                                })
                                                .log_err();
                                        },
                                    );
                                }

                                menu = menu.separator();
                            }

                            menu = menu.action("Profiles", Box::new(ManageProfiles::default()));
                        }

                        menu = menu
                            .action("Settings", Box::new(OpenSettings))
                            .separator()
                            .action("Toggle Sidebar", Box::new(ToggleSidebar));

                        if has_auth_methods || supports_logout {
                            menu = menu.separator()
                        }
                        if has_auth_methods {
                            menu = menu.action("Reauthenticate", Box::new(ReauthenticateAgent))
                        }
                        if supports_logout {
                            menu = menu.action("Log Out", Box::new(LogoutAgent))
                        }

                        menu
                    }))
                }
            })
    }
}
