use super::*;
use crate::OpenMode;
use crate::PathList;
use crate::ProjectGroupKey;
use crate::{
    multi_workspace::MultiWorkspace,
    persistence::{
        model::{
            SerializedItem, SerializedPane, SerializedPaneGroup, SerializedWorkspace,
            SessionWorkspace,
        },
        read_multi_workspace_state,
    },
};
use gpui::TaskExt;

use gpui::AppContext as _;
use pretty_assertions::assert_eq;
use project::Project;
use remote::SshConnectionOptions;
use serde_json::json;
use std::{thread, time::Duration};

/// Creates a unique directory in a FakeFs, returning the path.
/// Uses a UUID suffix to avoid collisions with other tests sharing the global DB.
async fn unique_test_dir(fs: &fs::FakeFs, prefix: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/test-dirs/{}-{}", prefix, uuid::Uuid::new_v4()));
    fs.insert_tree(&dir, json!({})).await;
    dir
}

mod breakpoints;
mod ids_and_serialization;
mod multi_workspace;
mod multi_workspace_state;
mod pane_cleanup;
mod project_groups;
mod recent_helpers;
mod recent_identity;
mod remote_workspaces;
mod serialization_lifecycle;
mod workspace_sessions;
