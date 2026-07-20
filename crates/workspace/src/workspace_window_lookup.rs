use super::{AppState, MultiWorkspace, OpenMode, Workspace, WorkspaceLocation};
use crate::persistence::model::SerializedWorkspaceLocation;
use anyhow::Context as _;
use gpui::{App, AsyncApp, WindowHandle};
use remote::RemoteConnectionOptions;
use std::sync::Arc;

pub fn activate_any_workspace_window(cx: &mut AsyncApp) -> Option<WindowHandle<MultiWorkspace>> {
    cx.update(|cx| {
        if let Some(workspace_window) = cx
            .active_window()
            .and_then(|window| window.downcast::<MultiWorkspace>())
        {
            return Some(workspace_window);
        }

        for window in cx.windows() {
            if let Some(workspace_window) = window.downcast::<MultiWorkspace>() {
                workspace_window
                    .update(cx, |_, window, _| window.activate_window())
                    .ok();
                return Some(workspace_window);
            }
        }
        None
    })
}

pub async fn get_any_active_multi_workspace(
    app_state: Arc<AppState>,
    mut cx: AsyncApp,
) -> anyhow::Result<WindowHandle<MultiWorkspace>> {
    // find an existing workspace to focus and show call controls
    let active_window = activate_any_workspace_window(&mut cx);
    if active_window.is_none() {
        cx.update(|cx| {
            Workspace::new_local(
                vec![],
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await?;
    }
    activate_any_workspace_window(&mut cx).context("could not open mav")
}

pub fn workspace_windows_for_location(
    serialized_location: &SerializedWorkspaceLocation,
    cx: &App,
) -> Vec<WindowHandle<MultiWorkspace>> {
    cx.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<MultiWorkspace>())
        .filter(|multi_workspace| {
            let same_host = |left: &RemoteConnectionOptions, right: &RemoteConnectionOptions| {
                match (left, right) {
                    (RemoteConnectionOptions::Ssh(a), RemoteConnectionOptions::Ssh(b)) => {
                        (&a.host, &a.username, &a.port) == (&b.host, &b.username, &b.port)
                    }
                    (RemoteConnectionOptions::Wsl(a), RemoteConnectionOptions::Wsl(b)) => {
                        a.distro_name == b.distro_name
                    }
                    (RemoteConnectionOptions::Docker(a), RemoteConnectionOptions::Docker(b)) => {
                        a.container_id == b.container_id
                    }
                    #[cfg(any(test, feature = "test-support"))]
                    (RemoteConnectionOptions::Mock(a), RemoteConnectionOptions::Mock(b)) => {
                        a.id == b.id
                    }
                    _ => false,
                }
            };

            multi_workspace.read(cx).is_ok_and(|multi_workspace| {
                multi_workspace.workspaces().any(|workspace| {
                    match workspace.read(cx).workspace_location(cx) {
                        WorkspaceLocation::Location(location, _) => {
                            match (&location, serialized_location) {
                                (
                                    SerializedWorkspaceLocation::Local,
                                    SerializedWorkspaceLocation::Local,
                                ) => true,
                                (
                                    SerializedWorkspaceLocation::Remote(a),
                                    SerializedWorkspaceLocation::Remote(b),
                                ) => same_host(a, b),
                                _ => false,
                            }
                        }
                        _ => false,
                    }
                })
            })
        })
        .collect()
}
