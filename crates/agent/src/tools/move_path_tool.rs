use super::tool_permissions::{
    authorize_symlink_escapes, canonicalize_worktree_roots, collect_symlink_escapes,
    resolve_creatable_global_skill_descendant_path, resolve_global_skill_descendant_path,
    resolves_to_global_skills_dir, sensitive_settings_kind,
};
use crate::{
    AgentTool, ToolCallEventStream, ToolInput, ToolPermissionDecision,
    authorize_with_sensitive_settings, decide_permission_for_paths,
};
use agent_client_protocol::schema::v1 as acp;
use agent_settings::AgentSettings;
use futures::FutureExt as _;
use gpui::{App, Entity, SharedString, Task};
use project::Project;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings;
use std::{path::Path, sync::Arc};
use util::markdown::MarkdownInlineCode;

/// Moves or rename a file or directory in the project, and returns confirmation that the move succeeded.
///
/// If the source and destination directories are the same, but the filename is different, this performs a rename. Otherwise, it performs a move.
///
/// This tool should be used when it's desirable to move or rename a file or directory without changing its contents at all.
/// The only supported paths outside the project are descendants of `~/.agents/skills`, for global agent skills.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MovePathToolInput {
    /// The source path of the file or directory to move/rename.
    ///
    /// <example>
    /// If the project has the following files:
    ///
    /// - directory1/a/something.txt
    /// - directory2/a/things.txt
    /// - directory3/a/other.txt
    ///
    /// You can move the first file by providing a source_path of "directory1/a/something.txt"
    /// </example>
    pub source_path: String,

    /// The destination path where the file or directory should be moved/renamed to.
    /// If the paths are the same except for the filename, then this will be a rename.
    ///
    /// <example>
    /// To move "directory1/a/something.txt" to "directory2/b/renamed.txt",
    /// provide a destination_path of "directory2/b/renamed.txt"
    /// </example>
    pub destination_path: String,
}

pub struct MovePathTool {
    project: Entity<Project>,
}

impl MovePathTool {
    pub fn new(project: Entity<Project>) -> Self {
        Self { project }
    }
}

impl AgentTool for MovePathTool {
    type Input = MovePathToolInput;
    type Output = String;

    const NAME: &'static str = "move_path";

    fn kind() -> acp::ToolKind {
        acp::ToolKind::Move
    }

    fn initial_title(
        &self,
        input: Result<Self::Input, serde_json::Value>,
        _cx: &mut App,
    ) -> SharedString {
        if let Ok(input) = input {
            let src = MarkdownInlineCode(&input.source_path);
            let dest = MarkdownInlineCode(&input.destination_path);
            let src_path = Path::new(&input.source_path);
            let dest_path = Path::new(&input.destination_path);

            match dest_path
                .file_name()
                .and_then(|os_str| os_str.to_os_string().into_string().ok())
            {
                Some(filename) if src_path.parent() == dest_path.parent() => {
                    let filename = MarkdownInlineCode(&filename);
                    format!("Rename {src} to {filename}").into()
                }
                _ => format!("Move {src} to {dest}").into(),
            }
        } else {
            "Move path".into()
        }
    }

