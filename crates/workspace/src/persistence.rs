mod bindings;
pub mod model;
mod recent;
mod serialization;

use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use chrono::{DateTime, NaiveDateTime, Utc};
use fs::Fs;

use anyhow::{Context as _, Result, bail};
use collections::{HashMap, HashSet, IndexSet};
use db::{
    kvp::KeyValueStore,
    query,
    sqlez::{connection::Connection, domain::Domain},
    sqlez_macros::sql,
};
use gpui::{Axis, Bounds, Task, WindowBounds, WindowId, point, size};
use project::{
    ProjectGroupKey,
    bookmark_store::SerializedBookmark,
    debugger::breakpoint_store::{BreakpointState, SourceBreakpoint},
    trusted_worktrees::{DbTrustedPaths, RemoteHostLocation},
};

use language::{LanguageName, Toolchain, ToolchainScope};
use remote::{
    DockerConnectionOptions, RemoteConnectionIdentity, RemoteConnectionOptions,
    SshConnectionOptions, WslConnectionOptions, remote_connection_identity,
};
use serde::{Deserialize, Serialize};
use sqlez::{
    bindable::{Bind, Column, StaticColumnCount},
    statement::Statement,
    thread_safe_connection::ThreadSafeConnection,
};

use ui::{App, SharedString, px};
use util::{ResultExt, maybe, rel_path::RelPath};
use uuid::Uuid;

use crate::{
    WorkspaceId,
    pane::PaneKind,
    path_list::{PathList, SerializedPathList},
    persistence::model::RemoteConnectionKind,
};

use model::{
    GroupId, ItemId, PaneId, RemoteConnectionId, SerializedItem, SerializedPane,
    SerializedPaneGroup, SerializedWorkspace,
};

use self::model::{DockStructure, SerializedWorkspaceLocation, SessionWorkspace};
use bindings::{Bookmark, Breakpoint, BreakpointStateWrapper};
pub use recent::RecentWorkspace;
use recent::{dedupe_recent_workspaces, resolve_local_workspace_identity};
pub(crate) use serialization::{SerializedAxis, SerializedWindowBounds};
pub use serialization::{read_default_window_bounds, write_default_window_bounds};

// https://www.sqlite.org/limits.html
// > <..> the maximum value of a host parameter number is SQLITE_MAX_VARIABLE_NUMBER,
// > which defaults to <..> 32766 for SQLite versions after 3.32.0.
const MAX_QUERY_PLACEHOLDERS: usize = 32000;

fn parse_timestamp(text: &str) -> DateTime<Utc> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
        .map(|naive| naive.and_utc())
        .unwrap_or_else(|_| Utc::now())
}

fn contains_wsl_path(paths: &PathList) -> bool {
    cfg!(windows)
        && paths
            .paths()
            .iter()
            .any(|path| util::paths::WslPath::from_path(path).is_some())
}

