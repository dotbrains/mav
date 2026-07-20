use super::{DockPosition, Workspace};
use db::kvp::KeyValueStore;
use gpui::{App, AppContext as _, Pixels, TaskExt as _};
use serde::Deserialize;
use util::ResultExt as _;

/// Reads a panel's pixel size from its legacy KVP format and deletes the legacy
/// key. This migration path only runs once per panel per workspace.
pub(super) fn load_legacy_panel_size(
    panel_key: &str,
    dock_position: DockPosition,
    workspace: &Workspace,
    cx: &mut App,
) -> Option<Pixels> {
    #[derive(Deserialize)]
    struct LegacyPanelState {
        #[serde(default)]
        width: Option<Pixels>,
        #[serde(default)]
        height: Option<Pixels>,
    }

    let workspace_id = workspace
        .database_id()
        .map(|id| i64::from(id).to_string())
        .or_else(|| workspace.session_id())?;

    let legacy_key = match panel_key {
        "ProjectPanel" => {
            format!("{}-{:?}", "ProjectPanel", workspace_id)
        }
        "OutlinePanel" => {
            format!("{}-{:?}", "OutlinePanel", workspace_id)
        }
        "GitPanel" => {
            format!("{}-{:?}", "GitPanel", workspace_id)
        }
        "TerminalPanel" => {
            format!("{:?}-{:?}", "TerminalPanel", workspace_id)
        }
        _ => return None,
    };

    let kvp = KeyValueStore::global(cx);
    let json = kvp.read_kvp(&legacy_key).log_err().flatten()?;
    let state = serde_json::from_str::<LegacyPanelState>(&json).log_err()?;
    let size = match dock_position {
        DockPosition::Bottom => state.height,
        DockPosition::Left | DockPosition::Right => state.width,
    }?;

    cx.background_spawn(async move { kvp.delete_kvp(legacy_key).await })
        .detach_and_log_err(cx);

    Some(size)
}
