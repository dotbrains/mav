use crate::{WorkspaceDb, persistence, window_chrome::window_bounds_env_override};
use anyhow::{Context as _, Result};
use gpui::{App, AppContext, Task, WindowBounds};
use remote::RemoteConnectionOptions;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug)]
pub struct WorkspacePosition {
    pub window_bounds: Option<WindowBounds>,
    pub display: Option<Uuid>,
    pub centered_layout: bool,
}

pub fn remote_workspace_position_from_db(
    connection_options: RemoteConnectionOptions,
    paths_to_open: &[PathBuf],
    cx: &App,
) -> Task<Result<WorkspacePosition>> {
    let paths = paths_to_open.to_vec();
    let db = WorkspaceDb::global(cx);
    let kvp = db::kvp::KeyValueStore::global(cx);

    cx.background_spawn(async move {
        let remote_connection_id = db
            .get_or_create_remote_connection(connection_options)
            .await
            .context("fetching serialized ssh project")?;
        let serialized_workspace = db.remote_workspace_for_roots(&paths, remote_connection_id);

        let (window_bounds, display) = if let Some(bounds) = window_bounds_env_override() {
            (Some(WindowBounds::Windowed(bounds)), None)
        } else {
            let restorable_bounds = serialized_workspace
                .as_ref()
                .and_then(|workspace| {
                    Some((workspace.display?, workspace.window_bounds.map(|b| b.0)?))
                })
                .or_else(|| persistence::read_default_window_bounds(&kvp));

            if let Some((serialized_display, serialized_bounds)) = restorable_bounds {
                (Some(serialized_bounds), Some(serialized_display))
            } else {
                (None, None)
            }
        };

        let centered_layout = serialized_workspace
            .as_ref()
            .map(|w| w.centered_layout)
            .unwrap_or(false);

        Ok(WorkspacePosition {
            window_bounds,
            display,
            centered_layout,
        })
    })
}
