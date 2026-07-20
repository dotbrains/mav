use anyhow::Result;
use collections::HashSet;
use futures::Future;
use gpui::Context;
use project::ProjectPath;
use std::path::PathBuf;
use ui::Window;
use util::ResultExt;

use crate::{ItemHandle, Workspace, persistence::model::SerializedWorkspace};

pub(crate) fn open_items(
    serialized_workspace: Option<SerializedWorkspace>,
    mut project_paths_to_open: Vec<(PathBuf, Option<ProjectPath>)>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> impl 'static + Future<Output = Result<Vec<Option<Result<Box<dyn ItemHandle>>>>>> + use<> {
    let restored_items = serialized_workspace.map(|serialized_workspace| {
        Workspace::load_workspace(
            serialized_workspace,
            project_paths_to_open
                .iter()
                .map(|(_, project_path)| project_path)
                .cloned()
                .collect(),
            window,
            cx,
        )
    });

    cx.spawn_in(window, async move |workspace, cx| {
        let mut opened_items = Vec::with_capacity(project_paths_to_open.len());

        if let Some(restored_items) = restored_items {
            let restored_items = restored_items.await?;

            let restored_project_paths = restored_items
                .iter()
                .filter_map(|item| {
                    cx.update(|_, cx| item.as_ref()?.project_path(cx))
                        .ok()
                        .flatten()
                })
                .collect::<HashSet<_>>();

            for restored_item in restored_items {
                opened_items.push(restored_item.map(Ok));
            }

            project_paths_to_open
                .iter_mut()
                .for_each(|(_, project_path)| {
                    if let Some(project_path_to_open) = project_path
                        && restored_project_paths.contains(project_path_to_open)
                    {
                        *project_path = None;
                    }
                });
        } else {
            for _ in 0..project_paths_to_open.len() {
                opened_items.push(None);
            }
        }
        assert!(opened_items.len() == project_paths_to_open.len());

        let tasks =
            project_paths_to_open
                .into_iter()
                .enumerate()
                .map(|(ix, (abs_path, project_path))| {
                    let workspace = workspace.clone();
                    cx.spawn(async move |cx| {
                        let file_project_path = project_path?;
                        let abs_path_task = workspace.update(cx, |workspace, cx| {
                            workspace.project().update(cx, |project, cx| {
                                project.resolve_abs_path(abs_path.to_string_lossy().as_ref(), cx)
                            })
                        });

                        // Directories were already opened earlier with `find_or_create_worktree`.
                        if let Ok(task) = abs_path_task
                            && task.await.is_none_or(|p| p.is_file())
                        {
                            return Some((
                                ix,
                                workspace
                                    .update_in(cx, |workspace, window, cx| {
                                        workspace.open_path_in_tabbed_pane(
                                            file_project_path,
                                            None,
                                            true,
                                            window,
                                            cx,
                                        )
                                    })
                                    .log_err()?
                                    .await,
                            ));
                        }
                        None
                    })
                });

        let tasks = tasks.collect::<Vec<_>>();

        let tasks = futures::future::join_all(tasks);
        for (ix, path_open_result) in tasks.await.into_iter().flatten() {
            opened_items[ix] = Some(path_open_result);
        }

        Ok(opened_items)
    })
}