    fn run(
        self: Arc<Self>,
        input: ToolInput<Self::Input>,
        event_stream: ToolCallEventStream,
        cx: &mut App,
    ) -> Task<Result<Self::Output, Self::Output>> {
        let project = self.project.clone();
        cx.spawn(async move |cx| {
            let input = input
                .recv()
                .await
                .map_err(|e| e.to_string())?;
            let paths = vec![input.source_path.clone(), input.destination_path.clone()];
            let decision = cx.update(|cx| {
                decide_permission_for_paths(Self::NAME, &paths, AgentSettings::get_global(cx))
            });
            if let ToolPermissionDecision::Deny(reason) = decision {
                return Err(reason);
            }

            let fs = project.read_with(cx, |project, _cx| project.fs().clone());
            let canonical_roots = canonicalize_worktree_roots(&project, &fs, cx).await;

            if resolves_to_global_skills_dir(Path::new(&input.source_path), fs.as_ref()).await
                || resolves_to_global_skills_dir(
                    Path::new(&input.destination_path),
                    fs.as_ref(),
                )
                .await
            {
                return Err(
                    "Cannot move the global agent skills directory itself. Move a skill directory or file beneath it instead."
                        .to_string(),
                );
            }

            let global_source_path =
                resolve_global_skill_descendant_path(Path::new(&input.source_path), fs.as_ref())
                    .await;
            let global_destination_path = resolve_creatable_global_skill_descendant_path(
                Path::new(&input.destination_path),
                fs.as_ref(),
            )
            .await;

            let symlink_escapes: Vec<(&str, std::path::PathBuf)> =
                project.read_with(cx, |project, cx| {
                    collect_symlink_escapes(
                        project,
                        &input.source_path,
                        &input.destination_path,
                        &canonical_roots,
                        cx,
                    )
                });

            let sensitive_kind = sensitive_settings_kind(
                Path::new(&input.source_path),
                &canonical_roots,
                fs.as_ref(),
            )
            .await
            .or(sensitive_settings_kind(
                Path::new(&input.destination_path),
                &canonical_roots,
                fs.as_ref(),
            )
            .await);

            let needs_confirmation = matches!(decision, ToolPermissionDecision::Confirm)
                || (matches!(decision, ToolPermissionDecision::Allow) && sensitive_kind.is_some());

            let authorize = if !symlink_escapes.is_empty() {
                // Symlink escape authorization replaces (rather than supplements)
                // the normal tool-permission prompt. The symlink prompt already
                // requires explicit user approval with the canonical target shown,
                // which is strictly more security-relevant than a generic confirm.
                Some(cx.update(|cx| {
                    authorize_symlink_escapes(Self::NAME, &symlink_escapes, &event_stream, cx)
                }))
            } else if needs_confirmation {
                Some(cx.update(|cx| {
                    let src = MarkdownInlineCode(&input.source_path);
                    let dest = MarkdownInlineCode(&input.destination_path);
                    let context = crate::ToolPermissionContext::new(
                        Self::NAME,
                        vec![input.source_path.clone(), input.destination_path.clone()],
                    );
                    let title = format!("Move {src} to {dest}");
                    authorize_with_sensitive_settings(
                        sensitive_kind,
                        context,
                        &title,
                        &event_stream,
                        cx,
                    )
                }))
            } else {
                None
            };

            if let Some(authorize) = authorize {
                authorize.await.map_err(|e| e.to_string())?;
            }

            if global_source_path.is_some() || global_destination_path.is_some() {
                let source_path = if let Some(global_source_path) = global_source_path {
                    global_source_path
                } else {
                    project.read_with(cx, |project, cx| {
                        let project_path = project.find_project_path(&input.source_path, cx).ok_or_else(|| {
                            format!("Source path {} was not found in the project.", input.source_path)
                        })?;
                        project.entry_for_path(&project_path, cx).ok_or_else(|| {
                            format!("Source path {} was not found in the project.", input.source_path)
                        })?;
                        project.absolute_path(&project_path, cx).ok_or_else(|| {
                            format!("Source path {} could not be resolved.", input.source_path)
                        })
                    })?
                };

                let destination_path = if let Some(global_destination_path) = global_destination_path
                {
                    global_destination_path
                } else {
                    project.read_with(cx, |project, cx| {
                        let project_path = project.find_project_path(&input.destination_path, cx).ok_or_else(|| {
                            format!(
                                "Destination path {} was outside the project.",
                                input.destination_path
                            )
                        })?;
                        project.absolute_path(&project_path, cx).ok_or_else(|| {
                            format!(
                                "Destination path {} could not be resolved.",
                                input.destination_path
                            )
                        })
                    })?
                };

                futures::select! {
                    result = fs.rename(
                        &source_path,
                        &destination_path,
                        fs::RenameOptions {
                            create_parents: true,
                            ..fs::RenameOptions::default()
                        },
                    ).fuse() => {
                        result.map_err(|e| format!("Moving {} to {}: {e}", input.source_path, input.destination_path))?;
                    }
                    _ = event_stream.cancelled_by_user().fuse() => {
                        return Err("Move cancelled by user".to_string());
                    }
                }

                return Ok(format!(
                    "Moved {} to {}",
                    input.source_path, input.destination_path
                ));
            }

            let rename_task = project.update(cx, |project, cx| {
                match project
                    .find_project_path(&input.source_path, cx)
                    .and_then(|project_path| project.entry_for_path(&project_path, cx))
                {
                    Some(entity) => match project.find_project_path(&input.destination_path, cx) {
                        Some(project_path) => Ok(project.rename_entry(entity.id, project_path, cx)),
                        None => Err(format!(
                            "Destination path {} was outside the project.",
                            input.destination_path
                        )),
                    },
                    None => Err(format!(
                        "Source path {} was not found in the project.",
                        input.source_path
                    )),
                }
            })?;

            futures::select! {
                result = rename_task.fuse() => result.map_err(|e| format!("Moving {} to {}: {e}", input.source_path, input.destination_path))?,
                _ = event_stream.cancelled_by_user().fuse() => {
                    return Err("Move cancelled by user".to_string());
                }
            };
            Ok(format!(
                "Moved {} to {}",
                input.source_path, input.destination_path
            ))
        })
    }
}

#[cfg(test)]
#[path = "move_path_tool/tests.rs"]
mod tests;
