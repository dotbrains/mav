use super::{WorkspaceDb, WorkspaceId};
use crate::persistence::model::SerializedWorkspace;
use anyhow::Result;
use gpui::{AppContext, AsyncApp, Task};
use remote::RemoteConnectionOptions;
use std::path::PathBuf;

pub(super) fn deserialize_remote_project(
    connection_options: RemoteConnectionOptions,
    paths: Vec<PathBuf>,
    cx: &AsyncApp,
) -> Task<Result<(WorkspaceId, Option<SerializedWorkspace>)>> {
    let db = cx.update(|cx| WorkspaceDb::global(cx));
    cx.background_spawn(async move {
        let remote_connection_id = db
            .get_or_create_remote_connection(connection_options)
            .await?;

        let serialized_workspace = db.remote_workspace_for_roots(&paths, remote_connection_id);

        let workspace_id = if let Some(workspace_id) =
            serialized_workspace.as_ref().map(|workspace| workspace.id)
        {
            workspace_id
        } else {
            db.next_id().await?
        };

        Ok((workspace_id, serialized_workspace))
    })
}
