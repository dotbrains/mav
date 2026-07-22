use super::*;

impl DebugPanel {
    pub fn load(
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncWindowContext,
    ) -> Task<Result<Entity<Self>>> {
        cx.spawn(async move |cx| {
            workspace.update_in(cx, |workspace, window, cx| {
                let debug_panel = DebugPanel::new(workspace, window, cx);

                workspace.register_action(|workspace, _: &ClearAllBreakpoints, _, cx| {
                    workspace.project().read(cx).breakpoint_store().update(
                        cx,
                        |breakpoint_store, cx| {
                            breakpoint_store.clear_breakpoints(cx);
                        },
                    )
                });

                workspace.set_debugger_provider(DebuggerProvider(debug_panel.clone()));
                workspace
                    .register_action({
                        let debug_panel = debug_panel.clone();
                        move |workspace, _: &ToggleFocus, window, cx| {
                            DebugPanel::open(debug_panel.clone(), workspace, window, cx);
                        }
                    })
                    .register_action({
                        let debug_panel = debug_panel.clone();
                        move |workspace, _: &mav_actions::debug_panel::Toggle, window, cx| {
                            DebugPanel::open(debug_panel.clone(), workspace, window, cx);
                        }
                    })
                    .register_action({
                        let debug_panel = debug_panel.clone();
                        move |workspace: &mut Workspace, _: &crate::Start, window, cx| {
                            DebugPanel::open(debug_panel.clone(), workspace, window, cx);
                            NewProcessModal::show(
                                workspace,
                                window,
                                NewProcessMode::Debug,
                                None,
                                cx,
                            );
                        }
                    })
                    .register_action({
                        let debug_panel = debug_panel.clone();
                        move |workspace: &mut Workspace, _: &crate::Rerun, window, cx| {
                            DebugPanel::open(debug_panel.clone(), workspace, window, cx);
                            debug_panel.update(cx, |debug_panel, cx| {
                                debug_panel.rerun_last_session(workspace, window, cx);
                            })
                        }
                    });

                debug_panel
            })
        })
    }

    pub fn open(
        debug_panel: Entity<Self>,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if !workspace.activate_item(&debug_panel, true, true, window, cx) {
            workspace.add_item_to_active_pane(Box::new(debug_panel), None, true, window, cx);
        }
    }

    pub fn start_session(
        &mut self,
        scenario: DebugScenario,
        task_context: SharedTaskContext,
        active_buffer: Option<Entity<Buffer>>,
        worktree_id: Option<WorktreeId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dap_store = self.project.read(cx).dap_store();
        let Some(adapter) = DapRegistry::global(cx).adapter(&scenario.adapter) else {
            return;
        };
        let quirks = SessionQuirks {
            compact: adapter.compact_child_session(),
            prefer_thread_name: adapter.prefer_thread_name(),
        };
        let session = dap_store.update(cx, |dap_store, cx| {
            dap_store.new_session(
                Some(scenario.label.clone()),
                DebugAdapterName(scenario.adapter.clone()),
                task_context.clone(),
                None,
                quirks,
                cx,
            )
        });
        let worktree = worktree_id.or_else(|| {
            active_buffer
                .as_ref()
                .and_then(|buffer| buffer.read(cx).file())
                .map(|f| f.worktree_id(cx))
        });

        let Some(worktree) = worktree
            .and_then(|id| self.project.read(cx).worktree_for_id(id, cx))
            .or_else(|| self.project.read(cx).visible_worktrees(cx).next())
        else {
            log::debug!("Could not find a worktree to spawn the debug session in");
            return;
        };

        self.debug_scenario_scheduled_last = true;
        if let Some(inventory) = self
            .project
            .read(cx)
            .task_store()
            .read(cx)
            .task_inventory()
            .cloned()
        {
            inventory.update(cx, |inventory, _| {
                inventory.scenario_scheduled(
                    scenario.clone(),
                    task_context.clone(),
                    worktree_id,
                    active_buffer.as_ref().map(|buffer| buffer.downgrade()),
                );
            })
        }
        let task = cx.spawn_in(window, {
            let session = session.clone();
            async move |this, cx| {
                let debug_session =
                    Self::register_session(this.clone(), session.clone(), true, cx).await?;
                let definition = debug_session
                    .update_in(cx, |debug_session, window, cx| {
                        debug_session.running_state().update(cx, |running, cx| {
                            if scenario.build.is_some() {
                                running.scenario = Some(scenario.clone());
                                running.scenario_context = Some(DebugScenarioContext {
                                    active_buffer: active_buffer
                                        .as_ref()
                                        .map(|entity| entity.downgrade()),
                                    task_context: task_context.clone(),
                                    worktree_id,
                                });
                            };
                            running.resolve_scenario(
                                scenario,
                                task_context,
                                active_buffer,
                                worktree_id,
                                window,
                                cx,
                            )
                        })
                    })?
                    .await?;
                dap_store
                    .update(cx, |dap_store, cx| {
                        dap_store.boot_session(session.clone(), definition, worktree, cx)
                    })
                    .await
            }
        });

        let boot_task = cx.spawn({
            let session = session.clone();

            async move |_, cx| {
                if let Err(error) = task.await {
                    let redacted_error = redact_command(&format!("{error:#}"));
                    log::error!("{redacted_error}");
                    session
                        .update(cx, |session, cx| {
                            session
                                .console_output(cx)
                                .unbounded_send(format!("error: {:#}", redacted_error))
                                .ok();
                            session.shutdown(cx)
                        })
                        .await;
                }
                anyhow::Ok(())
            }
        });

        session.update(cx, |session, _| match &mut session.state {
            SessionState::Booting(state_task) => {
                *state_task = Some(boot_task);
            }
            SessionState::Running(_) => {
                debug_panic!("Session state should be in building because we are just starting it");
            }
        });
    }

    pub(crate) fn rerun_last_session(
        &mut self,
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task_store = workspace.project().read(cx).task_store().clone();
        let Some(task_inventory) = task_store.read(cx).task_inventory() else {
            return;
        };
        let workspace = self.workspace.clone();
        let Some((scenario, context)) = task_inventory.read(cx).last_scheduled_scenario().cloned()
        else {
            window.defer(cx, move |window, cx| {
                workspace
                    .update(cx, |workspace, cx| {
                        NewProcessModal::show(workspace, window, NewProcessMode::Debug, None, cx);
                    })
                    .ok();
            });
            return;
        };

        let DebugScenarioContext {
            task_context,
            worktree_id,
            active_buffer,
        } = context;

        let active_buffer = active_buffer.and_then(|buffer| buffer.upgrade());

        self.start_session(
            scenario,
            task_context,
            active_buffer,
            worktree_id,
            window,
            cx,
        );
    }
}
