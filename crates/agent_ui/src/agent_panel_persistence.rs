use anyhow::Result;
use db::kvp::KeyValueStore;
use serde::{Deserialize, Serialize};
use util::ResultExt as _;
use workspace::{SerializedPathList, WorkspaceId};

use crate::{Agent, thread_metadata_store::ThreadId};

pub(crate) const AGENT_PANEL_KEY: &str = "agent_panel";

const LAST_USED_AGENT_KEY: &str = "agent_panel__last_used_external_agent";
const LAST_CREATED_ENTRY_KIND_KEY: &str = "agent_panel__last_created_entry_kind";

#[derive(Serialize, Deserialize)]
struct LastUsedAgent {
    agent: Agent,
}

#[derive(Serialize, Deserialize)]
struct LastCreatedEntryKind {
    entry_kind: AgentPanelEntryKind,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum AgentPanelEntryKind {
    #[default]
    Thread,
    Terminal,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct SerializedAgentPanel {
    pub(crate) selected_agent: Option<Agent>,
    #[serde(default)]
    pub(crate) last_created_entry_kind: AgentPanelEntryKind,
    #[serde(default)]
    pub(crate) last_active_thread: Option<SerializedActiveThread>,
    #[serde(default)]
    pub(crate) last_active_terminal_id: Option<String>,
    #[serde(default)]
    pub(crate) new_draft_thread_id: Option<ThreadId>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct SerializedActiveThread {
    /// For drafts this is `None`; use `thread_id` to address them instead.
    pub(crate) session_id: Option<String>,
    /// Optional for back-compat with older serialized payloads that only carried `session_id`.
    #[serde(default)]
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) agent_type: Agent,
    pub(crate) title: Option<String>,
    pub(crate) work_dirs: Option<SerializedPathList>,
}

/// Reads the most recently used agent across all workspaces. Used as a fallback
/// when opening a workspace that has no per-workspace agent preference yet.
pub(crate) fn read_global_last_used_agent(kvp: &KeyValueStore) -> Option<Agent> {
    kvp.read_kvp(LAST_USED_AGENT_KEY)
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str::<LastUsedAgent>(&json).log_err())
        .map(|entry| entry.agent)
}

pub(crate) async fn write_global_last_used_agent(kvp: KeyValueStore, agent: Agent) {
    if let Some(json) = serde_json::to_string(&LastUsedAgent { agent }).log_err() {
        kvp.write_kvp(LAST_USED_AGENT_KEY.to_string(), json)
            .await
            .log_err();
    }
}

pub(crate) fn read_global_last_created_entry_kind(
    kvp: &KeyValueStore,
) -> Option<AgentPanelEntryKind> {
    kvp.read_kvp(LAST_CREATED_ENTRY_KIND_KEY)
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str::<LastCreatedEntryKind>(&json).log_err())
        .map(|entry| entry.entry_kind)
}

pub(crate) async fn write_global_last_created_entry_kind(
    kvp: KeyValueStore,
    entry_kind: AgentPanelEntryKind,
) {
    if let Some(json) = serde_json::to_string(&LastCreatedEntryKind { entry_kind }).log_err() {
        kvp.write_kvp(LAST_CREATED_ENTRY_KIND_KEY.to_string(), json)
            .await
            .log_err();
    }
}

pub(crate) fn read_serialized_panel(
    workspace_id: WorkspaceId,
    kvp: &KeyValueStore,
) -> Option<SerializedAgentPanel> {
    let scope = kvp.scoped(AGENT_PANEL_KEY);
    let key = i64::from(workspace_id).to_string();
    scope
        .read(&key)
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str::<SerializedAgentPanel>(&json).log_err())
}

pub(crate) async fn save_serialized_panel(
    workspace_id: WorkspaceId,
    panel: SerializedAgentPanel,
    kvp: KeyValueStore,
) -> Result<()> {
    let scope = kvp.scoped(AGENT_PANEL_KEY);
    let key = i64::from(workspace_id).to_string();
    scope.write(key, serde_json::to_string(&panel)?).await?;
    Ok(())
}

/// Migration: reads the original single-panel format stored under the
/// `"agent_panel"` KVP key before per-workspace keying was introduced.
pub(crate) fn read_legacy_serialized_panel(kvp: &KeyValueStore) -> Option<SerializedAgentPanel> {
    kvp.read_kvp(AGENT_PANEL_KEY)
        .log_err()
        .flatten()
        .and_then(|json| serde_json::from_str::<SerializedAgentPanel>(&json).log_err())
}
