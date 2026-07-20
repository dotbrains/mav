use chrono::{DateTime, Utc};

use crate::thread_metadata_store::ThreadMetadata;

fn format_timestamp_human(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);

    let relative = if duration.num_seconds() < 0 {
        "in the future".to_string()
    } else if duration.num_seconds() < 60 {
        let seconds = duration.num_seconds();
        format!("{seconds} seconds ago")
    } else if duration.num_minutes() < 60 {
        let minutes = duration.num_minutes();
        format!("{minutes} minutes ago")
    } else if duration.num_hours() < 24 {
        let hours = duration.num_hours();
        format!("{hours} hours ago")
    } else {
        let days = duration.num_days();
        format!("{days} days ago")
    };

    format!("{} ({})", dt.to_rfc3339(), relative)
}

/// Used for `dev: show thread metadata` action.
pub(crate) fn thread_metadata_to_debug_json(metadata: &ThreadMetadata) -> serde_json::Value {
    serde_json::json!({
        "thread_id": metadata.thread_id,
        "session_id": metadata.session_id.as_ref().map(|s| s.0.to_string()),
        "agent_id": metadata.agent_id.0.to_string(),
        "title": metadata.title.as_ref().map(|t| t.to_string()),
        "title_override": metadata.title_override.as_ref().map(|t| t.to_string()),
        "updated_at": format_timestamp_human(&metadata.updated_at),
        "created_at": metadata.created_at.as_ref().map(format_timestamp_human),
        "interacted_at": metadata.interacted_at.as_ref().map(format_timestamp_human),
        "worktree_paths": format!("{:?}", metadata.worktree_paths),
        "archived": metadata.archived,
    })
}
