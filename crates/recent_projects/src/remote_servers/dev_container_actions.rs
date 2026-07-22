use super::*;

impl RemoteServerProjects {
    fn init_dev_container_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let configs = self
            .workspace
            .read_with(cx, |workspace, cx| find_devcontainer_configs(workspace, cx))
            .unwrap_or_default();

        if configs.len() > 1 {
            let delegate = DevContainerPickerDelegate::new(configs, cx.weak_entity());
            self.dev_container_picker =
                Some(cx.new(|cx| Picker::uniform_list(delegate, window, cx).embedded()));

            let state =
                CreateRemoteDevContainer::new(DevContainerCreationProgress::SelectingConfig, cx);
            self.mode = Mode::CreateRemoteDevContainer(state);
            cx.notify();
        } else if let Some((app_state, context)) = self
            .workspace
            .read_with(cx, |workspace, cx| {
                let app_state = workspace.app_state().clone();
                let context = DevContainerContext::from_workspace(workspace, cx)?;
                Some((app_state, context))
            })
            .ok()
            .flatten()
        {
            let config = configs.into_iter().next();
            self.open_dev_container(config, app_state, context, window, cx);
            self.view_in_progress_dev_container(window, cx);
        } else {
            log::error!("No active project directory for Dev Container");
        }
    }

    fn open_dev_container(
        &self,
        config: Option<DevContainerConfig>,
        app_state: Arc<AppState>,
        context: DevContainerContext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let replace_window = window.window_handle().downcast::<MultiWorkspace>();
        let app_state = Arc::downgrade(&app_state);

        cx.spawn_in(window, async move |entity, cx| {
            let environment = context.environment(cx).await;

            let (dev_container_connection, starting_dir) =
                match start_dev_container_with_config(context, config, environment).await {
                    Ok((c, s)) => (c, s),
                    Err(e) => {
                        log::error!("Failed to start dev container: {:?}", e);
                        cx.prompt(
                            gpui::PromptLevel::Critical,
                            "Failed to start Dev Container. See logs for details",
                            Some(&format!("{e}")),
                            &["OK"],
                        )
                        .await
                        .ok();
                        entity
                            .update_in(cx, |remote_server_projects, window, cx| {
                                remote_server_projects.allow_dismissal = true;
                                remote_server_projects.mode =
                                    Mode::CreateRemoteDevContainer(CreateRemoteDevContainer::new(
                                        DevContainerCreationProgress::Error(format!("{e}")),
                                        cx,
                                    ));
                                remote_server_projects.focus_handle(cx).focus(window, cx);
                            })
                            .ok();
                        return;
                    }
                };
            cx.update(|_, cx| {
                ExtensionStore::global(cx).update(cx, |this, cx| {
                    for extension in &dev_container_connection.extension_ids {
                        log::info!("Installing extension {extension} from devcontainer");
                        this.install_latest_extension(Arc::from(extension.clone()), cx);
                    }
                })
            })
            .log_err();

            entity
                .update(cx, |this, cx| {
                    this.allow_dismissal = true;
                    cx.emit(DismissEvent);
                })
                .log_err();

            let Some(app_state) = app_state.upgrade() else {
                return;
            };
            let result = open_remote_project(
                Connection::DevContainer(dev_container_connection).into(),
                vec![starting_dir].into_iter().map(PathBuf::from).collect(),
                app_state,
                OpenOptions {
                    requesting_window: replace_window,
                    ..OpenOptions::default()
                },
                cx,
            )
            .await;
            if let Err(e) = result {
                log::error!("Failed to connect: {e:#}");
                cx.prompt(
                    gpui::PromptLevel::Critical,
                    "Failed to connect",
                    Some(&e.to_string()),
                    &["OK"],
                )
                .await
                .ok();
            }
        })
        .detach();
    }
}
