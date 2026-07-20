use anyhow::{Context as _, Result};
use client::ErrorExt;
use futures::channel::oneshot;
use gpui::{App, AppContext, AsyncApp, Entity, Task, WeakEntity, WindowHandle};
use project::{Project, ProjectPath};
use remote::{
    RemoteClientDelegate, RemoteConnection, RemoteConnectionOptions,
    remote_client::ConnectionIdentifier,
};
use std::{path::PathBuf, sync::Arc};

use crate::{
    AppState, ItemHandle, MultiWorkspace, ProjectGroupKey, Workspace, WorkspaceDb, WorkspaceId,
    open_items, remote_project_deserialize::deserialize_remote_project,
};

pub fn open_remote_project_with_new_connection(
    window: WindowHandle<MultiWorkspace>,
    remote_connection: Arc<dyn RemoteConnection>,
    cancel_rx: oneshot::Receiver<()>,
    delegate: Arc<dyn RemoteClientDelegate>,
    app_state: Arc<AppState>,
    paths: Vec<PathBuf>,
    cx: &mut App,
) -> Task<Result<Vec<Option<Box<dyn ItemHandle>>>>> {
    cx.spawn(async move |cx| {
        let (workspace_id, serialized_workspace) =
            deserialize_remote_project(remote_connection.connection_options(), paths.clone(), cx)
                .await?;

        let session = match cx
            .update(|cx| {
                remote::RemoteClient::new(
                    ConnectionIdentifier::Workspace(workspace_id.0),
                    remote_connection,
                    cancel_rx,
                    delegate,
                    cx,
                )
            })
            .await?
        {
            Some(result) => result,
            None => return Ok(Vec::new()),
        };

        let project = cx.update(|cx| {
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
        });

        open_remote_project_inner(
            project,
            paths,
            workspace_id,
            serialized_workspace,
            app_state,
            window,
            None,
            None,
            cx,
        )
        .await
    })
}

pub fn open_remote_project_with_existing_connection(
    connection_options: RemoteConnectionOptions,
    project: Entity<Project>,
    paths: Vec<PathBuf>,
    app_state: Arc<AppState>,
    window: WindowHandle<MultiWorkspace>,
    provisional_project_group_key: Option<ProjectGroupKey>,
    source_workspace: Option<WeakEntity<Workspace>>,
    cx: &mut AsyncApp,
) -> Task<Result<Vec<Option<Box<dyn ItemHandle>>>>> {
    cx.spawn(async move |cx| {
        let (workspace_id, serialized_workspace) =
            deserialize_remote_project(connection_options.clone(), paths.clone(), cx).await?;

        open_remote_project_inner(
            project,
            paths,
            workspace_id,
            serialized_workspace,
            app_state,
            window,
            provisional_project_group_key,
            source_workspace,
            cx,
        )
        .await
    })
}

async fn open_remote_project_inner(
    project: Entity<Project>,
    paths: Vec<PathBuf>,
    workspace_id: WorkspaceId,
    serialized_workspace: Option<crate::persistence::model::SerializedWorkspace>,
    app_state: Arc<AppState>,
    window: WindowHandle<MultiWorkspace>,
    provisional_project_group_key: Option<ProjectGroupKey>,
    source_workspace: Option<WeakEntity<Workspace>>,
    cx: &mut AsyncApp,
) -> Result<Vec<Option<Box<dyn ItemHandle>>>> {
    let db = cx.update(|cx| WorkspaceDb::global(cx));
    let toolchains = db.toolchains(workspace_id).await?;
    for (toolchain, worktree_path, path) in toolchains {
        project
            .update(cx, |this, cx| {
                let Some(worktree_id) =
                    this.find_worktree(&worktree_path, cx)
                        .and_then(|(worktree, rel_path)| {
                            if rel_path.is_empty() {
                                Some(worktree.read(cx).id())
                            } else {
                                None
                            }
                        })
                else {
                    return Task::ready(None);
                };

                this.activate_toolchain(ProjectPath { worktree_id, path }, toolchain, cx)
            })
            .await;
    }
    let mut project_paths_to_open = vec![];
    let mut project_path_errors = vec![];

    for path in paths {
        let result = cx
            .update(|cx| {
                Workspace::project_path_for_path(project.clone(), path.as_path(), true, cx)
            })
            .await;
        match result {
            Ok((_, project_path)) => {
                project_paths_to_open.push((path, Some(project_path)));
            }
            Err(error) => {
                project_path_errors.push(error);
            }
        };
    }

    if project_paths_to_open.is_empty() {
        return Err(project_path_errors.pop().context("no paths given")?);
    }

    let workspace = window.update(cx, |multi_workspace, window, cx| {
        let new_workspace = cx.new(|cx| {
            let mut workspace =
                Workspace::new(Some(workspace_id), project, app_state.clone(), window, cx);
            workspace.update_history(cx);

            if let Some(ref serialized) = serialized_workspace {
                workspace.centered_layout = serialized.centered_layout;
            }

            workspace
        });

        if let Some(project_group_key) = provisional_project_group_key.clone() {
            multi_workspace.activate_provisional_workspace(
                new_workspace.clone(),
                project_group_key,
                window,
                cx,
            );
        } else {
            multi_workspace.activate(new_workspace.clone(), source_workspace, window, cx);
        }
        new_workspace
    })?;

    let items = window
        .update(cx, |_, window, cx| {
            window.activate_window();
            workspace.update(cx, |_workspace, cx| {
                open_items(serialized_workspace, project_paths_to_open, window, cx)
            })
        })?
        .await?;

    workspace.update(cx, |workspace, cx| {
        for error in project_path_errors {
            if error.error_code() == client::proto::ErrorCode::DevServerProjectPathDoesNotExist {
                if let Some(path) = error.error_tag("path") {
                    workspace.show_error(format!("'{path}' does not exist"), cx)
                }
            } else {
                workspace.show_error(format!("{error}"), cx)
            }
        }
    });

    Ok(items.into_iter().map(|item| item?.ok()).collect())
}
