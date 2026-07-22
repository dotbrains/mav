use super::*;
use collections::HashMap;
use futures::{channel::oneshot, future::join_all};
use gpui::AsyncApp;
use std::process::ExitStatus;

pub(super) struct WorkspaceTerminalProvider(pub(super) Entity<WorkspaceTerminalProviderState>);

impl workspace::TerminalProvider for WorkspaceTerminalProvider {
    fn spawn(
        &self,
        task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Option<Result<ExitStatus>>> {
        let terminal_provider = self.0.clone();
        window.spawn(cx, async move |cx| {
            let terminal = terminal_provider
                .update_in(cx, |terminal_provider, window, cx| {
                    terminal_provider.spawn_task(&task, window, cx)
                })
                .ok()?
                .await;
            match terminal {
                Ok(terminal) => {
                    let exit_status = terminal
                        .read_with(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
                        .ok()?
                        .await?;
                    Some(Ok(exit_status))
                }
                Err(error) => Some(Err(error)),
            }
        })
    }
}

pub(super) struct WorkspaceTerminalProviderState {
    workspace: WeakEntity<Workspace>,
    deferred_tasks: HashMap<TaskId, Task<()>>,
}

impl WorkspaceTerminalProviderState {
    pub(super) fn new(workspace: &Workspace) -> Self {
        Self {
            workspace: workspace.weak_handle(),
            deferred_tasks: HashMap::default(),
        }
    }

    fn spawn_task(
        &mut self,
        task: &SpawnInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Task::ready(Err(anyhow!("failed to read workspace")));
        };

        let project = workspace.read(cx).project().read(cx);

        if project.is_via_collab() {
            return Task::ready(Err(anyhow!("cannot spawn tasks as a guest")));
        }

        let remote_client = project.remote_client();
        let is_windows = project.path_style(cx).is_windows();
        let remote_shell = remote_client
            .as_ref()
            .and_then(|remote_client| remote_client.read(cx).shell());

        let shell = if let Some(remote_shell) = remote_shell
            && task.shell == Shell::System
        {
            Shell::Program(remote_shell)
        } else {
            task.shell.clone()
        };

        let task = terminal_panel::prepare_task_for_spawn(task, &shell, is_windows);

        if task.allow_concurrent_runs && task.use_new_terminal {
            return self.spawn_in_new_terminal(task, window, cx);
        }

        let mut terminals_for_task = self.terminals_for_task(&task.full_label, cx);
        let Some((_, existing_terminal)) = terminals_for_task.pop() else {
            return self.spawn_in_new_terminal(task, window, cx);
        };

        if task.allow_concurrent_runs {
            return self.replace_terminal(task, existing_terminal, window, cx);
        }

        let (tx, rx) = oneshot::channel();

        self.deferred_tasks.insert(
            task.id.clone(),
            cx.spawn_in(window, async move |terminal_provider, cx| {
                wait_for_terminals_tasks(terminals_for_task, cx).await;
                let task = terminal_provider.update_in(cx, |terminal_provider, window, cx| {
                    if task.use_new_terminal {
                        terminal_provider.spawn_in_new_terminal(task, window, cx)
                    } else {
                        terminal_provider.replace_terminal(task, existing_terminal, window, cx)
                    }
                });
                if let Ok(task) = task {
                    tx.send(task.await).ok();
                }
            }),
        );

        cx.spawn(async move |_, _| rx.await?)
    }

    fn terminals_for_task(
        &self,
        label: &str,
        cx: &mut App,
    ) -> Vec<(Entity<Pane>, Entity<TerminalView>)> {
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };

        let panes = workspace.read(cx).panes().to_vec();
        let mut terminals = Vec::new();
        for pane in panes {
            let terminal_views = pane
                .read(cx)
                .items()
                .filter_map(|item| item.act_as::<TerminalView>(cx))
                .collect::<Vec<_>>();

            for terminal_view in terminal_views {
                let matches_label = terminal_view
                    .read(cx)
                    .terminal()
                    .read(cx)
                    .task()
                    .is_some_and(|task_state| task_state.spawned_task.full_label == label);

                if matches_label {
                    terminals.push((pane.clone(), terminal_view));
                }
            }
        }

        terminals.sort_by_key(|(_, terminal_view)| terminal_view.entity_id());
        terminals
    }

    fn spawn_in_new_terminal(
        &mut self,
        spawn_task: SpawnInTerminal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let reveal = spawn_task.reveal;
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |_, cx| {
            let project = workspace.update(cx, |workspace, cx| {
                if !is_enabled_in_workspace(workspace, cx) {
                    anyhow::bail!("terminal not yet supported for remote projects");
                }
                Ok(workspace.project().clone())
            })??;

            let terminal = project
                .update(cx, |project, cx| {
                    project.create_terminal_task(spawn_task, cx)
                })
                .await?;

            workspace.update_in(cx, |workspace, window, cx| {
                add_terminal_to_workspace(
                    workspace,
                    select_terminal_target_pane(workspace, cx),
                    terminal.clone(),
                    matches!(reveal, RevealStrategy::Always),
                    window,
                    cx,
                );
                Ok(terminal.downgrade())
            })?
        })
    }

    fn replace_terminal(
        &self,
        spawn_task: SpawnInTerminal,
        terminal_to_replace: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<WeakEntity<Terminal>>> {
        let reveal = spawn_task.reveal;
        let workspace = self.workspace.clone();
        cx.spawn_in(window, async move |_, cx| {
            let project = workspace.read_with(cx, |workspace, _| workspace.project().clone())?;
            let new_terminal = project
                .update(cx, |project, cx| {
                    project.create_terminal_task(spawn_task, cx)
                })
                .await?;

            terminal_to_replace.update_in(cx, |terminal_to_replace, window, cx| {
                terminal_to_replace.set_terminal(new_terminal.clone(), window, cx);
            })?;

            match reveal {
                RevealStrategy::Always => {
                    workspace.update_in(cx, |workspace, window, cx| {
                        let did_activate =
                            workspace.activate_item(&terminal_to_replace, true, true, window, cx);
                        anyhow::ensure!(did_activate, "failed to retrieve terminal pane");
                        anyhow::Ok(())
                    })??;
                }
                RevealStrategy::NoFocus => {
                    workspace.update_in(cx, |workspace, window, cx| {
                        workspace.activate_item(&terminal_to_replace, false, false, window, cx);
                    })?;
                }
                RevealStrategy::Never => {}
            }

            Ok(new_terminal.downgrade())
        })
    }
}

async fn wait_for_terminals_tasks(
    terminals_for_task: Vec<(Entity<Pane>, Entity<TerminalView>)>,
    cx: &mut AsyncApp,
) {
    let pending_tasks = terminals_for_task.iter().map(|(_, terminal)| {
        terminal.update(cx, |terminal_view, cx| {
            terminal_view
                .terminal()
                .update(cx, |terminal, cx| terminal.wait_for_completed_task(cx))
        })
    });
    join_all(pending_tasks).await;
}
