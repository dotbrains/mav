use super::*;

pub(super) fn open_about_window(cx: &mut App) {
    fn about_window_icon(release_channel: ReleaseChannel) -> Arc<Image> {
        let bytes = match release_channel {
            ReleaseChannel::Dev => include_bytes!("../../resources/app-icon-dev.png").as_slice(),
            ReleaseChannel::Nightly => {
                include_bytes!("../../resources/app-icon-nightly.png").as_slice()
            }
            ReleaseChannel::Preview => {
                include_bytes!("../../resources/app-icon-preview.png").as_slice()
            }
            ReleaseChannel::Stable => include_bytes!("../../resources/app-icon.png").as_slice(),
        };

        Arc::new(Image::from_bytes(ImageFormat::Png, bytes.to_vec()))
    }

    struct AboutWindow {
        focus_handle: FocusHandle,
        ok_entry: NavigableEntry,
        copy_entry: NavigableEntry,
        app_icon: Arc<Image>,
        message: SharedString,
        commit: Option<SharedString>,
        full_version: SharedString,
    }

    impl AboutWindow {
        fn new(cx: &mut Context<Self>) -> Self {
            let release_channel = ReleaseChannel::global(cx);
            let release_channel_name = release_channel.display_name();
            let full_version: SharedString = AppVersion::global(cx).to_string().into();
            let version = env!("CARGO_PKG_VERSION");

            let debug = if cfg!(debug_assertions) {
                "(debug)"
            } else {
                ""
            };
            let message: SharedString = format!("{release_channel_name} {version} {debug}").into();
            let commit = AppCommitSha::try_global(cx)
                .map(|sha| sha.full())
                .filter(|commit| !commit.is_empty())
                .map(SharedString::from);

            Self {
                focus_handle: cx.focus_handle(),
                ok_entry: NavigableEntry::focusable(cx),
                copy_entry: NavigableEntry::focusable(cx),
                app_icon: about_window_icon(release_channel),
                message,
                commit,
                full_version,
            }
        }

        fn copy_details(&self, window: &mut Window, cx: &mut Context<Self>) {
            let content = match self.commit.as_ref() {
                Some(commit) => {
                    format!(
                        "{}\nCommit: {}\nVersion: {}",
                        self.message, commit, self.full_version
                    )
                }
                None => format!("{}\nVersion: {}", self.message, self.full_version),
            };
            cx.write_to_clipboard(ClipboardItem::new_string(content));
            window.remove_window();
        }
    }

    impl Render for AboutWindow {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let ok_is_focused = self.ok_entry.focus_handle.contains_focused(window, cx);
            let copy_is_focused = self.copy_entry.focus_handle.contains_focused(window, cx);

            Navigable::new(
                v_flex()
                    .id("about-window")
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|_, _: &menu::Cancel, window, _cx| {
                        window.remove_window();
                    }))
                    .min_w_0()
                    .size_full()
                    .bg(cx.theme().colors().editor_background)
                    .text_color(cx.theme().colors().text)
                    .p_4()
                    .when(cfg!(target_os = "macos"), |this| this.pt_10())
                    .gap_4()
                    .text_center()
                    .justify_between()
                    .child(
                        v_flex()
                            .w_full()
                            .gap_2()
                            .items_center()
                            .child(img(self.app_icon.clone()).size_16().flex_none())
                            .child(Headline::new(self.message.clone()))
                            .when_some(self.commit.clone(), |this, commit| {
                                this.child(
                                    Label::new("Commit")
                                        .color(Color::Muted)
                                        .size(LabelSize::XSmall),
                                )
                                .child(Label::new(commit).size(LabelSize::Small))
                            })
                            .child(
                                Label::new("Version")
                                    .color(Color::Muted)
                                    .size(LabelSize::XSmall),
                            )
                            .child(Label::new(self.full_version.clone()).size(LabelSize::Small)),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .gap_1()
                            .child(
                                div()
                                    .flex_1()
                                    .track_focus(&self.ok_entry.focus_handle)
                                    .on_action(cx.listener(|_, _: &menu::Confirm, window, _cx| {
                                        window.remove_window();
                                    }))
                                    .child(
                                        Button::new("ok", "OK")
                                            .full_width()
                                            .style(ButtonStyle::OutlinedGhost)
                                            .toggle_state(ok_is_focused)
                                            .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                            .on_click(cx.listener(|_, _, window, _cx| {
                                                window.remove_window();
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .track_focus(&self.copy_entry.focus_handle)
                                    .on_action(cx.listener(
                                        |this, _: &menu::Confirm, window, cx| {
                                            this.copy_details(window, cx);
                                        },
                                    ))
                                    .child(
                                        Button::new("copy", "Copy")
                                            .full_width()
                                            .style(ButtonStyle::Tinted(TintColor::Accent))
                                            .toggle_state(copy_is_focused)
                                            .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                                            .on_click(cx.listener(|this, _event, window, cx| {
                                                this.copy_details(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .into_any_element(),
            )
            .entry(self.ok_entry.clone())
            .entry(self.copy_entry.clone())
        }
    }

    impl Focusable for AboutWindow {
        fn focus_handle(&self, _cx: &App) -> FocusHandle {
            self.ok_entry.focus_handle.clone()
        }
    }

    // Don't open about window twice
    if let Some(existing) = cx
        .windows()
        .into_iter()
        .find_map(|w| w.downcast::<AboutWindow>())
    {
        existing
            .update(cx, |about_window, window, cx| {
                window.activate_window();
                about_window.ok_entry.focus_handle.focus(window, cx);
            })
            .log_err();
        return;
    }

    let window_size = Size {
        width: px(440.),
        height: px(300.),
    };

    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("About Mav".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.), px(12.))),
            }),
            window_bounds: Some(WindowBounds::centered(window_size, cx)),
            is_resizable: false,
            is_minimizable: false,
            kind: WindowKind::Floating,
            app_id: Some(ReleaseChannel::global(cx).app_id().to_owned()),
            ..Default::default()
        },
        |window, cx| {
            let about_window = cx.new(AboutWindow::new);
            let focus_handle = about_window.read(cx).ok_entry.focus_handle.clone();
            window.activate_window();
            focus_handle.focus(window, cx);
            about_window
        },
    )
    .log_err();
}

#[cfg(not(target_os = "windows"))]
pub(super) fn install_cli(
    _: &mut Workspace,
    _: &install_cli::InstallCliBinary,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    install_cli::install_cli_binary(window, cx)
}

static WAITING_QUIT_CONFIRMATION: AtomicBool = AtomicBool::new(false);
pub(super) fn quit(_: &Quit, cx: &mut App) {
    if WAITING_QUIT_CONFIRMATION.load(atomic::Ordering::Acquire) {
        return;
    }

    let should_confirm = WorkspaceSettings::get_global(cx).confirm_quit;
    cx.spawn(async move |cx| {
        let mut workspace_windows: Vec<WindowHandle<MultiWorkspace>> = cx.update(|cx| {
            cx.windows()
                .into_iter()
                .filter_map(|window| window.downcast::<MultiWorkspace>())
                .collect::<Vec<_>>()
        });

        // If multiple windows have unsaved changes, and need a save prompt,
        // prompt in the active window before switching to a different window.
        cx.update(|cx| {
            workspace_windows.sort_by_key(|window| window.is_active(cx) == Some(false));
        });

        if should_confirm && let Some(multi_workspace) = workspace_windows.first() {
            let answer = multi_workspace
                .update(cx, |_, window, cx| {
                    window.prompt(
                        PromptLevel::Info,
                        "Are you sure you want to quit?",
                        None,
                        &["Quit", "Cancel"],
                        cx,
                    )
                })
                .log_err();

            if let Some(answer) = answer {
                WAITING_QUIT_CONFIRMATION.store(true, atomic::Ordering::Release);
                let answer = answer.await.ok();
                WAITING_QUIT_CONFIRMATION.store(false, atomic::Ordering::Release);
                if answer != Some(0) {
                    return Ok(());
                }
            }
        }

        // If the user cancels any save prompt, then keep the app open.
        for window in &workspace_windows {
            let window = *window;
            let active_and_workspaces = window
                .update(cx, |multi_workspace, _, _cx| {
                    (
                        multi_workspace.workspace().clone(),
                        multi_workspace.workspaces().cloned().collect::<Vec<_>>(),
                    )
                })
                .log_err();

            let Some((originally_active, workspaces)) = active_and_workspaces else {
                continue;
            };

            for workspace in workspaces {
                if let Some(should_close) = window
                    .update(cx, |multi_workspace, window, cx| {
                        multi_workspace.activate(workspace.clone(), None, window, cx);
                        window.activate_window();
                        workspace.update(cx, |workspace, cx| {
                            workspace.prepare_to_close(CloseIntent::Quit, window, cx)
                        })
                    })
                    .log_err()
                {
                    if !should_close.await? {
                        // Activating each workspace above to surface its save
                        // prompts changed which workspace is active. Restore the
                        // user's focused workspace before bailing so the window
                        // is left as they had it.
                        window
                            .update(cx, |multi_workspace, window, cx| {
                                multi_workspace.activate(
                                    originally_active.clone(),
                                    None,
                                    window,
                                    cx,
                                );
                            })
                            .log_err();
                        return Ok(());
                    }
                }
            }

            // The loop above activated each workspace in turn, overwriting the
            // persisted active workspace. Re-activate the workspace the user
            // actually had focused so it is the one serialized (and restored on
            // next launch) as active, rather than whichever happened to be last.
            window
                .update(cx, |multi_workspace, window, cx| {
                    multi_workspace.activate(originally_active, None, window, cx);
                })
                .log_err();
        }
        // Flush all pending workspace serialization before quitting so that
        // session_id/window_id are up-to-date in the database.
        let mut flush_tasks = Vec::new();
        for window in &workspace_windows {
            window
                .update(cx, |multi_workspace, window, cx| {
                    for workspace in multi_workspace.workspaces() {
                        flush_tasks.push(workspace.update(cx, |workspace, cx| {
                            workspace.flush_serialization(window, cx)
                        }));
                    }
                    flush_tasks.append(&mut multi_workspace.take_pending_removal_tasks());
                    flush_tasks.push(multi_workspace.flush_serialization());
                })
                .log_err();
        }
        futures::future::join_all(flush_tasks).await;

        cx.update(|cx| cx.quit());
        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}
