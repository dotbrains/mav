use std::{path::Path, sync::Arc};

use anyhow::Context as _;
use gpui::{App, AsyncApp, Context, Entity, EventEmitter, Task, WeakEntity};
use language::{LanguageToolchainStore, Location, proto::deserialize_anchor};
use rpc::{AnyProtoClient, TypedEnvelope, proto};
use settings::{InvalidSettingsError, SettingsLocation};
use task::{TaskContext, TaskVariables, VariableName};
use text::{BufferId, OffsetRangeExt};

use crate::{
    Inventory, ProjectEnvironment, buffer_store::BufferStore, git_store::GitStore,
    worktree_store::WorktreeStore,
};

mod context;
use context::{local_task_context_for_location, remote_task_context_for_location};

// platform-dependent warning
pub enum TaskStore {
    Functional(StoreState),
    Noop,
}

pub struct StoreState {
    mode: StoreMode,
    task_inventory: Entity<Inventory>,
    buffer_store: WeakEntity<BufferStore>,
    worktree_store: Entity<WorktreeStore>,
    git_store: Entity<GitStore>,
    toolchain_store: Arc<dyn LanguageToolchainStore>,
}

enum StoreMode {
    Local {
        downstream_client: Option<(AnyProtoClient, u64)>,
        environment: Entity<ProjectEnvironment>,
    },
    Remote {
        upstream_client: AnyProtoClient,
        project_id: u64,
    },
}

impl EventEmitter<crate::Event> for TaskStore {}

