use super::tool_permissions::{
    authorize_symlink_access, canonicalize_worktree_roots, detect_symlink_escape,
    resolve_global_skill_descendant_path, resolves_to_global_skills_dir, sensitive_settings_kind,
};
use crate::{
    AgentTool, ToolCallEventStream, ToolInput, ToolPermissionDecision,
    authorize_with_sensitive_settings, decide_permission_for_path,
};
use action_log::ActionLog;
use agent_client_protocol::schema::v1 as acp;
use agent_settings::AgentSettings;
use futures::{FutureExt as _, SinkExt, StreamExt, channel::mpsc};
use gpui::{App, AppContext, Entity, SharedString, Task};
use project::{Project, ProjectPath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::path::Path;
use std::sync::Arc;
use util::markdown::MarkdownInlineCode;

/// Deletes the file or directory (and the directory's contents, recursively) at the specified path in the project, and returns confirmation of the deletion.
///
/// The only supported paths outside the project are descendants of `~/.agents/skills`, for global agent skills.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct DeletePathToolInput {
    /// The path of the file or directory to delete.
    ///
    /// <example>
    /// If the project has the following files:
    ///
    /// - directory1/a/something.txt
    /// - directory2/a/things.txt
    /// - directory3/a/other.txt
    ///
    /// You can delete the first file by providing a path of "directory1/a/something.txt"
    /// </example>
    pub path: String,
}

pub struct DeletePathTool {
    project: Entity<Project>,
    action_log: Entity<ActionLog>,
}

impl DeletePathTool {
    pub fn new(project: Entity<Project>, action_log: Entity<ActionLog>) -> Self {
        Self {
            project,
            action_log,
        }
    }
}

impl AgentTool for DeletePathTool {
    type Input = DeletePathToolInput;
    type Output = String;

