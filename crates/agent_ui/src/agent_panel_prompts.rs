use std::{collections::HashSet, path::PathBuf};

use acp_thread::{MentionUri, line_range_suffix};
use agent_client_protocol::schema::v1 as acp;
use gpui::{App, Entity};
use project::{Project, ProjectItem, ProjectPath};
use text::OffsetRangeExt;

use crate::completion_provider::AgentContextSelection;
use mav_actions::agent::ConflictContent;

pub(crate) fn format_selection_for_terminal(
    selection: &AgentContextSelection,
    project: &Entity<Project>,
    working_directory: Option<&std::path::Path>,
    cx: &App,
) -> String {
    match selection {
        AgentContextSelection::Editor(ranges) => {
            let path_style = project.read(cx).path_style(cx);
            let mut parts: Vec<String> = Vec::new();
            for (buffer, range) in ranges {
                let buffer = buffer.read(cx);
                let Some(project_path) = buffer.project_path(cx) else {
                    continue;
                };
                let snapshot = buffer.snapshot();
                let point_range = range.to_point(&snapshot);
                let line_range = point_range.start.row..=point_range.end.row;
                let path = mention_path_for_terminal(
                    project,
                    &project_path,
                    working_directory,
                    path_style,
                    cx,
                );
                parts.push(format!("{path}{}", line_range_suffix(&line_range)));
            }
            if parts.is_empty() {
                String::new()
            } else {
                format!("{} ", parts.join(" "))
            }
        }
        AgentContextSelection::Terminal(texts) => texts.join("\n"),
    }
}

fn mention_path_for_terminal(
    project: &Entity<Project>,
    project_path: &ProjectPath,
    working_directory: Option<&std::path::Path>,
    path_style: util::paths::PathStyle,
    cx: &App,
) -> String {
    let abs_path = project.read(cx).absolute_path(project_path, cx);
    match (abs_path, working_directory) {
        (Some(abs_path), Some(working_directory)) => path_style
            .strip_prefix(&abs_path, working_directory)
            .map(|relative| relative.display(path_style).into_owned())
            .unwrap_or_else(|| abs_path.to_string_lossy().into_owned()),
        (Some(abs_path), None) => abs_path.to_string_lossy().into_owned(),
        (None, _) => project_path.path.display(path_style).into_owned(),
    }
}

pub(crate) fn conflict_resource_block(conflict: &ConflictContent) -> acp::ContentBlock {
    let mention_uri = MentionUri::MergeConflict {
        file_path: conflict.file_path.clone(),
    };
    acp::ContentBlock::Resource(acp::EmbeddedResource::new(
        acp::EmbeddedResourceResource::TextResourceContents(acp::TextResourceContents::new(
            conflict.conflict_text.clone(),
            mention_uri.to_uri().to_string(),
        )),
    ))
}

pub(crate) fn build_conflict_resolution_prompt(
    conflicts: &[ConflictContent],
) -> Vec<acp::ContentBlock> {
    if conflicts.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();

    if conflicts.len() == 1 {
        let conflict = &conflicts[0];

        blocks.push(acp::ContentBlock::Text(acp::TextContent::new(
            "Please resolve the following merge conflict in ",
        )));
        let mention = MentionUri::File {
            abs_path: PathBuf::from(conflict.file_path.clone()),
        };
        blocks.push(acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
            mention.name(),
            mention.to_uri(),
        )));

        blocks.push(acp::ContentBlock::Text(acp::TextContent::new(
            indoc::formatdoc!(
                "\nThe conflict is between branch `{ours}` (ours) and `{theirs}` (theirs).

                Analyze both versions carefully and resolve the conflict by editing \
                the file directly. Choose the resolution that best preserves the intent \
                of both changes, or combine them if appropriate.

                ",
                ours = conflict.ours_branch_name,
                theirs = conflict.theirs_branch_name,
            ),
        )));
    } else {
        let n = conflicts.len();
        let unique_files: HashSet<&str> = conflicts.iter().map(|c| c.file_path.as_str()).collect();
        let ours = &conflicts[0].ours_branch_name;
        let theirs = &conflicts[0].theirs_branch_name;
        blocks.push(acp::ContentBlock::Text(acp::TextContent::new(
            indoc::formatdoc!(
                "Please resolve all {n} merge conflicts below.

                The conflicts are between branch `{ours}` (ours) and `{theirs}` (theirs).

                For each conflict, analyze both versions carefully and resolve them \
                by editing the file{suffix} directly. Choose resolutions that best preserve \
                the intent of both changes, or combine them if appropriate.

                ",
                suffix = if unique_files.len() > 1 { "s" } else { "" },
            ),
        )));
    }

    for conflict in conflicts {
        blocks.push(conflict_resource_block(conflict));
    }

    blocks
}

pub(crate) fn build_conflicted_files_resolution_prompt(
    conflicted_file_paths: &[String],
) -> Vec<acp::ContentBlock> {
    if conflicted_file_paths.is_empty() {
        return Vec::new();
    }

    let instruction = indoc::indoc!(
        "The following files have unresolved merge conflicts. Please open each \
         file, find the conflict markers (`<<<<<<<` / `=======` / `>>>>>>>`), \
         and resolve every conflict by editing the files directly.

         Choose resolutions that best preserve the intent of both changes, \
         or combine them if appropriate.

         Files with conflicts:
         ",
    );

    let mut content = vec![acp::ContentBlock::Text(acp::TextContent::new(instruction))];
    for path in conflicted_file_paths {
        let mention = MentionUri::File {
            abs_path: PathBuf::from(path),
        };
        content.push(acp::ContentBlock::ResourceLink(acp::ResourceLink::new(
            mention.name(),
            mention.to_uri(),
        )));
        content.push(acp::ContentBlock::Text(acp::TextContent::new("\n")));
    }
    content
}
