use super::{PathList, Workspace, WorkspaceDb, WorkspaceId};
use gpui::{App, Entity, WindowId};
use project::ProjectPath;
use util::ResultExt as _;

use crate::persistence::model::{SerializedWorkspaceLocation, SessionWorkspace};

pub trait WorkspaceHandle {
    fn file_project_paths(&self, cx: &App) -> Vec<ProjectPath>;
}

impl WorkspaceHandle for Entity<Workspace> {
    fn file_project_paths(&self, cx: &App) -> Vec<ProjectPath> {
        self.read(cx)
            .worktrees(cx)
            .flat_map(|worktree| {
                let worktree_id = worktree.read(cx).id();
                worktree.read(cx).files(true, 0).map(move |f| ProjectPath {
                    worktree_id,
                    path: f.path.clone(),
                })
            })
            .collect::<Vec<_>>()
    }
}

pub async fn last_opened_workspace_location(
    db: &WorkspaceDb,
    fs: &dyn fs::Fs,
) -> Option<(WorkspaceId, SerializedWorkspaceLocation, PathList)> {
    db.last_workspace(fs)
        .await
        .log_err()
        .flatten()
        .map(|workspace| (workspace.workspace_id, workspace.location, workspace.paths))
}

pub async fn last_session_workspace_locations(
    db: &WorkspaceDb,
    last_session_id: &str,
    last_session_window_stack: Option<Vec<WindowId>>,
    fs: &dyn fs::Fs,
) -> Option<Vec<SessionWorkspace>> {
    db.last_session_workspace_locations(last_session_id, last_session_window_stack, fs)
        .await
        .log_err()
}