    const NAME: &'static str = "delete_path";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Delete
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        if let Ok(input) = input {
            format!("Delete “`{}`”", input.path).into()
        } else {
            "Delete path".into()
        }
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        let project = self.project.clone();
        let action_log = self.action_log.clone();
        cx.spawn(async move |cx| {
            let input = input.recv().await.map_err(|e| e.to_string())?;
            let path = input.path;

            let decision = cx.update(|cx| {
                decide_permission_for_path(Self::NAME, &path, AgentSettings::get_global(cx))
            });

            if let ToolPermissionDecision::Deny(reason) = decision {
                return Err(reason);
            }

            let fs = project.read_with(cx, |project, _cx| project.fs().clone());
            let canonical_roots = canonicalize_worktree_roots(&project, &fs, cx).await;

            if resolves_to_global_skills_dir(Path::new(&path), fs.as_ref()).await {
                return Err(
                    "Cannot delete the global agent skills directory itself. Delete a skill directory or file beneath it instead."
                        .to_string(),
                );
            }

            let global_skill_path =
                resolve_global_skill_descendant_path(Path::new(&path), fs.as_ref()).await;

            let symlink_escape_target = project.read_with(cx, |project, cx| {
                detect_symlink_escape(project, &path, &canonical_roots, cx)
                    .map(|(_, target)| target)
            });

            let settings_kind =
                sensitive_settings_kind(Path::new(&path), &canonical_roots, fs.as_ref()).await;

            let decision =
                if matches!(decision, ToolPermissionDecision::Allow) && settings_kind.is_some() {
                    ToolPermissionDecision::Confirm
                } else {
                    decision
                };

            let authorize = if let Some(canonical_target) = symlink_escape_target {
                // Symlink escape authorization replaces (rather than supplements)
                // the normal tool-permission prompt. The symlink prompt already
                // requires explicit user approval with the canonical target shown,
                // which is strictly more security-relevant than a generic confirm.
                Some(cx.update(|cx| {
                    authorize_symlink_access(
                        Self::NAME,
                        &path,
                        &canonical_target,
                        &event_stream,
                        cx,
                    )
                }))
            } else {
                match decision {
                    ToolPermissionDecision::Allow => None,
                    ToolPermissionDecision::Confirm => Some(cx.update(|cx| {
                        let context =
                            crate::ToolPermissionContext::new(Self::NAME, vec![path.clone()]);
                        let title = format!("Delete {}", MarkdownInlineCode(&path));
                        authorize_with_sensitive_settings(
                            settings_kind,
                            context,
                            &title,
                            &event_stream,
                            cx,
                        )
                    })),
                    ToolPermissionDecision::Deny(_) => None,
                }
            };

            if let Some(authorize) = authorize {
                authorize.await.map_err(|e| e.to_string())?;
            }

            if let Some(global_skill_path) = global_skill_path {
                let metadata = fs
                    .metadata(&global_skill_path)
                    .await
                    .map_err(|e| format!("Deleting {path}: {e}"))?
                    .ok_or_else(|| format!("Deleting {path}: path not found"))?;

                futures::select! {
                    result = async {
                        if metadata.is_dir {
                            fs.remove_dir(
                                &global_skill_path,
                                fs::RemoveOptions {
                                    recursive: true,
                                    ..fs::RemoveOptions::default()
                                },
                            )
                            .await
                        } else {
                            fs.remove_file(&global_skill_path, fs::RemoveOptions::default()).await
                        }
                    }.fuse() => {
                        result.map_err(|e| format!("Deleting {path}: {e}"))?;
                    }
                    _ = event_stream.cancelled_by_user().fuse() => {
                        return Err("Delete cancelled by user".to_string());
                    }
                }

                return Ok(format!("Deleted {path}"));
            }

            let (project_path, worktree_snapshot) = project.read_with(cx, |project, cx| {
                let project_path = project.find_project_path(&path, cx).ok_or_else(|| {
                    format!("Couldn't delete {path} because that path isn't in this project.")
                })?;
                let worktree = project
                    .worktree_for_id(project_path.worktree_id, cx)
                    .ok_or_else(|| {
                        format!("Couldn't delete {path} because that path isn't in this project.")
                    })?;
                let worktree_snapshot = worktree.read(cx).snapshot();
                Result::<_, String>::Ok((project_path, worktree_snapshot))
            })?;

            let (mut paths_tx, mut paths_rx) = mpsc::channel(256);
            cx.background_spawn({
                let project_path = project_path.clone();
                async move {
                    for entry in
                        worktree_snapshot.traverse_from_path(true, false, false, &project_path.path)
                    {
                        if !entry.path.starts_with(&project_path.path) {
                            break;
                        }
                        paths_tx
                            .send(ProjectPath {
                                worktree_id: project_path.worktree_id,
                                path: entry.path.clone(),
                            })
                            .await?;
                    }
                    anyhow::Ok(())
                }
            })
            .detach();

            loop {
                let path_result = futures::select! {
                    path = paths_rx.next().fuse() => path,
                    _ = event_stream.cancelled_by_user().fuse() => {
                        return Err("Delete cancelled by user".to_string());
                    }
                };
                let Some(path) = path_result else {
                    break;
                };
                if let Ok(buffer) = project
                    .update(cx, |project, cx| project.open_buffer(path, cx))
                    .await
                {
                    action_log.update(cx, |action_log, cx| {
                        action_log.will_delete_buffer(buffer.clone(), cx)
                    });
                }
            }

            let deletion_task = project
                .update(cx, |project, cx| {
                    project.delete_file(project_path, false, cx)
                })
                .ok_or_else(|| {
                    format!("Couldn't delete {path} because that path isn't in this project.")
                })?;

            futures::select! {
                result = deletion_task.fuse() => {
                    result.map_err(|e| format!("Deleting {path}: {e}"))?;
                }
                _ = event_stream.cancelled_by_user().fuse() => {
                    return Err("Delete cancelled by user".to_string());
                }
            }
            Ok(format!("Deleted {path}"))
        })
    }
}

#[cfg(test)]
mod tests;
