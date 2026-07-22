use super::*;

pub async fn open_remote_worktree(
    connection_options: remote::RemoteConnectionOptions,
    paths: Vec<PathBuf>,
    app_state: Arc<workspace::AppState>,
    workspace: gpui::WeakEntity<Workspace>,
    cx: &mut gpui::AsyncWindowContext,
) -> anyhow::Result<()> {
    let connect_task = workspace.update_in(cx, |workspace, window, cx| {
        workspace.toggle_modal(window, cx, |window, cx| {
            remote_connection::RemoteConnectionModal::new(
                &connection_options,
                Vec::new(),
                window,
                cx,
            )
        });

        let prompt = workspace
            .active_modal::<remote_connection::RemoteConnectionModal>(cx)
            .expect("Modal just created")
            .read(cx)
            .prompt
            .clone();

        remote_connection::connect(
            remote::remote_client::ConnectionIdentifier::setup(),
            connection_options.clone(),
            prompt,
            window,
            cx,
        )
        .prompt_err("Failed to connect", window, cx, |_, _, _| None)
    })?;

    let session = connect_task.await;

    workspace
        .update_in(cx, |workspace, _window, cx| {
            if let Some(prompt) =
                workspace.active_modal::<remote_connection::RemoteConnectionModal>(cx)
            {
                prompt.update(cx, |prompt, cx| prompt.finished(cx))
            }
        })
        .ok();

    let Some(Some(session)) = session else {
        return Ok(());
    };

    let new_project = cx.update(|_, cx| {
        project::Project::remote(
            session,
            app_state.client.clone(),
            app_state.node_runtime.clone(),
            app_state.user_store.clone(),
            app_state.languages.clone(),
            app_state.fs.clone(),
            true,
            cx,
        )
    })?;

    let workspace_position = cx
        .update(|_, cx| {
            workspace::remote_workspace_position_from_db(connection_options.clone(), &paths, cx)
        })?
        .await
        .context("fetching workspace position from db")?;

    let mut options =
        cx.update(|_, cx| (app_state.build_window_options)(workspace_position.display, cx))?;
    options.window_bounds = workspace_position.window_bounds;

    let new_window = cx.open_window(options, |window, cx| {
        let workspace = cx.new(|cx| {
            let mut workspace =
                Workspace::new(None, new_project.clone(), app_state.clone(), window, cx);
            workspace.centered_layout = workspace_position.centered_layout;
            workspace
        });
        cx.new(|cx| MultiWorkspace::new(workspace, window, cx))
    })?;

    workspace::open_remote_project_with_existing_connection(
        connection_options,
        new_project,
        paths,
        app_state,
        new_window,
        None,
        None,
        cx,
    )
    .await?;

    Ok(())
}
