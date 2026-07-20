use super::{
    AppState, MultiWorkspace, MultiWorkspaceState, OpenMode, OpenResult, PathList, ProjectGroupKey,
    SerializedProjectGroupState, Workspace, open_workspace_by_id,
};
use crate::persistence::model::SerializedMultiWorkspace;
use gpui::{AsyncApp, WindowHandle};
use std::sync::Arc;

pub async fn restore_multiworkspace(
    multi_workspace: SerializedMultiWorkspace,
    app_state: Arc<AppState>,
    cx: &mut AsyncApp,
) -> anyhow::Result<WindowHandle<MultiWorkspace>> {
    let SerializedMultiWorkspace {
        active_workspace,
        state,
    } = multi_workspace;

    let workspace_result = if active_workspace.paths.is_empty() {
        cx.update(|cx| {
            open_workspace_by_id(active_workspace.workspace_id, app_state.clone(), None, cx)
        })
        .await
    } else {
        cx.update(|cx| {
            Workspace::new_local(
                active_workspace.paths.paths().to_vec(),
                app_state.clone(),
                None,
                None,
                None,
                OpenMode::Activate,
                cx,
            )
        })
        .await
        .map(|result| result.window)
    };

    let window_handle = match workspace_result {
        Ok(handle) => handle,
        Err(err) => {
            log::error!("Failed to restore active workspace: {err:#}");

            let mut fallback_handle = None;
            for key in &state.project_groups {
                let key: ProjectGroupKey = key.clone().into();
                let paths = key.path_list().paths().to_vec();
                match cx
                    .update(|cx| {
                        Workspace::new_local(
                            paths,
                            app_state.clone(),
                            None,
                            None,
                            None,
                            OpenMode::Activate,
                            cx,
                        )
                    })
                    .await
                {
                    Ok(OpenResult { window, .. }) => {
                        fallback_handle = Some(window);
                        break;
                    }
                    Err(fallback_err) => {
                        log::error!("Fallback project group also failed: {fallback_err:#}");
                    }
                }
            }

            fallback_handle.ok_or(err)?
        }
    };

    apply_restored_multiworkspace_state(window_handle, &state, app_state.fs.clone(), cx).await;

    window_handle
        .update(cx, |_, window, _cx| {
            window.activate_window();
        })
        .ok();

    Ok(window_handle)
}

pub async fn apply_restored_multiworkspace_state(
    window_handle: WindowHandle<MultiWorkspace>,
    state: &MultiWorkspaceState,
    fs: Arc<dyn fs::Fs>,
    cx: &mut AsyncApp,
) {
    let MultiWorkspaceState {
        sidebar_open,
        project_groups,
        sidebar_state,
        ..
    } = state;

    if !project_groups.is_empty() {
        // Resolve linked worktree paths to their main repo paths so
        // stale keys from previous sessions get normalized and deduped.
        let mut resolved_groups: Vec<SerializedProjectGroupState> = Vec::new();
        for serialized in project_groups.iter().cloned() {
            let SerializedProjectGroupState { key, expanded } = serialized.into_restored_state();
            if key.path_list().paths().is_empty() {
                continue;
            }
            let mut resolved_paths = Vec::new();
            for path in key.path_list().paths() {
                if key.host().is_none()
                    && let Some(common_dir) =
                        project::discover_root_repo_common_dir(path, fs.as_ref()).await
                {
                    let main_path = project::repo_identity_path(&common_dir);
                    resolved_paths.push(main_path.to_path_buf());
                } else {
                    resolved_paths.push(path.to_path_buf());
                }
            }
            let resolved = ProjectGroupKey::new(key.host(), PathList::new(&resolved_paths));
            if !resolved_groups.iter().any(|g| g.key == resolved) {
                resolved_groups.push(SerializedProjectGroupState {
                    key: resolved,
                    expanded,
                });
            }
        }

        window_handle
            .update(cx, |multi_workspace, _window, cx| {
                multi_workspace.restore_project_groups(resolved_groups, cx);
            })
            .ok();
    }

    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.restore_sidebar_open_state(*sidebar_open, window, cx);
        })
        .ok();

    if let Some(sidebar_state) = sidebar_state {
        window_handle
            .update(cx, |multi_workspace, window, cx| {
                multi_workspace.restore_sidebar_serialized_state(sidebar_state.clone(), window, cx);
            })
            .ok();
    }
}

pub(super) fn apply_restored_sidebar_state(
    window_handle: WindowHandle<MultiWorkspace>,
    state: &MultiWorkspaceState,
    cx: &mut AsyncApp,
) {
    window_handle
        .update(cx, |multi_workspace, window, cx| {
            multi_workspace.restore_sidebar_open_state(state.sidebar_open, window, cx);
            if let Some(sidebar_state) = &state.sidebar_state {
                multi_workspace.restore_sidebar_serialized_state(sidebar_state.clone(), window, cx);
            }
        })
        .ok();
}
