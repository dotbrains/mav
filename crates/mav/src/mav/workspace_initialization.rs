use super::*;

fn bind_on_window_closed(cx: &mut App) -> Option<gpui::Subscription> {
    #[cfg(target_os = "macos")]
    {
        WorkspaceSettings::get_global(cx)
            .on_last_window_closed
            .is_quit_app()
            .then(|| {
                cx.on_window_closed(|cx, _window_id| {
                    if cx.windows().is_empty() {
                        cx.quit();
                    }
                })
            })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Some(cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        }))
    }
}

pub fn build_window_options(display_uuid: Option<Uuid>, cx: &mut App) -> WindowOptions {
    let display = display_uuid.and_then(|uuid| {
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().ok() == Some(uuid))
    });
    let app_id = ReleaseChannel::global(cx).app_id();
    let window_decorations = match std::env::var("MAV_WINDOW_DECORATIONS") {
        Ok(val) if val == "server" => gpui::WindowDecorations::Server,
        Ok(val) if val == "client" => gpui::WindowDecorations::Client,
        _ => match WorkspaceSettings::get_global(cx).window_decorations {
            settings::WindowDecorations::Server => gpui::WindowDecorations::Server,
            settings::WindowDecorations::Client => gpui::WindowDecorations::Client,
        },
    };

    let use_system_window_tabs = WorkspaceSettings::get_global(cx).use_system_window_tabs;

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    static APP_ICON: std::sync::LazyLock<Option<std::sync::Arc<image::RgbaImage>>> =
        std::sync::LazyLock::new(|| {
            // this shouldn't fail since decode is checked in build.rs
            const BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app_icon.png"));
            util::maybe!({
                let image = image::ImageReader::new(std::io::Cursor::new(BYTES))
                    .with_guessed_format()?
                    .decode()?
                    .into();
                anyhow::Ok(Arc::new(image))
            })
            .log_err()
        });

    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: None,
            appears_transparent: true,
            traffic_light_position: Some(point(px(9.0), px(9.0))),
        }),
        window_bounds: None,
        focus: false,
        show: false,
        kind: WindowKind::Normal,
        // On macOS, Mav handles window movement itself, so disable AppKit's titlebar dragging.
        // On other platforms, `is_movable` gates native window dragging (e.g. Windows'
        // `HTCAPTION` hit test), so it must remain `true`.
        is_movable: cfg!(not(target_os = "macos")),
        display_id: display.map(|display| display.id()),
        window_background: cx.theme().window_background_appearance(),
        app_id: Some(app_id.to_owned()),
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        icon: APP_ICON.as_ref().cloned(),
        window_decorations: Some(window_decorations),
        window_min_size: Some(gpui::Size {
            width: px(360.0),
            height: px(240.0),
        }),
        tabbing_identifier: if use_system_window_tabs {
            Some(String::from("mav"))
        } else {
            None
        },
        ..Default::default()
    }
}

