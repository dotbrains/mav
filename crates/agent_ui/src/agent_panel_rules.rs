use std::path::PathBuf;

use gpui::{App, Context, Entity, TaskExt, Window};
use project::Project;
use workspace::Workspace;

pub(crate) fn project_agents_md_path(
    project: &Entity<Project>,
    require_existing_file: bool,
    cx: &App,
) -> Option<PathBuf> {
    let rel_path = util::rel_path::RelPath::unix("AGENTS.md").ok()?;
    project
        .read(cx)
        .visible_worktrees(cx)
        .next()
        .and_then(|worktree| {
            let worktree = worktree.read(cx);

            if require_existing_file {
                let entry = worktree.entry_for_path(rel_path)?;
                if !entry.is_file() {
                    return None;
                }
            }

            Some(worktree.absolutize(rel_path))
        })
}

pub(crate) fn open_global_rules(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    workspace
        .open_abs_path(
            paths::agents_file().clone(),
            workspace::OpenOptions {
                focus: Some(true),
                ..Default::default()
            },
            window,
            cx,
        )
        .detach_and_log_err(cx);
}

pub(crate) fn open_project_rules(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(path) = project_agents_md_path(workspace.project(), false, cx) {
        workspace
            .open_abs_path(
                path,
                workspace::OpenOptions {
                    focus: Some(true),
                    ..Default::default()
                },
                window,
                cx,
            )
            .detach_and_log_err(cx);
    }
}
