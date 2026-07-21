use super::*;

impl ThreadView {
    pub(super) fn render_send_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let message_editor = self.message_editor.read(cx);
        let is_editor_empty = message_editor.is_empty(cx);
        let focus_handle = message_editor.focus_handle(cx);

        let is_generating = self.thread.read(cx).status() != ThreadStatus::Idle;

        if self.is_loading_contents {
            div()
                .id("loading-message-content")
                .px_1()
                .tooltip(Tooltip::text("Loading Added Context…"))
                .child(loading_contents_spinner(IconSize::default()))
                .into_any_element()
        } else if is_generating && is_editor_empty {
            IconButton::new("stop-generation", IconName::Stop)
                .icon_color(Color::Error)
                .style(ButtonStyle::Tinted(TintColor::Error))
                .tooltip(move |_window, cx| {
                    Tooltip::for_action("Stop Generation", &editor::actions::Cancel, cx)
                })
                .on_click(cx.listener(|this, _event, _, cx| this.cancel_generation(cx)))
                .into_any_element()
        } else {
            let send_icon = if is_generating {
                IconName::QueueMessage
            } else {
                IconName::Send
            };
            IconButton::new("send-message", send_icon)
                .style(ButtonStyle::Filled)
                .map(|this| {
                    if is_editor_empty && !is_generating {
                        this.disabled(true).icon_color(Color::Muted)
                    } else {
                        this.icon_color(Color::Accent)
                    }
                })
                .tooltip(move |_window, cx| {
                    if is_editor_empty && !is_generating {
                        Tooltip::for_action("Type to Send", &Chat, cx)
                    } else if is_generating {
                        let focus_handle = focus_handle.clone();

                        Tooltip::element(move |_window, cx| {
                            v_flex()
                                .gap_1()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .justify_between()
                                        .child(Label::new("Queue and Send"))
                                        .child(KeyBinding::for_action_in(&Chat, &focus_handle, cx)),
                                )
                                .child(
                                    h_flex()
                                        .pt_1()
                                        .gap_2()
                                        .justify_between()
                                        .border_t_1()
                                        .border_color(cx.theme().colors().border_variant)
                                        .child(Label::new("Send Immediately"))
                                        .child(KeyBinding::for_action_in(
                                            &SendImmediately,
                                            &focus_handle,
                                            cx,
                                        )),
                                )
                                .into_any_element()
                        })(_window, cx)
                    } else {
                        Tooltip::for_action("Send Message", &Chat, cx)
                    }
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.send(window, cx);
                }))
                .into_any_element()
        }
    }

    pub(super) fn render_add_context_button(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.message_editor.focus_handle(cx);
        let weak_self = cx.weak_entity();

        PopoverMenu::new("add-context-menu")
            .trigger_with_tooltip(
                IconButton::new("add-context", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted),
                {
                    move |_window, cx| {
                        Tooltip::for_action_in(
                            "Add Context",
                            &OpenAddContextMenu,
                            &focus_handle,
                            cx,
                        )
                    }
                },
            )
            .anchor(gpui::Anchor::BottomLeft)
            .with_handle(self.add_context_menu_handle.clone())
            .offset(gpui::Point {
                x: px(0.0),
                y: px(-2.0),
            })
            .menu(move |window, cx| {
                weak_self
                    .update(cx, |this, cx| this.build_add_context_menu(window, cx))
                    .ok()
            })
    }

    fn build_add_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let message_editor = self.message_editor.clone();
        let workspace = self.workspace.clone();
        let session_capabilities = self.session_capabilities.read();
        let supports_images = session_capabilities.supports_images();
        let supports_embedded_context = session_capabilities.supports_embedded_context();
        let available_skills = session_capabilities.completion_skills();
        drop(session_capabilities);

        let has_editor_selection = workspace
            .upgrade()
            .and_then(|ws| {
                ws.read(cx)
                    .active_item(cx)
                    .and_then(|item| item.downcast::<Editor>())
            })
            .is_some_and(|editor| {
                editor.update(cx, |editor, cx| {
                    editor.has_non_empty_selection(&editor.display_snapshot(cx))
                })
            });

        let has_terminal_selection = workspace
            .upgrade()
            .and_then(|ws| ws.read(cx).panel::<TerminalPanel>(cx))
            .is_some_and(|panel| !panel.read(cx).terminal_selections(cx).is_empty());

        let has_selection = has_editor_selection || has_terminal_selection;

        ContextMenu::build(window, cx, move |menu, _window, _cx| {
            menu.key_context("AddContextMenu")
                .item(
                    ContextMenuEntry::new("Files & Directories")
                        .icon(IconName::File)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .handler({
                            let message_editor = message_editor.clone();
                            move |window, cx| {
                                message_editor.focus_handle(cx).focus(window, cx);
                                message_editor.update(cx, |editor, cx| {
                                    editor.insert_context_type("file", window, cx);
                                });
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new("Symbols")
                        .icon(IconName::Code)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .handler({
                            let message_editor = message_editor.clone();
                            move |window, cx| {
                                message_editor.focus_handle(cx).focus(window, cx);
                                message_editor.update(cx, |editor, cx| {
                                    editor.insert_context_type("symbol", window, cx);
                                });
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new("Threads")
                        .icon(IconName::Thread)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .handler({
                            let message_editor = message_editor.clone();
                            move |window, cx| {
                                message_editor.focus_handle(cx).focus(window, cx);
                                message_editor.update(cx, |editor, cx| {
                                    editor.insert_context_type("thread", window, cx);
                                });
                            }
                        }),
                )
                .when(!available_skills.is_empty(), |this| {
                    this.submenu_with_colored_icon("Skills", IconName::Sparkle, Color::Muted, {
                        let message_editor = message_editor.clone();
                        let available_skills = available_skills.clone();
                        move |mut menu, _window, _cx| {
                            for skill in &available_skills {
                                menu = menu
                                    .item(Self::skill_menu_entry(skill, message_editor.clone()));
                            }
                            menu
                        }
                    })
                })
                .item(
                    ContextMenuEntry::new("Image")
                        .icon(IconName::Image)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .disabled(!supports_images)
                        .handler({
                            let message_editor = message_editor.clone();
                            move |window, cx| {
                                message_editor.focus_handle(cx).focus(window, cx);
                                message_editor.update(cx, |editor, cx| {
                                    editor.add_images_from_picker(window, cx);
                                });
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new("Selection")
                        .icon(IconName::CursorIBeam)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .disabled(!has_selection)
                        .handler({
                            move |window, cx| {
                                window.dispatch_action(
                                    mav_actions::agent::AddSelectionToThread.boxed_clone(),
                                    cx,
                                );
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new("Branch Diff")
                        .icon(IconName::GitBranch)
                        .icon_color(Color::Muted)
                        .icon_size(IconSize::XSmall)
                        .disabled(!supports_embedded_context)
                        .handler({
                            move |window, cx| {
                                message_editor.update(cx, |editor, cx| {
                                    editor.insert_branch_diff_crease(window, cx);
                                });
                            }
                        }),
                )
        })
    }

    fn skill_menu_entry(
        skill: &AvailableSkill,
        message_editor: Entity<crate::message_editor::MessageEditor>,
    ) -> ContextMenuEntry {
        let label = format!("{} ({})", skill.name, skill.source);
        let skill = skill.clone();

        ContextMenuEntry::new(label)
            .icon(IconName::Sparkle)
            .icon_color(Color::Muted)
            .icon_size(IconSize::XSmall)
            .handler(move |window, cx| {
                message_editor.focus_handle(cx).focus(window, cx);
                message_editor.update(cx, |editor, cx| {
                    editor.insert_skill_crease(&skill, window, cx);
                });
            })
    }

    pub(super) fn render_follow_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let following = self.is_following(cx);

        let tooltip_label = if following {
            if self.agent_id.as_ref() == agent::MAV_AGENT_ID.as_ref() {
                format!("Stop Following the {}", self.agent_id)
            } else {
                format!("Stop Following {}", self.agent_id)
            }
        } else if self.agent_id.as_ref() == agent::MAV_AGENT_ID.as_ref() {
            format!("Follow the {}", self.agent_id)
        } else {
            format!("Follow {}", self.agent_id)
        };

        IconButton::new("follow-agent", IconName::Crosshair)
            .icon_size(IconSize::Small)
            .icon_color(Color::Muted)
            .toggle_state(following)
            .selected_icon_color(Some(Color::Custom(cx.theme().players().agent().cursor)))
            .tooltip(move |_window, cx| {
                if following {
                    Tooltip::for_action(tooltip_label.clone(), &Follow, cx)
                } else {
                    Tooltip::with_meta(
                        tooltip_label.clone(),
                        Some(&Follow),
                        "Track the agent's location as it reads and edits files.",
                        cx,
                    )
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.toggle_following(window, cx);
            }))
    }
}
