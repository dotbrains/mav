use super::*;

impl RunningState {
    pub(crate) fn resolve_scenario(
        &self,
        scenario: DebugScenario,
        task_context: SharedTaskContext,
        buffer: Option<Entity<Buffer>>,
        worktree_id: Option<WorktreeId>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<DebugTaskDefinition>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Err(anyhow!("no workspace")));
        };
        let project = workspace.read(cx).project().clone();
        let dap_store = project.read(cx).dap_store().downgrade();
        let dap_registry = cx.global::<DapRegistry>().clone();
        let task_store = project.read(cx).task_store().downgrade();
        let weak_project = project.downgrade();
        let weak_workspace = workspace.downgrade();
        let is_windows = project.read(cx).path_style(cx).is_windows();
        let remote_shell = project
            .read(cx)
            .remote_client()
            .as_ref()
            .and_then(|remote| remote.read(cx).shell());

        cx.spawn_in(window, async move |this, cx| {
            let DebugScenario {
                adapter,
                label,
                build,
                mut config,
                tcp_connection,
            } = scenario;
            Self::relativize_paths(None, &mut config, &task_context);
            Self::substitute_variables_in_config(&mut config, &task_context);

            if Self::contains_substring(&config, PROCESS_ID_PLACEHOLDER.as_str()) || label.as_ref().contains(PROCESS_ID_PLACEHOLDER.as_str()) {
                let (tx, rx) = futures::channel::oneshot::channel::<Option<i32>>();

                let weak_workspace_clone = weak_workspace.clone();
                weak_workspace.update_in(cx, |workspace, window, cx| {
                    let project = workspace.project().clone();
                    workspace.toggle_modal(window, cx, |window, cx| {
                        AttachModal::new(
                            ModalIntent::ResolveProcessId(Some(tx)),
                            weak_workspace_clone,
                            project,
                            true,
                            window,
                            cx,
                        )
                    });
                }).ok();

                let Some(process_id) = rx.await.ok().flatten() else {
                    bail!("No process selected with config that contains {}", PROCESS_ID_PLACEHOLDER.as_str())
                };

                Self::substitute_process_id_in_config(&mut config, process_id);
            }

            let request_type = match dap_registry
                .adapter(&adapter)
                .with_context(|| format!("{}: is not a valid adapter name", &adapter)) {
                    Ok(adapter) => adapter.request_kind(&config).await,
                    Err(e) => Err(e)
                };


            let config_is_valid = request_type.is_ok();
            let mut extra_config = Value::Null;
            let build_output = if let Some(build) = build {
                let (task_template, locator_name) = match build {
                    BuildTaskDefinition::Template {
                        task_template,
                        locator_name,
                    } => (task_template, locator_name),
                    BuildTaskDefinition::ByName(ref label) => {
                        let task = task_store.update(cx, |this, cx| {
                            this.task_inventory().map(|inventory| {
                                inventory.read(cx).task_template_by_label(
                                    buffer,
                                    worktree_id,
                                    label,
                                    cx,
                                )
                            })
                        })?;
                        let task = match task {
                            Some(task) => task.await,
                            None => None,
                        }.with_context(|| format!("Couldn't find task template for {build:?}"))?;
                        (task, None)
                    }
                };
                let Some(mut task) = task_template.resolve_task("debug-build-task", &task_context) else {
                    anyhow::bail!("Could not resolve task variables within a debug scenario");
                };

                let locator_name = if let Some(locator_name) = locator_name {
                    extra_config = config.clone();
                    debug_assert!(!config_is_valid);
                    Some(locator_name)
                } else if !config_is_valid {
                    let task = dap_store
                        .update(cx, |this, cx| {
                            this.debug_scenario_for_build_task(
                                task.original_task().clone(),
                                adapter.clone().into(),
                                task.display_label().to_owned().into(),
                                cx,
                            )

                        });
                    if let Ok(t) = task {
                        t.await.and_then(|scenario| {
                            extra_config = scenario.config;
                            match scenario.build {
                                Some(BuildTaskDefinition::Template {
                                    locator_name, ..
                                }) => locator_name,
                                _ => None,
                            }
                        })
                    } else {
                        None
                    }

                } else {
                    None
                };

                if let Some(remote_shell) = remote_shell && task.resolved.shell == Shell::System {
                    task.resolved.shell = Shell::Program(remote_shell);
                }

                let builder = ShellBuilder::new(&task.resolved.shell, is_windows);
                let command_label = builder.command_label(task.resolved.command.as_deref().unwrap_or(""));
                let (command, args) =
                    builder.build(task.resolved.command.clone(), &task.resolved.args);

                let task_with_shell = SpawnInTerminal {
                    command_label,
                    command: Some(command),
                    args,
                    ..task.resolved.clone()
                };

                Workspace::save_for_task(&weak_workspace, task_with_shell.save, cx).await;

                let terminal = project
                    .update(cx, |project, cx| {
                        project.create_terminal_task(
                            task_with_shell.clone(),
                            cx,
                        )
                    }).await?;

                let terminal_view = cx.new_window_entity(|window, cx| {
                    TerminalView::new(
                        terminal.clone(),
                        weak_workspace,
                        None,
                        weak_project,
                        window,
                        cx,
                    )
                })?;

                this.update_in(cx, |this, window, cx| {
                    this.ensure_pane_item(DebuggerPaneItem::Terminal, window, cx);
                    this.debug_terminal.update(cx, |debug_terminal, cx| {
                        debug_terminal.terminal = Some(terminal_view);
                        cx.notify();
                    });
                })?;

                let exit_status = terminal
                    .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
                    .await
                    .context("Failed to wait for completed task")?;

                if !exit_status.success() {
                    anyhow::bail!("Build failed");
                }
                Some((task.resolved.clone(), locator_name, extra_config))
            } else {
                None
            };

            if config_is_valid {
            } else if let Some((task, locator_name, extra_config)) = build_output {
                let locator_name =
                    locator_name.with_context(|| {
                        format!("Could not find a valid locator for a build task and configure is invalid with error: {}", request_type.err()
                            .map(|err| err.to_string())
                            .unwrap_or_default())
                    })?;
                let request = dap_store
                    .update(cx, |this, cx| {
                        this.run_debug_locator(&locator_name, task, cx)
                    })?
                    .await?;

                let mav_config = MavDebugConfig {
                    label: label.clone(),
                    adapter: adapter.clone(),
                    request,
                    stop_on_entry: None,
                };

                let scenario = dap_registry
                    .adapter(&adapter)
                    .with_context(|| anyhow!("{}: is not a valid adapter name", &adapter))?.config_from_mav_format(mav_config)
                    .await?;
                config = scenario.config;
                util::merge_non_null_json_value_into(extra_config, &mut config);

                Self::substitute_variables_in_config(&mut config, &task_context);
            } else {
                let Err(e) = request_type else {
                    unreachable!();
                };
                anyhow::bail!("Mav cannot determine how to run this debug scenario. `build` field was not provided and Debug Adapter won't accept provided configuration because: {e}");
            };

            Ok(DebugTaskDefinition {
                label,
                adapter: DebugAdapterName(adapter),
                config,
                tcp_connection,
            })
        })
    }

    fn handle_run_in_terminal(
        &self,
        request: &RunInTerminalRequestArguments,
        mut sender: mpsc::Sender<Result<u32>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let running = cx.entity();
        let Ok(project) = self
            .workspace
            .read_with(cx, |workspace, _| workspace.project().clone())
        else {
            return Task::ready(Err(anyhow!("no workspace")));
        };
        let session = self.session.read(cx);

        let cwd = (!request.cwd.is_empty())
            .then(|| PathBuf::from(&request.cwd))
            .or_else(|| session.binary().unwrap().cwd.clone());

        let mut envs: HashMap<String, String> =
            self.session.read(cx).task_context().project_env.clone();
        if let Some(Value::Object(env)) = &request.env {
            for (key, value) in env {
                let value_str = match (key.as_str(), value) {
                    (_, Value::String(value)) => value,
                    _ => continue,
                };

                envs.insert(key.clone(), value_str.clone());
            }
        }

        let mut args = request.args.clone();
        let command = if envs.contains_key("VSCODE_INSPECTOR_OPTIONS") {
            // Handle special case for NodeJS debug adapter
            // If the Node binary path is provided (possibly with arguments like --experimental-network-inspection),
            // we set the command to None
            // This prevents the NodeJS REPL from appearing, which is not the desired behavior
            // The expected usage is for users to provide their own Node command, e.g., `node test.js`
            // This allows the NodeJS debug client to attach correctly
            if args
                .iter()
                .filter(|arg| !arg.starts_with("--"))
                .collect::<Vec<_>>()
                .len()
                > 1
            {
                Some(args.remove(0))
            } else {
                None
            }
        } else if !args.is_empty() {
            Some(args.remove(0))
        } else {
            None
        };

        let shell = project.read(cx).terminal_settings(&cwd, cx).shell.clone();
        let title = request
            .title
            .clone()
            .filter(|title| !title.is_empty())
            .or_else(|| command.clone())
            .unwrap_or_else(|| "Debug terminal".to_string());
        let kind = task::SpawnInTerminal {
            id: task::TaskId("debug".to_string()),
            full_label: title.clone(),
            label: title.clone(),
            command,
            args,
            command_label: title,
            cwd,
            env: envs,
            use_new_terminal: true,
            allow_concurrent_runs: true,
            reveal: task::RevealStrategy::NoFocus,
            reveal_target: task::RevealTarget::Dock,
            hide: task::HideStrategy::Never,
            shell,
            show_summary: false,
            show_command: false,
            show_rerun: false,
            save: task::SaveStrategy::default(),
        };

        let workspace = self.workspace.clone();
        let weak_project = project.downgrade();

        let terminal_task =
            project.update(cx, |project, cx| project.create_terminal_task(kind, cx));
        let terminal_task = cx.spawn_in(window, async move |_, cx| {
            let terminal = terminal_task.await?;

            let terminal_view = cx.new_window_entity(|window, cx| {
                TerminalView::new(terminal.clone(), workspace, None, weak_project, window, cx)
            })?;

            running.update_in(cx, |running, window, cx| {
                running.ensure_pane_item(DebuggerPaneItem::Terminal, window, cx);
                running.debug_terminal.update(cx, |debug_terminal, cx| {
                    debug_terminal.terminal = Some(terminal_view);
                    cx.notify();
                });
            })?;

            terminal.read_with(cx, |terminal, _| {
                terminal
                    .pid()
                    .map(|pid| pid.as_u32())
                    .context("Terminal was spawned but PID was not available")
            })
        });

        cx.background_spawn(async move { anyhow::Ok(sender.send(terminal_task.await).await?) })
    }
}
