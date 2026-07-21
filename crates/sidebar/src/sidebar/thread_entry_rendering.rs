use super::*;

impl Sidebar {
    pub(super) fn render_thread(
        &self,
        ix: usize,
        thread: &ThreadEntry,
        is_active: bool,
        is_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_notification = self.contents.is_thread_notified(&thread.metadata.thread_id);

        let title: SharedString = thread.metadata.display_title();
        let metadata = thread.metadata.clone();
        let thread_workspace = thread.workspace.clone();

        let is_hovered = self.hovered_thread_index == Some(ix);
        let is_selected = is_active;
        let is_draft = thread.draft.is_some();
        let is_empty_draft = thread.draft == Some(DraftKind::Empty);
        let is_running = matches!(
            thread.status,
            AgentThreadStatus::Running | AgentThreadStatus::WaitingForConfirmation
        );
        let is_renaming = self.renaming_thread_id == Some(thread.metadata.thread_id);

        let thread_id_for_actions = thread.metadata.thread_id;
        let session_id_for_delete = thread.metadata.session_id.clone();
        let focus_handle = self.focus_handle.clone();
        let title_editor = self.thread_rename_editor.clone();

        let id = SharedString::from(format!("thread-entry-{}", ix));

        let color = cx.theme().colors();
        let sidebar_bg = color.editor_background;

        let timestamp: SharedString = if is_empty_draft {
            SharedString::default()
        } else {
            format_history_entry_timestamp(Self::thread_display_time(&thread.metadata)).into()
        };

        let is_remote = thread.workspace.is_remote(cx);

        let worktrees = apply_worktree_label_mode(
            thread.worktrees.clone(),
            cx.flag_value::<AgentThreadWorktreeLabelFlag>(),
        );

        let (icon, icon_svg) = if is_draft {
            (IconName::Circle, None)
        } else {
            (thread.icon, thread.icon_from_external_svg.clone())
        };

        let title_generating = thread.is_title_generating
            || self
                .regenerating_titles
                .contains(&thread.metadata.thread_id);

        let thread_item = ThreadItem::new(id, title.clone())
            .base_bg(sidebar_bg)
            .icon(icon)
            .when(is_draft, |this| {
                this.icon_color(Color::Custom(cx.theme().colors().icon_muted.opacity(0.2)))
            })
            .status(thread.status)
            .is_remote(is_remote)
            .when_some(icon_svg, |this, svg| {
                this.custom_icon_from_external_svg(svg)
            })
            .worktrees(worktrees)
            .timestamp(timestamp)
            .highlight_positions(thread.highlight_positions.to_vec())
            .title_generating(title_generating)
            .notified(has_notification)
            .when(thread.diff_stats.lines_added > 0, |this| {
                this.added(thread.diff_stats.lines_added as usize)
            })
            .when(thread.diff_stats.lines_removed > 0, |this| {
                this.removed(thread.diff_stats.lines_removed as usize)
            })
            .selected(is_selected)
            .focused(is_focused)
            .hovered(is_hovered)
            .on_hover(cx.listener(move |this, is_hovered: &bool, _window, cx| {
                if *is_hovered {
                    this.hovered_thread_index = Some(ix);
                } else if this.hovered_thread_index == Some(ix) {
                    this.hovered_thread_index = None;
                }
                cx.notify();
            }))
            .when(is_renaming, |this| {
                this.is_truncated(false).title_slot(
                    div()
                        .h_full()
                        .min_w_0()
                        .flex_1()
                        .capture_action(cx.listener(
                            |this, _: &editor::actions::Newline, window, cx| {
                                this.finish_thread_rename(window, cx);
                            },
                        ))
                        .on_action(cx.listener(|this, _: &Confirm, window, cx| {
                            this.finish_thread_rename(window, cx);
                        }))
                        .on_action(
                            cx.listener(|this, _: &editor::actions::Cancel, window, cx| {
                                this.finish_thread_rename(window, cx);
                            }),
                        )
                        .child(title_editor),
                )
            })
            .when(is_hovered && !is_renaming, |this| {
                let rename_button = IconButton::new(("rename-thread", ix), IconName::Pencil)
                    .icon_size(IconSize::Small)
                    .tooltip({
                        let focus_handle = focus_handle.clone();
                        move |_window, cx| {
                            Tooltip::for_action_in(
                                "Rename Thread",
                                &RenameSelectedThread,
                                &focus_handle,
                                cx,
                            )
                        }
                    })
                    .on_click({
                        let title = title.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.start_renaming_thread(
                                ix,
                                thread_id_for_actions,
                                title.clone(),
                                window,
                                cx,
                            );
                        })
                    });

                let contextual_action: Option<AnyElement> = if is_running {
                    Some(
                        IconButton::new("stop-thread", IconName::Stop)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Error)
                            .style(ButtonStyle::Tinted(TintColor::Error))
                            .tooltip(Tooltip::text("Stop Generation"))
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.stop_thread(&thread_id_for_actions, cx);
                            }))
                            .into_any_element(),
                    )
                } else {
                    match thread.draft {
                        Some(DraftKind::Empty) => None,
                        Some(DraftKind::WithContent) => Some(
                            IconButton::new("discard_thread", IconName::Close)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text("Discard Draft"))
                                .on_click({
                                    let thread_workspace = thread_workspace.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        this.remove_draft(
                                            thread_id_for_actions,
                                            &thread_workspace,
                                            window,
                                            cx,
                                        );
                                    })
                                })
                                .into_any_element(),
                        ),
                        None => Some(
                            IconButton::new("archive-thread", IconName::Archive)
                                .icon_size(IconSize::Small)
                                .tooltip({
                                    let focus_handle = focus_handle.clone();
                                    move |_window, cx| {
                                        Tooltip::for_action_in(
                                            "Archive Thread",
                                            &ArchiveSelectedThread,
                                            &focus_handle,
                                            cx,
                                        )
                                    }
                                })
                                .on_click({
                                    let session_id = session_id_for_delete.clone();
                                    cx.listener(move |this, _, window, cx| {
                                        if let Some(ref session_id) = session_id {
                                            this.archive_thread(session_id, window, cx);
                                        }
                                    })
                                })
                                .into_any_element(),
                        ),
                    }
                };

                this.action_slot(
                    h_flex()
                        .gap_0p5()
                        .child(rename_button)
                        .when_some(contextual_action, |this, action| this.child(action)),
                )
            })
            .on_click({
                let thread_workspace = thread_workspace.clone();
                cx.listener(move |this, _, window, cx| {
                    this.selection = None;
                    match &thread_workspace {
                        ThreadEntryWorkspace::Open(workspace) => {
                            this.activate_thread(metadata.clone(), workspace, false, window, cx);
                        }
                        ThreadEntryWorkspace::Closed {
                            folder_paths,
                            project_group_key,
                        } => {
                            this.open_workspace_and_activate_thread(
                                metadata.clone(),
                                folder_paths.clone(),
                                project_group_key,
                                window,
                                cx,
                            );
                        }
                    }
                })
            });

        if is_draft || thread.metadata.session_id.is_none() {
            return thread_item.into_any_element();
        }

        let Some(session_id) = thread.metadata.session_id.clone() else {
            return thread_item.into_any_element();
        };

        let context_menu_id = SharedString::from(format!("thread-context-menu-{}", ix));
        let sidebar = cx.weak_entity();

        let active_workspace = self.active_workspace(cx);
        let thread_workspace = match &thread_workspace {
            ThreadEntryWorkspace::Open(workspace) => Some(workspace.clone()),
            ThreadEntryWorkspace::Closed { .. } => None,
        };

        let is_mav_thread = thread.metadata.agent_id.as_ref() == MAV_AGENT_ID.as_ref();
        let can_open_as_markdown = thread.is_live || is_mav_thread;
        let folder_paths = thread.metadata.folder_paths().clone();

        right_click_menu(context_menu_id)
            .trigger(move |_, _, _| thread_item)
            .menu({
                let thread_id = thread.metadata.thread_id;
                let markdown_title = Some(thread.metadata.display_title());
                let rename_title = title;
                move |_window, cx| {
                    let session_id = session_id.clone();
                    let sidebar = sidebar.clone();
                    let active_workspace = active_workspace.clone();
                    let thread_workspace = thread_workspace.clone();
                    let markdown_title = markdown_title.clone();
                    let rename_title = rename_title.clone();
                    let folder_paths = folder_paths.clone();
                    ContextMenu::build(_window, cx, move |mut menu, _window, _cx| {
                        menu = menu.entry("Rename Title", None, {
                            let sidebar = sidebar.clone();
                            let rename_title = rename_title.clone();
                            move |window, cx| {
                                sidebar
                                    .update(cx, |sidebar, cx| {
                                        sidebar.start_renaming_thread(
                                            ix,
                                            thread_id,
                                            rename_title.clone(),
                                            window,
                                            cx,
                                        );
                                    })
                                    .ok();
                            }
                        });

                        if is_mav_thread {
                            menu = menu.entry("Regenerate Thread Title", None, {
                                let session_id = session_id.clone();
                                let sidebar = sidebar.clone();
                                let thread_workspace = thread_workspace.clone();
                                let folder_paths = folder_paths.clone();
                                move |_window, cx| {
                                    sidebar
                                        .update(cx, |sidebar, cx| {
                                            sidebar.regenerate_thread_title(
                                                &session_id,
                                                thread_id,
                                                folder_paths.clone(),
                                                thread_workspace.clone(),
                                                cx,
                                            );
                                        })
                                        .ok();
                                }
                            });
                        }

                        if can_open_as_markdown {
                            menu = menu.entry("Open Thread as Markdown", None, {
                                let session_id = session_id.clone();
                                let markdown_title = markdown_title.clone();
                                let thread_workspace = thread_workspace.clone();
                                move |window, cx| {
                                    if let Some(thread_workspace) = thread_workspace.as_ref()
                                        && let Some(panel) =
                                            thread_workspace.read(cx).panel::<AgentPanel>(cx)
                                    {
                                        let opened = panel.update(cx, |panel, cx| {
                                            panel.open_thread_as_markdown(
                                                thread_id,
                                                thread_workspace.clone(),
                                                window,
                                                cx,
                                            )
                                        });
                                        if opened {
                                            return;
                                        }
                                    }

                                    if is_mav_thread
                                        && let Some(active_workspace) = &active_workspace
                                    {
                                        Self::open_closed_native_thread_as_markdown(
                                            &session_id,
                                            markdown_title.clone(),
                                            active_workspace,
                                            window,
                                            cx,
                                        );
                                    }
                                }
                            });
                        }

                        menu.separator().entry("Archive Thread", None, {
                            let session_id = session_id.clone();
                            move |window, cx| {
                                sidebar
                                    .update(cx, |sidebar, cx| {
                                        sidebar.archive_thread(&session_id, window, cx);
                                    })
                                    .ok();
                            }
                        })
                    })
                }
            })
            .into_any_element()
    }

    pub(super) fn render_terminal(
        &self,
        ix: usize,
        terminal: &TerminalEntry,
        is_active: bool,
        is_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = ElementId::from(format!("terminal-{}", terminal.metadata.terminal_id));
        let timestamp = format_history_entry_timestamp(terminal.metadata.created_at);
        let is_hovered = self.hovered_thread_index == Some(ix);
        let color = cx.theme().colors();
        let sidebar_bg = color.editor_background;
        let metadata = terminal.metadata.clone();
        let workspace = terminal.workspace.clone();
        let focus_handle = self.focus_handle.clone();
        let worktrees = apply_worktree_label_mode(
            terminal.worktrees.clone(),
            cx.flag_value::<AgentThreadWorktreeLabelFlag>(),
        );
        let is_remote = terminal.workspace.is_remote(cx);

        let display_title = terminal.metadata.display_title();
        let (icon_char, title, highlight_positions) =
            match split_leading_icon_char(&display_title, &terminal.highlight_positions) {
                Some((icon_char, title, positions)) => (Some(icon_char), title, positions),
                None => (None, display_title, terminal.highlight_positions.clone()),
            };

        ThreadItem::new(id, title)
            .base_bg(sidebar_bg)
            .icon(IconName::Terminal)
            .when_some(icon_char, |this, icon_char| this.icon_char(icon_char))
            .is_remote(is_remote)
            .worktrees(worktrees)
            .timestamp(timestamp)
            .notified(terminal.has_notification)
            .highlight_positions(highlight_positions)
            .selected(is_active)
            .focused(is_focused)
            .hovered(is_hovered)
            .on_hover(cx.listener(move |this, is_hovered: &bool, _window, cx| {
                if *is_hovered {
                    this.hovered_thread_index = Some(ix);
                } else if this.hovered_thread_index == Some(ix) {
                    this.hovered_thread_index = None;
                }
                cx.notify();
            }))
            .when(is_hovered, |this| {
                this.action_slot(
                    IconButton::new("close-terminal", IconName::Close)
                        .icon_size(IconSize::Small)
                        .icon_color(Color::Muted)
                        .tooltip({
                            let focus_handle = focus_handle.clone();
                            move |_window, cx| {
                                Tooltip::for_action_in(
                                    "Close Terminal",
                                    &ArchiveSelectedThread,
                                    &focus_handle,
                                    cx,
                                )
                            }
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.close_terminal(&metadata, &workspace, window, cx);
                        })),
                )
            })
            .on_click(cx.listener({
                let metadata = terminal.metadata.clone();
                let workspace = terminal.workspace.clone();
                move |this, _, window, cx| {
                    this.activate_terminal_entry(
                        metadata.clone(),
                        workspace.clone(),
                        false,
                        window,
                        cx,
                    );
                }
            }))
            .into_any_element()
    }
}