pub fn initialize_workspace(app_state: Arc<AppState>, cx: &mut App) {
    let mut _on_close_subscription = bind_on_window_closed(cx);
    cx.observe_global::<SettingsStore>(move |cx| {
        // A 1.92 regression causes unused-assignment to trigger on this variable.
        _ = _on_close_subscription.is_some();
        _on_close_subscription = bind_on_window_closed(cx);
    })
    .detach();

    init_cursor_hide_mode(cx);

    cx.observe_new(|_multi_workspace: &mut MultiWorkspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        #[cfg(feature = "track-project-leak")]
        {
            let multi_workspace_handle = cx.weak_entity();
            let workspace_handle = _multi_workspace.workspace().downgrade();
            let project_handle = _multi_workspace.workspace().read(cx).project().downgrade();
            let window_id_2 = window.window_handle().window_id();
            cx.on_window_closed(move |cx, window_id| {
                let multi_workspace_handle = multi_workspace_handle.clone();
                let workspace_handle = workspace_handle.clone();
                let project_handle = project_handle.clone();
                if window_id != window_id_2 {
                    return;
                }
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1500))
                        .await;

                    multi_workspace_handle.assert_released();
                    workspace_handle.assert_released();
                    project_handle.assert_released();
                })
                .detach();
            })
            .detach();
        }

        cx.spawn_in(window, async move |_this, cx| {
            const TELEMETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);
            loop {
                cx.background_executor().timer(TELEMETRY_INTERVAL).await;
                if cx
                    .update(|window, cx| {
                        input_latency_ui::report_input_latency_telemetry(window, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let multi_workspace_handle = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            multi_workspace_handle
                .update(cx, |multi_workspace, cx| {
                    // We'll handle closing asynchronously
                    multi_workspace.close_window(&CloseWindow, window, cx);
                    false
                })
                .unwrap_or(true)
        });

        let window_handle = window.window_handle();
        let multi_workspace_handle = cx.entity();
        cx.subscribe_in(
            &multi_workspace_handle,
            window,
            |this, _multi_workspace, event: &workspace::MultiWorkspaceEvent, window, cx| {
                let workspace::MultiWorkspaceEvent::ActiveWorkspaceChanged { source_workspace } =
                    event
                else {
                    return;
                };

                let active_workspace = this.workspace().clone();
                let source_workspace = source_workspace.clone();
                active_workspace.update(cx, |workspace, cx| {
                    if let Some(ref source) = source_workspace {
                        if let Some(panel) = workspace.panel::<agent_ui::AgentPanel>(cx) {
                            panel.update(cx, |panel, cx| {
                                panel.initialize_from_source_workspace_if_needed(
                                    source.clone(),
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                });
            },
        )
        .detach();

        cx.defer(move |cx| {
            window_handle
                .update(cx, |_, window, cx| {
                    let sidebar =
                        cx.new(|cx| Sidebar::new(multi_workspace_handle.clone(), window, cx));
                    multi_workspace_handle.update(cx, |multi_workspace, cx| {
                        multi_workspace.register_sidebar(sidebar, window, cx);
                    });
                })
                .ok();
        });
    })
    .detach();

    cx.observe_new(move |workspace: &mut Workspace, window, cx| {
        let Some(window) = window else {
            return;
        };

        let workspace_handle = cx.entity();
        let center_pane = workspace.active_pane().clone();
        initialize_pane(workspace, &center_pane, window, cx);

        cx.subscribe_in(&workspace_handle, window, {
            move |workspace, _, event, window, cx| match event {
                workspace::Event::PaneAdded(pane) => {
                    initialize_pane(workspace, pane, window, cx);
                }
                workspace::Event::OpenBundledFile {
                    text,
                    title,
                    language,
                } => open_bundled_file(workspace, text.clone(), title, language, window, cx),
                _ => {}
            }
        })
        .detach();

        #[cfg(not(any(test, target_os = "macos")))]
        initialize_file_watcher(window, cx);

        if let Some(specs) = window.gpu_specs() {
            log::info!("Using GPU: {:?}", specs);
            show_software_emulation_warning_if_needed(specs.clone(), window, cx);
            if let Some(crash_client) = cx.try_global::<CrashHandler>() {
                crashes::set_gpu_info(&crash_client.0, specs);
            }
        }

        let edit_prediction_menu_handle = PopoverMenuHandle::default();
        let edit_prediction_ui = cx.new(|cx| {
            edit_prediction_ui::EditPredictionButton::new(
                app_state.fs.clone(),
                app_state.user_store.clone(),
                edit_prediction_menu_handle.clone(),
                workspace.project().clone(),
                cx,
            )
        });
        workspace.register_action({
            move |_, _: &edit_prediction_ui::ToggleMenu, window, cx| {
                edit_prediction_menu_handle.toggle(window, cx);
            }
        });

        let search_button = cx.new(|_| search::search_status_button::SearchButton::new());
        let diagnostic_summary =
            cx.new(|cx| diagnostics::items::DiagnosticIndicator::new(workspace, cx));
        let active_file_name = cx.new(|_| workspace::active_file_name::ActiveFileName::new());
        let activity_indicator = activity_indicator::ActivityIndicator::new(
            workspace,
            workspace.project().read(cx).languages().clone(),
            window,
            cx,
        );
        let active_buffer_encoding =
            cx.new(|_| encoding_selector::ActiveBufferEncoding::new(workspace));
        let active_buffer_language =
            cx.new(|_| language_selector::ActiveBufferLanguage::new(workspace));
        let active_toolchain_language =
            cx.new(|cx| toolchain_selector::ActiveToolchain::new(workspace, window, cx));
        let vim_mode_indicator = cx.new(|cx| vim::ModeIndicator::new(window, cx));
        let image_info = cx.new(|_cx| ImageInfo::new(workspace));

        let lsp_button_menu_handle = PopoverMenuHandle::default();
        let lsp_button =
            cx.new(|cx| LspButton::new(workspace, lsp_button_menu_handle.clone(), window, cx));
        workspace.register_action({
            move |_, _: &lsp_button::ToggleMenu, window, cx| {
                lsp_button_menu_handle.toggle(window, cx);
            }
        });

        let cursor_position =
            cx.new(|_| go_to_line::cursor_position::CursorPosition::new(workspace));
        let line_ending_indicator =
            cx.new(|_| line_ending_selector::LineEndingIndicator::default());
        let git_blame_status = cx.new(|_| git_ui::GitBlameStatus::default());
        let merge_conflict_indicator =
            cx.new(|cx| git_ui::MergeConflictIndicator::new(workspace, cx));
        workspace.status_bar().update(cx, |status_bar, cx| {
            status_bar.add_left_item(search_button, window, cx);
            status_bar.add_left_item(lsp_button, window, cx);
            status_bar.add_left_item(diagnostic_summary, window, cx);
            status_bar.add_left_item(active_file_name, window, cx);
            status_bar.add_left_item(git_blame_status, window, cx);
            status_bar.add_left_item(merge_conflict_indicator, window, cx);
            status_bar.add_left_item(activity_indicator, window, cx);
            status_bar.add_right_item(edit_prediction_ui, window, cx);
            status_bar.add_right_item(active_buffer_encoding, window, cx);
            status_bar.add_right_item(active_buffer_language, window, cx);
            status_bar.add_right_item(active_toolchain_language, window, cx);
            status_bar.add_right_item(line_ending_indicator, window, cx);
            status_bar.add_right_item(vim_mode_indicator, window, cx);
            status_bar.add_right_item(cursor_position, window, cx);
            status_bar.add_right_item(image_info, window, cx);
        });

        let panels_task = initialize_panels(window, cx);
        workspace.set_panels_task(panels_task);
        register_actions(app_state.clone(), workspace, window, cx);

        if !workspace.has_active_modal(window, cx) {
            workspace.focus_handle(cx).focus(window, cx);
        }
    })
    .detach();
}
