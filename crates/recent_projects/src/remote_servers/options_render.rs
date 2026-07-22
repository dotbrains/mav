use super::*;

impl RemoteServerProjects {
    fn render_view_options(
        &mut self,
        options: ViewServerOptionsState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let last_entry = options.entries().last().unwrap();

        let mut view = Navigable::new(
            div()
                .track_focus(&self.focus_handle(cx))
                .size_full()
                .child(match &options {
                    ViewServerOptionsState::Ssh { connection, .. } => SshConnectionHeader {
                        connection_string: connection.host.to_string().into(),
                        paths: Default::default(),
                        nickname: connection.nickname.clone().map(|s| s.into()),
                        is_wsl: false,
                        is_devcontainer: false,
                    }
                    .render(window, cx)
                    .into_any_element(),
                    ViewServerOptionsState::Wsl { connection, .. } => SshConnectionHeader {
                        connection_string: connection.distro_name.clone().into(),
                        paths: Default::default(),
                        nickname: None,
                        is_wsl: true,
                        is_devcontainer: false,
                    }
                    .render(window, cx)
                    .into_any_element(),
                })
                .child(
                    v_flex()
                        .pb_1()
                        .child(ListSeparator)
                        .map(|this| match &options {
                            ViewServerOptionsState::Ssh {
                                connection,
                                entries,
                                server_index,
                            } => this.child(self.render_edit_ssh(
                                connection,
                                *server_index,
                                entries,
                                window,
                                cx,
                            )),
                            ViewServerOptionsState::Wsl {
                                connection,
                                entries,
                                server_index,
                            } => this.child(self.render_edit_wsl(
                                connection,
                                *server_index,
                                entries,
                                window,
                                cx,
                            )),
                        })
                        .child(ListSeparator)
                        .child({
                            div()
                                .id("ssh-options-copy-server-address")
                                .track_focus(&last_entry.focus_handle)
                                .on_action(cx.listener(|this, _: &menu::Confirm, window, cx| {
                                    this.mode = Mode::default_mode(&this.ssh_config_servers, cx);
                                    cx.focus_self(window);
                                    cx.notify();
                                }))
                                .child(
                                    ListItem::new("go-back")
                                        .toggle_state(
                                            last_entry.focus_handle.contains_focused(window, cx),
                                        )
                                        .inset(true)
                                        .spacing(ui::ListItemSpacing::Sparse)
                                        .start_slot(
                                            Icon::new(IconName::ArrowLeft).color(Color::Muted),
                                        )
                                        .child(Label::new("Go Back"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.mode =
                                                Mode::default_mode(&this.ssh_config_servers, cx);
                                            cx.focus_self(window);
                                            cx.notify()
                                        })),
                                )
                        }),
                )
                .into_any_element(),
        );

        for entry in options.entries() {
            view = view.entry(entry.clone());
        }

