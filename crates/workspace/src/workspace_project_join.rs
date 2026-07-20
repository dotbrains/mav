use super::*;

pub fn join_in_room_project(
    project_id: u64,
    follow_user_id: u64,
    app_state: Arc<AppState>,
    cx: &mut App,
) -> Task<Result<()>> {
    let windows = cx.windows();
    cx.spawn(async move |cx| {
        let existing_window_and_workspace: Option<(
            WindowHandle<MultiWorkspace>,
            Entity<Workspace>,
        )> = windows.into_iter().find_map(|window_handle| {
            window_handle
                .downcast::<MultiWorkspace>()
                .and_then(|window_handle| {
                    window_handle
                        .update(cx, |multi_workspace, _window, cx| {
                            multi_workspace
                                .workspaces()
                                .find(|workspace| {
                                    workspace.read(cx).project().read(cx).remote_id()
                                        == Some(project_id)
                                })
                                .map(|workspace| (window_handle, workspace.clone()))
                        })
                        .unwrap_or(None)
                })
        });

        let multi_workspace_window = if let Some((existing_window, target_workspace)) =
            existing_window_and_workspace
        {
            existing_window
                .update(cx, |multi_workspace, window, cx| {
                    multi_workspace.activate(target_workspace, None, window, cx);
                })
                .ok();
            existing_window
        } else {
            let active_call = cx.update(|cx| GlobalAnyActiveCall::global(cx).clone());
            let project = cx
                .update(|cx| {
                    active_call.0.join_project(
                        project_id,
                        app_state.languages.clone(),
                        app_state.fs.clone(),
                        cx,
                    )
                })
                .await?;

            let window_bounds_override = window_bounds_env_override();
            cx.update(|cx| {
                let mut options = (app_state.build_window_options)(None, cx);
                options.window_bounds = window_bounds_override.map(WindowBounds::Windowed);
                cx.open_window(options, |window, cx| {
                    let workspace = cx.new(|cx| {
                        Workspace::new(Default::default(), project, app_state.clone(), window, cx)
                    });
                    cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
                })
            })?
        };

        multi_workspace_window.update(cx, |multi_workspace, window, cx| {
            cx.activate(true);
            window.activate_window();

            // We set the active workspace above, so this is the correct workspace.
            let workspace = multi_workspace.workspace().clone();
            workspace.update(cx, |workspace, cx| {
                let follow_peer_id = GlobalAnyActiveCall::try_global(cx)
                    .and_then(|call| call.0.peer_id_for_user_in_room(follow_user_id, cx))
                    .or_else(|| {
                        // If we couldn't follow the given user, follow the host instead.
                        let collaborator = workspace
                            .project()
                            .read(cx)
                            .collaborators()
                            .values()
                            .find(|collaborator| collaborator.is_host)?;
                        Some(collaborator.peer_id)
                    });

                if let Some(follow_peer_id) = follow_peer_id {
                    workspace.follow(follow_peer_id, window, cx);
                }
            });
        })?;

        anyhow::Ok(())
    })
}

pub fn with_active_or_new_workspace(
    cx: &mut App,
    f: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + Send + 'static,
) {
    match cx
        .active_window()
        .and_then(|w| w.downcast::<MultiWorkspace>())
    {
        Some(multi_workspace) => {
            cx.defer(move |cx| {
                multi_workspace
                    .update(cx, |multi_workspace, window, cx| {
                        let workspace = multi_workspace.workspace().clone();
                        workspace.update(cx, |workspace, cx| f(workspace, window, cx));
                    })
                    .log_err();
            });
        }
        None => {
            let app_state = AppState::global(cx);
            open_new(
                OpenOptions::default(),
                app_state,
                cx,
                move |workspace, window, cx| f(workspace, window, cx),
            )
            .detach_and_log_err(cx);
        }
    }
}
