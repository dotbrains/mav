use std::{path::PathBuf, sync::Arc};

use anyhow::Context as _;
use collections::HashMap;
use fs::Fs;
use gpui::{App, Entity, Task};
use language::{
    ContextLocation, ContextProvider as _, LanguageToolchainStore, Location,
    proto::serialize_anchor,
};
use rpc::{AnyProtoClient, proto};
use task::{TaskContext, TaskVariables};
use util::ResultExt;

use crate::{
    BasicContextProvider, ProjectEnvironment, git_store::GitStore, worktree_store::WorktreeStore,
};

pub(super) fn local_task_context_for_location(
    worktree_store: Entity<WorktreeStore>,
    git_store: Entity<GitStore>,
    toolchain_store: Arc<dyn LanguageToolchainStore>,
    environment: Entity<ProjectEnvironment>,
    captured_variables: TaskVariables,
    location: Location,
    cx: &App,
) -> Task<anyhow::Result<Option<TaskContext>>> {
    let worktree_id = location.buffer.read(cx).file().map(|f| f.worktree_id(cx));
    let worktree_abs_path = worktree_id
        .and_then(|worktree_id| worktree_store.read(cx).worktree_for_id(worktree_id, cx))
        .and_then(|worktree| worktree.read(cx).root_dir());
    let fs = worktree_store.read(cx).fs();

    cx.spawn(async move |cx| {
        let project_env = environment
            .update(cx, |environment, cx| {
                environment.buffer_environment(&location.buffer, &worktree_store, cx)
            })
            .await;

        let mut task_variables = cx
            .update(|cx| {
                combine_task_variables(
                    captured_variables,
                    fs,
                    worktree_store.clone(),
                    location,
                    project_env.clone(),
                    BasicContextProvider::new(worktree_store, git_store),
                    toolchain_store,
                    cx,
                )
            })
            .await?;

        // Remove all custom entries starting with _, as they're not intended for use by the end user.
        task_variables.sweep();

        Ok(Some(TaskContext {
            project_env: project_env.unwrap_or_default(),
            cwd: worktree_abs_path.map(|p| p.to_path_buf()),
            task_variables,
        }))
    })
}

pub(super) fn remote_task_context_for_location(
    project_id: u64,
    upstream_client: AnyProtoClient,
    worktree_store: Entity<WorktreeStore>,
    git_store: Entity<GitStore>,
    captured_variables: TaskVariables,
    location: Location,
    toolchain_store: Arc<dyn LanguageToolchainStore>,
    cx: &mut App,
) -> Task<anyhow::Result<Option<TaskContext>>> {
    cx.spawn(async move |cx| {
        // We need to gather a client context, as the headless one may lack certain information (e.g. tree-sitter parsing is disabled there, so symbols are not available).
        let mut remote_context = cx
            .update(|cx| {
                let worktree_root = worktree_root(&worktree_store, &location, cx);

                BasicContextProvider::new(worktree_store, git_store).build_context(
                    &TaskVariables::default(),
                    ContextLocation {
                        fs: None,
                        worktree_root,
                        file_location: &location,
                    },
                    None,
                    toolchain_store,
                    cx,
                )
            })
            .await
            .log_err()
            .unwrap_or_default();
        remote_context.extend(captured_variables);

        let buffer_id = cx.update(|cx| location.buffer.read(cx).remote_id().to_proto());
        let context_task = upstream_client.request(proto::TaskContextForLocation {
            project_id,
            location: Some(proto::Location {
                buffer_id,
                start: Some(serialize_anchor(&location.range.start)),
                end: Some(serialize_anchor(&location.range.end)),
            }),
            task_variables: remote_context
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        });
        let task_context = context_task.await?;
        Ok(Some(TaskContext {
            cwd: task_context.cwd.map(PathBuf::from),
            task_variables: task_context
                .task_variables
                .into_iter()
                .filter_map(
                    |(variable_name, variable_value)| match variable_name.parse() {
                        Ok(variable_name) => Some((variable_name, variable_value)),
                        Err(()) => {
                            log::error!("Unknown variable name: {variable_name}");
                            None
                        }
                    },
                )
                .collect(),
            project_env: task_context.project_env.into_iter().collect(),
        }))
    })
}

fn worktree_root(
    worktree_store: &Entity<WorktreeStore>,
    location: &Location,
    cx: &mut App,
) -> Option<PathBuf> {
    location
        .buffer
        .read(cx)
        .file()
        .map(|f| f.worktree_id(cx))
        .and_then(|worktree_id| worktree_store.read(cx).worktree_for_id(worktree_id, cx))
        .and_then(|worktree| {
            let worktree = worktree.read(cx);
            if !worktree.is_visible() {
                return None;
            }
            let root_entry = worktree.root_entry()?;
            if !root_entry.is_dir() {
                return None;
            }
            Some(worktree.absolutize(&root_entry.path))
        })
}

fn combine_task_variables(
    mut captured_variables: TaskVariables,
    fs: Option<Arc<dyn Fs>>,
    worktree_store: Entity<WorktreeStore>,
    location: Location,
    project_env: Option<HashMap<String, String>>,
    baseline: BasicContextProvider,
    toolchain_store: Arc<dyn LanguageToolchainStore>,
    cx: &mut App,
) -> Task<anyhow::Result<TaskVariables>> {
    let language_context_provider = location
        .buffer
        .read(cx)
        .language()
        .and_then(|language| language.context_provider());
    cx.spawn(async move |cx| {
        let baseline = cx
            .update(|cx| {
                let worktree_root = worktree_root(&worktree_store, &location, cx);
                baseline.build_context(
                    &captured_variables,
                    ContextLocation {
                        fs: fs.clone(),
                        worktree_root,
                        file_location: &location,
                    },
                    project_env.clone(),
                    toolchain_store.clone(),
                    cx,
                )
            })
            .await
            .context("building basic default context")?;
        captured_variables.extend(baseline);
        if let Some(provider) = language_context_provider {
            captured_variables.extend(
                cx.update(|cx| {
                    let worktree_root = worktree_root(&worktree_store, &location, cx);
                    provider.build_context(
                        &captured_variables,
                        ContextLocation {
                            fs,
                            worktree_root,
                            file_location: &location,
                        },
                        project_env,
                        toolchain_store,
                        cx,
                    )
                })
                .await?,
            );
        }
        Ok(captured_variables)
    })
}