        view.render(window, cx).into_any_element()
    }

    fn render_edit_wsl(
        &self,
        connection: &WslConnectionOptions,
        index: WslServerIndex,
        entries: &[NavigableEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let distro_name = SharedString::new(connection.distro_name.clone());

        v_flex().child({
            fn remove_wsl_distro(
                remote_servers: Entity<RemoteServerProjects>,
                index: WslServerIndex,
                distro_name: SharedString,
                window: &mut Window,
                cx: &mut App,
            ) {
                let prompt_message = format!("Remove WSL distro `{}`?", distro_name);

                let confirmation = window.prompt(
                    PromptLevel::Warning,
                    &prompt_message,
                    None,
                    &["Yes, remove it", "No, keep it"],
                    cx,
                );

                cx.spawn(async move |cx| {
                    if confirmation.await.ok() == Some(0) {
                        remote_servers.update(cx, |this, cx| {
                            this.delete_wsl_distro(index, cx);
                        });
                        remote_servers.update(cx, |this, cx| {
                            this.mode = Mode::default_mode(&this.ssh_config_servers, cx);
                            cx.notify();
                        });
                    }
                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
            div()
                .id("wsl-options-remove-distro")
                .track_focus(&entries[0].focus_handle)
                .on_action(cx.listener({
                    let distro_name = distro_name.clone();
                    move |_, _: &menu::Confirm, window, cx| {
                        remove_wsl_distro(cx.entity(), index, distro_name.clone(), window, cx);
                        cx.focus_self(window);
                    }
                }))
                .child(
                    ListItem::new("remove-distro")
                        .toggle_state(entries[0].focus_handle.contains_focused(window, cx))
                        .inset(true)
                        .spacing(ui::ListItemSpacing::Sparse)
                        .start_slot(Icon::new(IconName::Trash).color(Color::Error))
                        .child(Label::new("Remove Distro").color(Color::Error))
                        .on_click(cx.listener(move |_, _, window, cx| {
                            remove_wsl_distro(cx.entity(), index, distro_name.clone(), window, cx);
                            cx.focus_self(window);
                        })),
                )
        })
    }

    fn render_edit_ssh(
        &self,
        connection: &SshConnectionOptions,
        index: SshServerIndex,
        entries: &[NavigableEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let connection_string = SharedString::new(connection.host.to_string());

        v_flex()
            .child({
                let label = if connection.nickname.is_some() {
                    "Edit Nickname"
                } else {
                    "Add Nickname to Server"
                };
                div()
                    .id("ssh-options-add-nickname")
                    .track_focus(&entries[0].focus_handle)
                    .on_action(cx.listener(move |this, _: &menu::Confirm, window, cx| {
                        this.mode = Mode::EditNickname(EditNicknameState::new(index, window, cx));
                        cx.notify();
                    }))
                    .child(
                        ListItem::new("add-nickname")
                            .toggle_state(entries[0].focus_handle.contains_focused(window, cx))
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(Icon::new(IconName::Pencil).color(Color::Muted))
                            .child(Label::new(label))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.mode =
                                    Mode::EditNickname(EditNicknameState::new(index, window, cx));
                                cx.notify();
                            })),
                    )
            })
            .child({
                let workspace = self.workspace.clone();
                fn callback(
                    workspace: WeakEntity<Workspace>,
                    connection_string: SharedString,
                    cx: &mut App,
                ) {
                    cx.write_to_clipboard(ClipboardItem::new_string(connection_string.to_string()));
                    workspace
                        .update(cx, |this, cx| {
                            struct SshServerAddressCopiedToClipboard;
                            let notification = format!(
                                "Copied server address ({}) to clipboard",
                                connection_string
                            );

                            this.show_toast(
                                Toast::new(
                                    NotificationId::composite::<SshServerAddressCopiedToClipboard>(
                                        connection_string.clone(),
                                    ),
                                    notification,
                                )
                                .autohide(),
                                cx,
                            );
                        })
                        .ok();
                }
                div()
                    .id("ssh-options-copy-server-address")
                    .track_focus(&entries[1].focus_handle)
                    .on_action({
                        let connection_string = connection_string.clone();
                        let workspace = self.workspace.clone();
                        move |_: &menu::Confirm, _, cx| {
                            callback(workspace.clone(), connection_string.clone(), cx);
                        }
                    })
                    .child(
                        ListItem::new("copy-server-address")
                            .toggle_state(entries[1].focus_handle.contains_focused(window, cx))
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(Icon::new(IconName::Copy).color(Color::Muted))
                            .child(Label::new("Copy Server Address"))
                            .end_slot(Label::new(connection_string.clone()).color(Color::Muted))
                            .show_end_slot_on_hover()
                            .on_click({
                                let connection_string = connection_string.clone();
                                move |_, _, cx| {
                                    callback(workspace.clone(), connection_string.clone(), cx);
                                }
                            }),
                    )
            })
            .child({
                fn remove_ssh_server(
                    remote_servers: Entity<RemoteServerProjects>,
                    index: SshServerIndex,
                    connection_string: SharedString,
                    window: &mut Window,
                    cx: &mut App,
                ) {
                    let prompt_message = format!("Remove server `{}`?", connection_string);

                    let confirmation = window.prompt(
                        PromptLevel::Warning,
                        &prompt_message,
                        None,
                        &["Yes, remove it", "No, keep it"],
                        cx,
                    );

                    cx.spawn(async move |cx| {
                        if confirmation.await.ok() == Some(0) {
                            remote_servers.update(cx, |this, cx| {
                                this.delete_ssh_server(index, cx);
                            });
                            remote_servers.update(cx, |this, cx| {
                                this.mode = Mode::default_mode(&this.ssh_config_servers, cx);
                                cx.notify();
                            });
                        }
                        anyhow::Ok(())
                    })
                    .detach_and_log_err(cx);
                }
                div()
                    .id("ssh-options-copy-server-address")
                    .track_focus(&entries[2].focus_handle)
                    .on_action(cx.listener({
                        let connection_string = connection_string.clone();
                        move |_, _: &menu::Confirm, window, cx| {
                            remove_ssh_server(
                                cx.entity(),
                                index,
                                connection_string.clone(),
                                window,
                                cx,
                            );
                            cx.focus_self(window);
                        }
                    }))
                    .child(
                        ListItem::new("remove-server")
                            .toggle_state(entries[2].focus_handle.contains_focused(window, cx))
                            .inset(true)
                            .spacing(ui::ListItemSpacing::Sparse)
                            .start_slot(Icon::new(IconName::Trash).color(Color::Error))
                            .child(Label::new("Remove Server").color(Color::Error))
                            .on_click(cx.listener(move |_, _, window, cx| {
                                remove_ssh_server(
                                    cx.entity(),
                                    index,
                                    connection_string.clone(),
                                    window,
                                    cx,
                                );
                                cx.focus_self(window);
                            })),
                    )
            })
    }

    fn render_edit_nickname(
        &self,
        state: &EditNicknameState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(connection) = RemoteSettings::get_global(cx)
            .ssh_connections()
            .nth(state.index.0)
        else {
            return v_flex()
                .id("ssh-edit-nickname")
                .track_focus(&self.focus_handle(cx));
        };

        let connection_string = connection.host.clone();
        let nickname = connection.nickname.map(|s| s.into());

        v_flex()
            .id("ssh-edit-nickname")
            .track_focus(&self.focus_handle(cx))
            .child(
                SshConnectionHeader {
                    connection_string: connection_string.into(),
                    paths: Default::default(),
                    nickname,
                    is_wsl: false,
                    is_devcontainer: false,
                }
                .render(window, cx),
            )
            .child(
                h_flex()
                    .p_2()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
                    .child(state.editor.clone()),
            )
    }
}
