use super::*;

/// Collects every `Workspace` entity across all open `MultiWorkspace` windows.
pub fn all_open_workspaces(cx: &App) -> Vec<Entity<Workspace>> {
    cx.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<MultiWorkspace>())
        .flat_map(|multi_workspace| {
            multi_workspace
                .read(cx)
                .map(|multi_workspace| multi_workspace.workspaces().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .collect()
}

pub fn workspaces_for_archive(
    multi_workspace: Option<&Entity<MultiWorkspace>>,
    cx: &App,
) -> Vec<Entity<Workspace>> {
    let mut workspaces = multi_workspace
        .map(|multi_workspace| {
            multi_workspace
                .read(cx)
                .workspaces()
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for workspace in all_open_workspaces(cx) {
        if !workspaces.contains(&workspace) {
            workspaces.push(workspace);
        }
    }
    workspaces
}

fn current_app_state(cx: &mut AsyncApp) -> Option<Arc<AppState>> {
    cx.update(|cx| {
        all_open_workspaces(cx)
            .into_iter()
            .next()
            .map(|workspace| workspace.read(cx).app_state().clone())
    })
}
