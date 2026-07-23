use super::*;

pub fn init(cx: &mut App) {
    register_serializable_item::<TerminalView>(cx);

    cx.observe_new(|workspace: &mut Workspace, window, cx| {
        let terminal_provider =
            cx.new(|_| terminal_provider::WorkspaceTerminalProviderState::new(workspace));
        workspace.set_terminal_provider(terminal_provider::WorkspaceTerminalProvider(
            terminal_provider,
        ));
        workspace.register_action(TerminalView::deploy);
        workspace.register_action(new_terminal);
        workspace.register_action(open_terminal);
        if let Some(window) = window
            && let Some((database_id, serialization_key)) = workspace
                .database_id()
                .zip(persistence::terminal_panel_serialization_key(workspace))
        {
            persistence::migrate_legacy_terminal_panel(
                workspace.weak_handle(),
                database_id,
                serialization_key,
                workspace.project().clone(),
                window,
                cx,
            )
            .detach_and_log_err(cx);
        }
    })
    .detach();
}

pub(crate) fn new_terminal(
    workspace: &mut Workspace,
    action: &NewTerminal,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let local = action.local;
    let working_directory = default_working_directory(workspace, cx);
    add_terminal_to_active_pane(workspace, window, cx, move |project, cx| {
        if local {
            project.create_local_terminal(cx)
        } else {
            project.create_terminal_shell(working_directory, cx)
        }
    })
    .detach_and_log_err(cx);
}

pub(crate) fn open_terminal(
    workspace: &mut Workspace,
    action: &OpenTerminal,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let local = action.local;
    let working_directory = action.working_directory.clone();
    add_terminal_to_active_pane(workspace, window, cx, move |project, cx| {
        if local {
            project.create_local_terminal(cx)
        } else {
            project.create_terminal_shell(Some(working_directory), cx)
        }
    })
    .detach_and_log_err(cx);
}

pub(crate) fn add_terminal_to_active_pane<F>(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    create_terminal: F,
) -> Task<Result<WeakEntity<Terminal>>>
where
    F: FnOnce(&mut Project, &mut Context<Project>) -> Task<Result<Entity<Terminal>>> + 'static,
{
    if !is_enabled_in_workspace(workspace, cx) {
        return Task::ready(Err(anyhow!(
            "terminal not yet supported for collaborative projects"
        )));
    }

    let project = workspace.project().downgrade();

    cx.spawn_in(window, async move |workspace, cx| {
        let terminal = project.update(cx, create_terminal)?.await;
        workspace.update_in(cx, |workspace, window, cx| match terminal {
            Ok(terminal) => {
                let pane = workspace.active_pane().clone();
                add_terminal_to_workspace(workspace, pane, terminal.clone(), true, window, cx);
                Ok(terminal.downgrade())
            }
            Err(error) => {
                let pane = workspace.active_pane().clone();
                let failed_to_spawn = Box::new(cx.new(|cx| FailedToSpawnTerminal {
                    error: error.to_string(),
                    focus_handle: cx.focus_handle(),
                }));
                workspace.add_item(pane, failed_to_spawn, None, true, true, window, cx);
                Err(error)
            }
        })?
    })
}

pub(crate) fn add_terminal_to_workspace(
    workspace: &mut Workspace,
    pane: Entity<Pane>,
    terminal: Entity<Terminal>,
    focus: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let terminal_view = Box::new(cx.new(|cx| {
        TerminalView::new(
            terminal,
            workspace.weak_handle(),
            workspace.database_id(),
            workspace.project().downgrade(),
            window,
            cx,
        )
    }));
    workspace.add_item(pane, terminal_view, None, focus, focus, window, cx);
}

pub(crate) fn select_terminal_target_pane(workspace: &Workspace, cx: &App) -> Entity<Pane> {
    let active_pane = workspace.active_pane().clone();
    if pane_contains_terminal(&active_pane, cx) {
        return active_pane;
    }

    workspace
        .panes()
        .iter()
        .find(|pane| pane_contains_terminal(pane, cx))
        .cloned()
        .unwrap_or(active_pane)
}

fn pane_contains_terminal(pane: &Entity<Pane>, cx: &App) -> bool {
    pane.read(cx)
        .items()
        .any(|item| item.act_as::<TerminalView>(cx).is_some())
}

pub(crate) fn is_enabled_in_workspace(workspace: &Workspace, cx: &App) -> bool {
    workspace.project().read(cx).supports_terminal(cx)
}
