use super::*;
use futures::channel::mpsc::UnboundedReceiver;

pub(crate) fn handle_open_request(request: OpenRequest, app_state: Arc<AppState>, cx: &mut App) {
    if let Some(kind) = request.kind {
        match kind {
            OpenRequestKind::CliConnection(connection) => {
                cx.spawn(async move |cx| handle_cli_connection(connection, app_state, cx).await)
                    .detach();
            }
            OpenRequestKind::FocusApp => {
                cx.spawn(async move |cx| {
                    if workspace::activate_any_workspace_window(cx).is_some() {
                        return anyhow::Ok(());
                    }
                    restore_or_create_workspace(app_state, cx).await
                })
                .detach_and_log_err(cx);
            }
            OpenRequestKind::Extension { extension_id } => {
                cx.spawn(async move |cx| {
                    let workspace =
                        workspace::get_any_active_multi_workspace(app_state, cx.clone()).await?;
                    workspace.update(cx, |_, window, cx| {
                        window.dispatch_action(
                            Box::new(mav_actions::Extensions {
                                category_filter: None,
                                id: Some(extension_id),
                            }),
                            cx,
                        );
                    })
                })
                .detach_and_log_err(cx);
            }
            OpenRequestKind::AgentPanel {
                external_source_prompt,
            } => {
                cx.spawn(async move |cx| {
                    let multi_workspace =
                        workspace::get_any_active_multi_workspace(app_state, cx.clone()).await?;

                    let panels_task = multi_workspace.update(cx, |multi_workspace, _, cx| {
                        multi_workspace
                            .workspace()
                            .update(cx, |workspace, _| workspace.take_panels_task())
                    })?;
                    if let Some(task) = panels_task {
                        task.await.log_err();
                    }

                    multi_workspace.update(cx, |multi_workspace, window, cx| {
                        multi_workspace.workspace().update(cx, |workspace, cx| {
                            if let Some(panel) = workspace.focus_panel::<AgentPanel>(window, cx) {
                                panel.update(cx, |panel, cx| {
                                    panel.new_agent_thread_with_external_source_prompt(
                                        external_source_prompt,
                                        window,
                                        cx,
                                    );
                                });
                            } else {
                                log::warn!(
                                    "mav://agent received but the AgentPanel is not registered \
                                     (is `disable_ai` enabled?)"
                                );
                            }
                        });
                    })
                })
                .detach_and_log_err(cx);
            }
            OpenRequestKind::InstallSkill { content } => {
                cx.spawn(async move |cx| {
                    let multi_workspace =
                        workspace::get_any_active_multi_workspace(app_state, cx.clone()).await?;

                    multi_workspace.update(cx, |_multi_workspace, _window, cx| {
                        settings_ui::open_skill_creator(
                            settings_ui::pages::SkillCreatorOpenMode::Install { content },
                            Some(multi_workspace),
                            cx,
                        );
                    })
                })
                .detach_and_log_err(cx);
            }
            OpenRequestKind::DockMenuAction { index } => {
                cx.perform_dock_menu_action(index);
            }
            OpenRequestKind::BuiltinJsonSchema { schema_path } => {
                workspace::with_active_or_new_workspace(cx, |_workspace, window, cx| {
                    cx.spawn_in(window, async move |workspace, cx| {
                        let res = async move {
                            let json = app_state.languages.language_for_name("JSONC").await.ok();
                            let lsp_store = workspace.update(cx, |workspace, cx| {
                                workspace
                                    .project()
                                    .update(cx, |project, _| project.lsp_store())
                            })?;
                            let uri = format!("mav://schemas/{}", schema_path);
                            let json_schema_content =
                                json_schema_store::handle_schema_request(lsp_store, uri, cx)
                                    .await?;
                            let json_schema_value: serde_json::Value =
                                serde_json::from_str(&json_schema_content)
                                    .context("Failed to parse JSON Schema")?;
                            let json_schema_content =
                                serde_json::to_string_pretty(&json_schema_value)
                                    .context("Failed to serialize JSON Schema as JSON")?;
                            let buffer_task = workspace.update(cx, |workspace, cx| {
                                workspace.project().update(cx, |project, cx| {
                                    project.create_buffer(json, false, cx)
                                })
                            })?;

                            let buffer = buffer_task.await?;

                            workspace.update_in(cx, |workspace, window, cx| {
                                buffer.update(cx, |buffer, cx| {
                                    buffer.edit([(0..0, json_schema_content)], None, cx);
                                    buffer.edit(
                                        [(0..0, format!("// {} JSON Schema\n", schema_path))],
                                        None,
                                        cx,
                                    );
                                });

                                workspace.add_item_to_active_pane(
                                    Box::new(cx.new(|cx| {
                                        let mut editor =
                                            editor::Editor::for_buffer(buffer, None, window, cx);
                                        editor.set_read_only(true);
                                        editor
                                    })),
                                    None,
                                    true,
                                    window,
                                    cx,
                                );
                            })
                        }
                        .await;
                        res.context("Failed to open builtin JSON Schema").log_err();
                    })
                    .detach();
                });
            }
            OpenRequestKind::Setting { setting_path } => {
                // mav://settings/languages/$(language)/tab_size  - DONT SUPPORT
                // mav://settings/languages/Rust/tab_size  - SUPPORT
                // languages.$(language).tab_size
                // [ languages $(language) tab_size]
                cx.spawn(async move |cx| {
                    let workspace =
                        workspace::get_any_active_multi_workspace(app_state, cx.clone()).await?;

                    workspace.update(cx, |_, window, cx| match setting_path {
                        None => window.dispatch_action(Box::new(mav_actions::OpenSettings), cx),
                        Some(setting_path) => window.dispatch_action(
                            Box::new(mav_actions::OpenSettingsAt {
                                path: setting_path,
                                target: None,
                            }),
                            cx,
                        ),
                    })
                })
                .detach_and_log_err(cx);
            }
            OpenRequestKind::GitClone { repo_url } => {
                workspace::with_active_or_new_workspace(cx, |_workspace, window, cx| {
                    if window.is_window_active() {
                        clone_and_open(
                            repo_url,
                            cx.weak_entity(),
                            window,
                            cx,
                            Arc::new(|workspace: &mut workspace::Workspace, window, cx| {
                                workspace.focus_panel::<ProjectPanel>(window, cx);
                            }),
                        );
                        return;
                    }

                    let subscription = Rc::new(RefCell::new(None));
                    subscription.replace(Some(cx.observe_in(&cx.entity(), window, {
                        let subscription = subscription.clone();
                        let repo_url = repo_url;
                        move |_, workspace_entity, window, cx| {
                            if window.is_window_active() && subscription.take().is_some() {
                                clone_and_open(
                                    repo_url.clone(),
                                    workspace_entity.downgrade(),
                                    window,
                                    cx,
                                    Arc::new(|workspace: &mut workspace::Workspace, window, cx| {
                                        workspace.focus_panel::<ProjectPanel>(window, cx);
                                    }),
                                );
                            }
                        }
                    })));
                });
            }
            OpenRequestKind::GitCommit { sha } => {
                let base_open_options = mav::open_options_for_request(
                    request.open_behavior,
                    &workspace::SerializedWorkspaceLocation::Local,
                    cx,
                );
                cx.spawn(async move |cx| {
                    let paths_with_position =
                        derive_paths_with_position(app_state.fs.as_ref(), request.open_paths).await;
                    let (workspace, _results) = open_paths_with_positions(
                        &paths_with_position,
                        &[],
                        false,
                        app_state,
                        base_open_options,
                        cx,
                    )
                    .await?;

                    workspace
                        .update(cx, |multi_workspace, window, cx| {
                            multi_workspace
                                .workspace()
                                .clone()
                                .update(cx, |workspace, cx| {
                                    let Some(repo) =
                                        workspace.project().read(cx).active_repository(cx)
                                    else {
                                        log::error!("no active repository found for commit view");
                                        return Err(anyhow::anyhow!("no active repository found"));
                                    };

                                    git_ui::commit_view::CommitView::open(
                                        sha,
                                        repo.downgrade(),
                                        workspace.weak_handle(),
                                        None,
                                        None,
                                        window,
                                        cx,
                                    );
                                    Ok(())
                                })
                        })
                        .log_err();

                    anyhow::Ok(())
                })
                .detach_and_log_err(cx);
            }
        }

        return;
    }

    if let Some(connection_options) = request.remote_connection {
        let open_behavior = request.open_behavior;
        let location = workspace::SerializedWorkspaceLocation::Remote(connection_options.clone());
        let base_open_options = mav::open_options_for_request(open_behavior, &location, cx);
        cx.spawn(async move |cx| {
            let paths: Vec<PathBuf> = request.open_paths.into_iter().map(PathBuf::from).collect();
            open_remote_project(connection_options, paths, app_state, base_open_options, cx).await
        })
        .detach_and_log_err(cx);
        return;
    }

    let mut task = None;
    let dev_container = request.dev_container;
    if !request.open_paths.is_empty() || !request.diff_paths.is_empty() {
        let app_state = app_state.clone();
        let base_open_options = mav::open_options_for_request(
            request.open_behavior,
            &workspace::SerializedWorkspaceLocation::Local,
            cx,
        );
        task = Some(cx.spawn(async move |cx| {
            let paths_with_position =
                derive_paths_with_position(app_state.fs.as_ref(), request.open_paths).await;
            let (_window, results) = open_paths_with_positions(
                &paths_with_position,
                &request.diff_paths,
                request.diff_all,
                app_state,
                workspace::OpenOptions {
                    open_in_dev_container: dev_container,
                    ..base_open_options
                },
                cx,
            )
            .await?;
            for result in results.into_iter().flatten() {
                if let Err(err) = result {
                    log::error!("Error opening path: {err:#}");
                }
            }
            anyhow::Ok(())
        }));
    }

    if !request.open_channel_notes.is_empty() || request.join_channel.is_some() {
        cx.spawn(async move |cx| {
            let result = maybe!(async {
                if let Some(task) = task {
                    task.await?;
                }
                let client = app_state.client.clone();
                // we continue even if connection fails as join_channel/ open channel notes will
                // show a visible error message.
                client.connect(true, cx).await.into_response().log_err();

                if let Some(channel_id) = request.join_channel {
                    cx.update(|cx| {
                        workspace::join_channel(
                            client::ChannelId(channel_id),
                            app_state.clone(),
                            None,
                            None,
                            cx,
                        )
                    })
                    .await?;
                }

                let workspace_window =
                    workspace::get_any_active_multi_workspace(app_state, cx.clone()).await?;

                let workspace = workspace_window.read_with(cx, |mw, _| mw.workspace().clone())?;
                let weak_workspace = workspace.downgrade();

                let mut promises = Vec::new();
                for (channel_id, heading) in request.open_channel_notes {
                    promises.push(cx.update_window(workspace_window.into(), |_, window, cx| {
                        ChannelView::open(
                            client::ChannelId(channel_id),
                            heading,
                            workspace.clone(),
                            window,
                            cx,
                        )
                    })?)
                }
                for result in future::join_all(promises).await {
                    result.notify_workspace_async_err(weak_workspace.clone(), cx);
                }
                anyhow::Ok(())
            })
            .await;
            if let Err(err) = result {
                fail_to_open_window_async(err, cx);
            }
        })
        .detach()
    } else if let Some(task) = task {
        cx.spawn(async move |cx| {
            if let Err(err) = task.await {
                fail_to_open_window_async(err, cx);
            }
        })
        .detach();
    }
}