#[derive(Debug)]
pub enum TaskSettingsLocation<'a> {
    Global(&'a Path),
    Worktree(SettingsLocation<'a>),
}

impl TaskStore {
    pub fn init(client: Option<&AnyProtoClient>) {
        if let Some(client) = client {
            client.add_entity_request_handler(Self::handle_task_context_for_location);
        }
    }

    async fn handle_task_context_for_location(
        store: Entity<Self>,
        envelope: TypedEnvelope<proto::TaskContextForLocation>,
        mut cx: AsyncApp,
    ) -> anyhow::Result<proto::TaskContext> {
        let location = envelope
            .payload
            .location
            .context("no location given for task context handling")?;
        let (buffer_store, is_remote) = store.read_with(&cx, |store, _| {
            Ok(match store {
                TaskStore::Functional(state) => (
                    state.buffer_store.clone(),
                    match &state.mode {
                        StoreMode::Local { .. } => false,
                        StoreMode::Remote { .. } => true,
                    },
                ),
                TaskStore::Noop => {
                    anyhow::bail!("empty task store cannot handle task context requests")
                }
            })
        })?;
        let buffer_store = buffer_store
            .upgrade()
            .context("no buffer store when handling task context request")?;

        let buffer_id = BufferId::new(location.buffer_id).with_context(|| {
            format!(
                "cannot handle task context request for invalid buffer id: {}",
                location.buffer_id
            )
        })?;

        let start = location
            .start
            .and_then(deserialize_anchor)
            .context("missing task context location start")?;
        let end = location
            .end
            .and_then(deserialize_anchor)
            .context("missing task context location end")?;
        let buffer = buffer_store
            .update(&mut cx, |buffer_store, cx| {
                if is_remote {
                    buffer_store.wait_for_remote_buffer(buffer_id, cx)
                } else {
                    Task::ready(
                        buffer_store
                            .get(buffer_id)
                            .with_context(|| format!("no local buffer with id {buffer_id}")),
                    )
                }
            })
            .await?;

        let location = Location {
            buffer,
            range: start..end,
        };
        let context_task = store.update(&mut cx, |store, cx| {
            let captured_variables = {
                let mut variables = TaskVariables::from_iter(
                    envelope
                        .payload
                        .task_variables
                        .into_iter()
                        .filter_map(|(k, v)| Some((k.parse().ok()?, v))),
                );

                let snapshot = location.buffer.read(cx).snapshot();
                let range = location.range.to_offset(&snapshot);

                for range in snapshot.runnable_ranges(range) {
                    for (capture_name, value) in range.extra_captures {
                        variables.insert(VariableName::Custom(capture_name.into()), value);
                    }
                }
                variables
            };
            store.task_context_for_location(captured_variables, location, cx)
        });
        let task_context = context_task.await?.unwrap_or_default();
        Ok(proto::TaskContext {
            project_env: task_context.project_env.into_iter().collect(),
            cwd: task_context
                .cwd
                .map(|cwd| cwd.to_string_lossy().into_owned()),
            task_variables: task_context
                .task_variables
                .into_iter()
                .map(|(variable_name, variable_value)| (variable_name.to_string(), variable_value))
                .collect(),
        })
    }

    pub fn local(
        buffer_store: WeakEntity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        toolchain_store: Arc<dyn LanguageToolchainStore>,
        environment: Entity<ProjectEnvironment>,
        git_store: Entity<GitStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::Functional(StoreState {
            mode: StoreMode::Local {
                downstream_client: None,
                environment,
            },
            task_inventory: Inventory::new(cx),
            buffer_store,
            git_store,
            toolchain_store,
            worktree_store,
        })
    }

    pub fn remote(
        buffer_store: WeakEntity<BufferStore>,
        worktree_store: Entity<WorktreeStore>,
        toolchain_store: Arc<dyn LanguageToolchainStore>,
        upstream_client: AnyProtoClient,
        project_id: u64,
        git_store: Entity<GitStore>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::Functional(StoreState {
            mode: StoreMode::Remote {
                upstream_client,
                project_id,
            },
            task_inventory: Inventory::new(cx),
            buffer_store,
            git_store,
            toolchain_store,
            worktree_store,
        })
    }

    pub fn task_context_for_location(
        &self,
        captured_variables: TaskVariables,
        location: Location,
        cx: &mut App,
    ) -> Task<anyhow::Result<Option<TaskContext>>> {
        match self {
            TaskStore::Functional(state) => match &state.mode {
                StoreMode::Local { environment, .. } => local_task_context_for_location(
                    state.worktree_store.clone(),
                    state.git_store.clone(),
                    state.toolchain_store.clone(),
                    environment.clone(),
                    captured_variables,
                    location,
                    cx,
                ),
                StoreMode::Remote {
                    upstream_client,
                    project_id,
                } => remote_task_context_for_location(
                    *project_id,
                    upstream_client.clone(),
                    state.worktree_store.clone(),
                    state.git_store.clone(),
                    captured_variables,
                    location,
                    state.toolchain_store.clone(),
                    cx,
                ),
            },
            TaskStore::Noop => Task::ready(Ok(None)),
        }
    }

    pub fn task_inventory(&self) -> Option<&Entity<Inventory>> {
        match self {
            TaskStore::Functional(state) => Some(&state.task_inventory),
            TaskStore::Noop => None,
        }
    }

    pub fn shared(&mut self, remote_id: u64, new_downstream_client: AnyProtoClient, _cx: &mut App) {
        if let Self::Functional(StoreState {
            mode: StoreMode::Local {
                downstream_client, ..
            },
            ..
        }) = self
        {
            *downstream_client = Some((new_downstream_client, remote_id));
        }
    }

    pub fn unshared(&mut self, _: &mut Context<Self>) {
        if let Self::Functional(StoreState {
            mode: StoreMode::Local {
                downstream_client, ..
            },
            ..
        }) = self
        {
            *downstream_client = None;
        }
    }

    pub(super) fn update_user_tasks(
        &self,
        location: TaskSettingsLocation<'_>,
        raw_tasks_json: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), InvalidSettingsError> {
        let task_inventory = match self {
            TaskStore::Functional(state) => &state.task_inventory,
            TaskStore::Noop => return Ok(()),
        };
        let raw_tasks_json = raw_tasks_json
            .map(|json| json.trim())
            .filter(|json| !json.is_empty());

        task_inventory.update(cx, |inventory, _| {
            inventory.update_file_based_tasks(location, raw_tasks_json)
        })
    }

    pub(super) fn update_user_debug_scenarios(
        &self,
        location: TaskSettingsLocation<'_>,
        raw_tasks_json: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), InvalidSettingsError> {
        let task_inventory = match self {
            TaskStore::Functional(state) => &state.task_inventory,
            TaskStore::Noop => return Ok(()),
        };
        let raw_tasks_json = raw_tasks_json
            .map(|json| json.trim())
            .filter(|json| !json.is_empty());

        task_inventory.update(cx, |inventory, _| {
            inventory.update_file_based_scenarios(location, raw_tasks_json)
        })
    }
}