pub(crate) fn read_multi_workspace_state_if_present(
    window_id: WindowId,
    cx: &App,
) -> Option<model::MultiWorkspaceState> {
    let kvp = KeyValueStore::global(cx);
    kvp.scoped("multi_workspace_state")
        .read(&window_id.as_u64().to_string())
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn read_multi_workspace_state(window_id: WindowId, cx: &App) -> model::MultiWorkspaceState {
    read_multi_workspace_state_if_present(window_id, cx).unwrap_or_default()
}

pub async fn write_multi_workspace_state(
    kvp: &KeyValueStore,
    window_id: WindowId,
    state: model::MultiWorkspaceState,
) {
    if let Ok(json_str) = serde_json::to_string(&state) {
        kvp.scoped("multi_workspace_state")
            .write(window_id.as_u64().to_string(), json_str)
            .await
            .log_err();
    }
}

pub fn read_serialized_multi_workspaces(
    session_workspaces: Vec<model::SessionWorkspace>,
    cx: &App,
) -> Vec<model::SerializedMultiWorkspace> {
    let mut window_groups: Vec<Vec<model::SessionWorkspace>> = Vec::new();
    let mut window_id_to_group: HashMap<WindowId, usize> = HashMap::default();

    for session_workspace in session_workspaces {
        match session_workspace.window_id {
            Some(window_id) => {
                let group_index = *window_id_to_group.entry(window_id).or_insert_with(|| {
                    window_groups.push(Vec::new());
                    window_groups.len() - 1
                });
                window_groups[group_index].push(session_workspace);
            }
            None => {
                window_groups.push(vec![session_workspace]);
            }
        }
    }

    window_groups
        .into_iter()
        .filter_map(|group| {
            let window_id = group.first().and_then(|sw| sw.window_id);
            let state = window_id
                .map(|wid| read_multi_workspace_state(wid, cx))
                .unwrap_or_default();
            let active_workspace = state
                .active_workspace_id
                .and_then(|id| group.iter().position(|ws| ws.workspace_id == id))
                // If the persisted active workspace can't be matched (e.g. its
                // pointer was lost or its row was pruned), fall back to the
                // first workspace that actually has paths rather than blindly
                // taking index 0, so a stray scratch/empty workspace isn't
                // restored as the focused window. Only if none have paths do we
                // fall back to the first entry.
                .or_else(|| group.iter().position(|ws| !ws.paths.is_empty()))
                .or(Some(0))
                .and_then(|index| group.into_iter().nth(index))?;
            Some(model::SerializedMultiWorkspace {
                active_workspace,
                state,
            })
        })
        .collect()
}

const DEFAULT_DOCK_STATE_KEY: &str = "default_dock_state";

pub fn read_default_dock_state(kvp: &KeyValueStore) -> Option<DockStructure> {
    let json_str = kvp.read_kvp(DEFAULT_DOCK_STATE_KEY).log_err().flatten()?;

    serde_json::from_str::<DockStructure>(&json_str).ok()
}

pub async fn write_default_dock_state(
    kvp: &KeyValueStore,
    docks: DockStructure,
) -> anyhow::Result<()> {
    let json_str = serde_json::to_string(&docks)?;
    kvp.write_kvp(DEFAULT_DOCK_STATE_KEY.to_string(), json_str)
        .await?;
    Ok(())
}

mod metadata;
mod panes;
mod recent_sessions;
mod remote_connections;
mod save_workspace;
mod schema;
mod updates_toolchains_trust;
mod workspace_rows;

pub struct WorkspaceDb(ThreadSafeConnection);

db::static_connection!(WorkspaceDb, []);

impl WorkspaceDb {
    query! {
        pub async fn next_id() -> Result<WorkspaceId> {
            INSERT INTO workspaces DEFAULT VALUES RETURNING workspace_id
        }
    }

    query! {
        pub(super) fn recent_workspaces_query() -> Result<Vec<(WorkspaceId, String, String, Option<String>, Option<String>, Option<u64>, Option<String>, String)>> {
            SELECT workspace_id, paths, paths_order, identity_paths, identity_paths_order, remote_connection_id, session_id, timestamp
            FROM workspaces
            WHERE
                paths IS NOT NULL OR
                remote_connection_id IS NOT NULL
            ORDER BY timestamp DESC
        }
    }

    query! {
        pub(super) fn session_workspaces_query(session_id: String) -> Result<Vec<(i64, String, String, Option<u64>, Option<u64>)>> {
            SELECT workspace_id, paths, paths_order, window_id, remote_connection_id
            FROM workspaces
            WHERE session_id = ?1
            ORDER BY timestamp DESC
        }
    }

    query! {
        pub fn breakpoints_for_file(workspace_id: WorkspaceId, file_path: &Path) -> Result<Vec<Breakpoint>> {
            SELECT breakpoint_location
            FROM breakpoints
            WHERE  workspace_id= ?1 AND path = ?2
        }
    }

    query! {
        pub fn clear_breakpoints(file_path: &Path) -> Result<()> {
            DELETE FROM breakpoints
            WHERE file_path = ?2
        }
    }

    query! {
        pub async fn delete_workspace_by_id(id: WorkspaceId) -> Result<()> {
            DELETE FROM workspaces
            WHERE workspace_id IS ?
        }
    }

    query! {
        pub async fn update_timestamp(workspace_id: WorkspaceId) -> Result<()> {
            UPDATE workspaces
            SET timestamp = CURRENT_TIMESTAMP
            WHERE workspace_id = ?
        }
    }

    #[cfg(test)]
    query! {
        pub(crate) async fn set_timestamp_for_tests(workspace_id: WorkspaceId, timestamp: String) -> Result<()> {
            UPDATE workspaces
            SET timestamp = ?2
            WHERE workspace_id = ?1
        }
    }

    query! {
        pub(crate) async fn set_window_open_status(workspace_id: WorkspaceId, bounds: SerializedWindowBounds, display: Uuid) -> Result<()> {
            UPDATE workspaces
            SET window_state = ?2,
                window_x = ?3,
                window_y = ?4,
                window_width = ?5,
                window_height = ?6,
                display = ?7
            WHERE workspace_id = ?1
        }
    }

    query! {
        pub(crate) async fn set_centered_layout(workspace_id: WorkspaceId, centered_layout: bool) -> Result<()> {
            UPDATE workspaces
            SET centered_layout = ?2
            WHERE workspace_id = ?1
        }
    }

    query! {
        pub(crate) async fn set_session_id(workspace_id: WorkspaceId, session_id: Option<String>) -> Result<()> {
            UPDATE workspaces
            SET session_id = ?2
            WHERE workspace_id = ?1
        }
    }

    query! {
        pub(crate) async fn set_session_binding(workspace_id: WorkspaceId, session_id: Option<String>, window_id: Option<u64>) -> Result<()> {
            UPDATE workspaces
            SET session_id = ?2, window_id = ?3
            WHERE workspace_id = ?1
        }
    }

    query! {
        pub(super) fn trusted_worktrees() -> Result<Vec<(Option<PathBuf>, Option<String>, Option<String>)>> {
            SELECT absolute_path, user_name, host_name
            FROM trusted_worktrees
        }
    }

    query! {
        pub async fn clear_trusted_worktrees() -> Result<()> {
            DELETE FROM trusted_worktrees
        }
    }
}

pub fn delete_unloaded_items(
    alive_items: Vec<ItemId>,
    workspace_id: WorkspaceId,
    table: &'static str,
    db: &ThreadSafeConnection,
    cx: &mut App,
) -> Task<Result<()>> {
    let db = db.clone();
    cx.spawn(async move |_| {
        let placeholders = alive_items
            .iter()
            .map(|_| "?")
            .collect::<Vec<&str>>()
            .join(", ");

        let query = format!(
            "DELETE FROM {table} WHERE workspace_id = ? AND item_id NOT IN ({placeholders})"
        );

        db.write(move |conn| {
            let mut statement = Statement::prepare(conn, query)?;
            let mut next_index = statement.bind(&workspace_id, 1)?;
            for id in alive_items {
                next_index = statement.bind(&id, next_index)?;
            }
            statement.exec()
        })
        .await
    })
}

#[cfg(test)]
mod tests;