pub(crate) fn handle_initial_open_requests(
    args: Args,
    open_listener: OpenListener,
    mut open_rx: UnboundedReceiver<RawOpenRequest>,
    app_state: Arc<AppState>,
    cx: &mut App,
) {
    let urls: Vec<_> = args
        .paths_or_urls
        .iter()
        .map(|arg| parse_url_arg(arg, cx))
        .collect();

    // Check if any diff paths are directories to determine diff_all mode
    let diff_all_mode = args
        .diff
        .chunks(2)
        .any(|pair| Path::new(&pair[0]).is_dir() || Path::new(&pair[1]).is_dir());

    let diff_paths: Vec<[String; 2]> = args
        .diff
        .chunks(2)
        .map(|chunk| [chunk[0].clone(), chunk[1].clone()])
        .collect();

    #[cfg(target_os = "windows")]
    let wsl = args.wsl;
    #[cfg(not(target_os = "windows"))]
    let wsl = None;

    if !urls.is_empty() || !diff_paths.is_empty() {
        open_listener.open(RawOpenRequest {
            urls,
            diff_paths,
            wsl,
            diff_all: diff_all_mode,
            dev_container: args.dev_container,
            ..Default::default()
        })
    }

    let (current_session_id, last_session_id) = {
        let session = app_state.session.read(cx);
        (
            session.id().to_owned(),
            session.last_session_id().map(|id| id.to_owned()),
        )
    };

    let restore_task = match open_rx
        .try_recv()
        .ok()
        .and_then(|request| OpenRequest::parse(request, cx).log_err())
    {
        Some(request) if request.is_focus_app_only() => cx.spawn({
            let app_state = app_state.clone();
            async move |cx| {
                if let Err(e) = restore_or_create_workspace(app_state, cx).await {
                    fail_to_open_window_async(e, cx)
                }
            }
        }),
        Some(request) => {
            handle_open_request(request, app_state.clone(), cx);
            Task::ready(())
        }
        None => cx.spawn({
            let app_state = app_state.clone();
            async move |cx| {
                if let Err(e) = restore_or_create_workspace(app_state, cx).await {
                    fail_to_open_window_async(e, cx)
                }
            }
        }),
    };

    cx.spawn({
        let db = workspace::WorkspaceDb::global(cx);
        let fs = app_state.fs.clone();
        async move |_cx| {
            restore_task.await;
            db.garbage_collect_workspaces(
                fs.as_ref(),
                &current_session_id,
                last_session_id.as_deref(),
            )
            .await
        }
    })
    .detach_and_log_err(cx);

    let app_state = app_state.clone();

    component_preview::init(app_state.clone(), cx);

    cx.spawn(async move |cx| {
        while let Some(urls) = open_rx.next().await {
            cx.update(|cx| {
                if let Some(request) = OpenRequest::parse(urls, cx).log_err() {
                    handle_open_request(request, app_state.clone(), cx);
                }
            });
        }
    })
    .detach();
}
