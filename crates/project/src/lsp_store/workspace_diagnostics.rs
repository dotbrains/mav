use futures::{
    FutureExt, StreamExt,
    channel::oneshot,
    future::{Either, pending, select},
    stream::FuturesUnordered,
};
use gpui::{App, AppContext, Context, SharedString, Task};
use language::{DiagnosticSourceKind, LocalFile as _};
use lsp::{DEFAULT_LSP_REQUEST_TIMEOUT, DiagnosticServerCapabilities, LanguageServer};
use postage::{mpsc, sink::Sink, stream::Stream};
use settings::Settings;
use std::{borrow::Cow, future::ready, path::PathBuf, pin::pin, sync::Arc, time::Duration};
use util::{ConnectionResult, ResultExt as _};
use worktree::File;

use collections::{HashMap, HashSet};

use super::{
    DocumentDiagnosticsUpdate, LspStore,
    server_state::{LanguageServerState, WorkspaceRefreshTask},
};
use crate::{
    LspPullDiagnostics, ProjectPath, PulledDiagnostics,
    lsp_command::{GetDocumentDiagnostics, file_path_to_lsp_url},
    project_settings::ProjectSettings,
};

pub(super) const WORKSPACE_DIAGNOSTICS_TOKEN_START: &str = "id:";

pub(super) fn lsp_workspace_diagnostics_refresh(
    registration_id: Option<String>,
    options: DiagnosticServerCapabilities,
    server: Arc<LanguageServer>,
    cx: &mut Context<'_, LspStore>,
) -> Option<WorkspaceRefreshTask> {
    let identifier = workspace_diagnostic_identifier(&options)?;
    let registration_id_shared = registration_id.as_ref().map(SharedString::from);

    let (progress_tx, mut progress_rx) = mpsc::channel(1);
    let (mut refresh_tx, mut refresh_rx) = mpsc::channel::<Option<oneshot::Sender<bool>>>(1);
    refresh_tx.try_send(None).ok();

    let request_timeout = ProjectSettings::get_global(cx)
        .global_lsp_settings
        .get_request_timeout();

    let timeout = if request_timeout != Duration::ZERO {
        request_timeout.max(DEFAULT_LSP_REQUEST_TIMEOUT)
    } else {
        request_timeout
    };

    let workspace_query_language_server = cx.spawn(async move |lsp_store, cx| {
        let mut attempts = 0;
        let max_attempts = 50;
        let mut requests = 0;

        loop {
            let Some(mut completion_tx) = refresh_rx.recv().await else {
                return;
            };

            'request: loop {
                requests += 1;
                if attempts > max_attempts {
                    log::error!(
                        "Failed to pull workspace diagnostics {max_attempts} times, aborting"
                    );
                    return;
                }
                let backoff_millis = (50 * (1 << attempts)).clamp(30, 1000);
                cx.background_executor()
                    .timer(Duration::from_millis(backoff_millis))
                    .await;
                attempts += 1;

                let Ok(previous_result_ids) = lsp_store.update(cx, |lsp_store, _| {
                    lsp_store
                        .result_ids_for_workspace_refresh(server.server_id(), &registration_id_shared)
                        .into_iter()
                        .filter_map(|(abs_path, result_id)| {
                            let uri = file_path_to_lsp_url(&abs_path).ok()?;
                            Some(lsp::PreviousResultId {
                                uri,
                                value: result_id.to_string(),
                            })
                        })
                        .collect()
                }) else {
                    return;
                };

                let token = if let Some(registration_id) = &registration_id {
                    format!(
                        "workspace/diagnostic/{}/{requests}/{WORKSPACE_DIAGNOSTICS_TOKEN_START}{registration_id}",
                        server.server_id(),
                    )
                } else {
                    format!("workspace/diagnostic/{}/{requests}", server.server_id())
                };

                progress_rx.try_recv().ok();
                let timer = server.request_timer(timeout).fuse();
                let progress = pin!(progress_rx.recv().fuse());
                let response_result = server
                    .request_with_timer::<lsp::WorkspaceDiagnosticRequest, _>(
                        lsp::WorkspaceDiagnosticParams {
                            previous_result_ids,
                            identifier: identifier.clone(),
                            work_done_progress_params: Default::default(),
                            partial_result_params: lsp::PartialResultParams {
                                partial_result_token: Some(lsp::ProgressToken::String(token)),
                            },
                        },
                        select(timer, progress).then(|either| match either {
                            Either::Left((message, ..)) => ready(message).left_future(),
                            Either::Right(..) => pending::<String>().right_future(),
                        }),
                    )
                    .await;

                match response_result {
                    ConnectionResult::Timeout => {
                        log::error!("Timeout during workspace diagnostics pull");
                        continue 'request;
                    }
                    ConnectionResult::ConnectionReset => {
                        log::error!("Server closed a workspace diagnostics pull request");
                        continue 'request;
                    }
                    ConnectionResult::Result(Err(e)) => {
                        log::error!("Error during workspace diagnostics pull: {e:#}");
                        if let Some(tx) = completion_tx.take() {
                            tx.send(false).ok();
                        }
                        break 'request;
                    }
                    ConnectionResult::Result(Ok(pulled_diagnostics)) => {
                        attempts = 0;
                        if lsp_store
                            .update(cx, |lsp_store, cx| {
                                lsp_store.apply_workspace_diagnostic_report(
                                    server.server_id(),
                                    pulled_diagnostics,
                                    registration_id_shared.clone(),
                                    cx,
                                )
                            })
                            .is_err()
                        {
                            return;
                        }
                        if let Some(tx) = completion_tx.take() {
                            tx.send(true).ok();
                        }
                        break 'request;
                    }
                }
            }
        }
    });

    Some(WorkspaceRefreshTask {
        refresh_tx,
        progress_tx,
        task: workspace_query_language_server,
    })
}

pub(super) fn buffer_diagnostic_identifier(
    options: &DiagnosticServerCapabilities,
) -> Option<SharedString> {
    match &options {
        lsp::DiagnosticServerCapabilities::Options(diagnostic_options) => diagnostic_options
            .identifier
            .as_deref()
            .map(SharedString::new),
        lsp::DiagnosticServerCapabilities::RegistrationOptions(registration_options) => {
            let diagnostic_options = &registration_options.diagnostic_options;
            diagnostic_options
                .identifier
                .as_deref()
                .map(SharedString::new)
        }
    }
}

fn workspace_diagnostic_identifier(
    options: &DiagnosticServerCapabilities,
) -> Option<Option<String>> {
    match &options {
        lsp::DiagnosticServerCapabilities::Options(diagnostic_options) => {
            if !diagnostic_options.workspace_diagnostics {
                return None;
            }
            Some(diagnostic_options.identifier.clone())
        }
        lsp::DiagnosticServerCapabilities::RegistrationOptions(registration_options) => {
            let diagnostic_options = &registration_options.diagnostic_options;
            if !diagnostic_options.workspace_diagnostics {
                return None;
            }
            Some(diagnostic_options.identifier.clone())
        }
    }
}

impl LspStore {
    pub(crate) fn cleanup_lsp_data(&mut self, for_server: lsp::LanguageServerId) {
        self.lsp_server_capabilities.remove(&for_server);
        self.semantic_token_config.remove_server_data(for_server);
        for lsp_data in self.lsp_data.values_mut() {
            lsp_data.remove_server_data(for_server);
        }
        if let Some(local) = self.as_local_mut() {
            local.buffer_pull_diagnostics_result_ids.remove(&for_server);
            local
                .workspace_pull_diagnostics_result_ids
                .remove(&for_server);
            for buffer_servers in local.buffers_opened_in_servers.values_mut() {
                buffer_servers.remove(&for_server);
            }
        }
    }

    pub fn result_id_for_buffer_pull(
        &self,
        server_id: lsp::LanguageServerId,
        buffer_id: text::BufferId,
        registration_id: &Option<SharedString>,
        cx: &App,
    ) -> Option<SharedString> {
        let abs_path = self
            .buffer_store
            .read(cx)
            .get(buffer_id)
            .and_then(|b| File::from_dyn(b.read(cx).file()))
            .map(|f| f.abs_path(cx))?;
        self.as_local()?
            .buffer_pull_diagnostics_result_ids
            .get(&server_id)?
            .get(registration_id)?
            .get(&abs_path)?
            .clone()
    }

    /// Gets all result_ids for a workspace diagnostics pull request.
    /// First, it tries to find buffer's result_id retrieved via the diagnostics pull; if it fails, it falls back to the workspace diagnostics pull result_id.
    /// The latter is lower priority because diagnostics for open buffers are pulled eagerly.
    pub fn result_ids_for_workspace_refresh(
        &self,
        server_id: lsp::LanguageServerId,
        registration_id: &Option<SharedString>,
    ) -> HashMap<PathBuf, SharedString> {
        let Some(local) = self.as_local() else {
            return HashMap::default();
        };
        local
            .workspace_pull_diagnostics_result_ids
            .get(&server_id)
            .into_iter()
            .filter_map(|diagnostics| diagnostics.get(registration_id))
            .flatten()
            .filter_map(|(abs_path, result_id)| {
                let result_id = local
                    .buffer_pull_diagnostics_result_ids
                    .get(&server_id)
                    .and_then(|buffer_ids_result_ids| {
                        buffer_ids_result_ids.get(registration_id)?.get(abs_path)
                    })
                    .cloned()
                    .flatten()
                    .or_else(|| result_id.clone())?;
                Some((abs_path.clone(), result_id))
            })
            .collect()
    }

    pub fn pull_workspace_diagnostics(&mut self, server_id: lsp::LanguageServerId) {
        if let Some(LanguageServerState::Running {
            workspace_diagnostics_refresh_tasks,
            ..
        }) = self
            .as_local_mut()
            .and_then(|local| local.language_servers.get_mut(&server_id))
        {
            for diagnostics in workspace_diagnostics_refresh_tasks.values_mut() {
                diagnostics.refresh_tx.try_send(None).ok();
            }
        }
    }

    /// Triggers a workspace diagnostics pull on all running language servers
    /// and returns a [`Task`] that resolves once the requests have completed.
    ///
    /// This reuses the same background refresh loops as
    /// [`Self::pull_workspace_diagnostics`], but provides a completion signal
    /// so callers can wait for fresh diagnostics before reading them.
    pub fn pull_workspace_diagnostics_once(&mut self, cx: &mut Context<Self>) -> Task<bool> {
        let Some(local) = self.as_local_mut() else {
            return Task::ready(true);
        };

        let mut receivers = Vec::new();
        for state in local.language_servers.values_mut() {
            let LanguageServerState::Running {
                workspace_diagnostics_refresh_tasks,
                ..
            } = state
            else {
                continue;
            };
            for task in workspace_diagnostics_refresh_tasks.values_mut() {
                let (tx, rx) = oneshot::channel();
                task.refresh_tx.try_send(Some(tx)).ok();
                receivers.push(rx);
            }
        }

        cx.background_spawn(async {
            FuturesUnordered::from_iter(receivers)
                .all(async |result| result.unwrap_or(false))
                .await
        })
    }

    /// Refreshes `textDocument/diagnostic` for all open buffers associated with the given server.
    /// This is called in response to `workspace/diagnostic/refresh` to comply with the LSP spec,
    /// which requires refreshing both workspace and document diagnostics.
    pub fn pull_document_diagnostics_for_server(
        &mut self,
        server_id: lsp::LanguageServerId,
        source_buffer_id: Option<text::BufferId>,
        cx: &mut Context<Self>,
    ) -> futures::future::Shared<Task<()>> {
        let Some(local) = self.as_local_mut() else {
            return Task::ready(()).shared();
        };
        let mut buffers_to_refresh = HashSet::default();
        for (buffer_id, server_ids) in &local.buffers_opened_in_servers {
            if server_ids.contains(&server_id) && Some(buffer_id) != source_buffer_id.as_ref() {
                buffers_to_refresh.insert(*buffer_id);
            }
        }

        self.refresh_background_diagnostics_for_buffers(buffers_to_refresh, cx)
    }

    pub fn pull_document_diagnostics_for_buffer_edit(
        &mut self,
        buffer_id: text::BufferId,
        cx: &mut Context<Self>,
    ) {
        let Some(local) = self.as_local_mut() else {
            return;
        };
        let Some(languages_servers) = local.buffers_opened_in_servers.get(&buffer_id).cloned()
        else {
            return;
        };
        for server_id in languages_servers {
            let _ = self.pull_document_diagnostics_for_server(server_id, Some(buffer_id), cx);
        }
    }

    pub(crate) fn apply_workspace_diagnostic_report(
        &mut self,
        server_id: lsp::LanguageServerId,
        report: lsp::WorkspaceDiagnosticReportResult,
        registration_id: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let mut workspace_diagnostics =
            GetDocumentDiagnostics::deserialize_workspace_diagnostics_report(
                report,
                server_id,
                registration_id,
            );
        workspace_diagnostics.retain(|d| match &d.diagnostics {
            LspPullDiagnostics::Response {
                server_id,
                registration_id,
                ..
            } => self.diagnostic_registration_exists(*server_id, &registration_id),
            LspPullDiagnostics::Default => false,
        });
        let mut unchanged_buffers = HashMap::default();
        let workspace_diagnostics_updates = workspace_diagnostics
            .into_iter()
            .filter_map(
                |workspace_diagnostics| match workspace_diagnostics.diagnostics {
                    LspPullDiagnostics::Response {
                        server_id,
                        uri,
                        diagnostics,
                        registration_id,
                    } => Some((
                        server_id,
                        uri,
                        diagnostics,
                        workspace_diagnostics.version,
                        registration_id,
                    )),
                    LspPullDiagnostics::Default => None,
                },
            )
            .fold(
                HashMap::default(),
                |mut acc, (server_id, uri, diagnostics, version, new_registration_id)| {
                    let (result_id, diagnostics) = match diagnostics {
                        PulledDiagnostics::Unchanged { result_id } => {
                            unchanged_buffers
                                .entry(new_registration_id.clone())
                                .or_insert_with(HashSet::default)
                                .insert(uri.clone());
                            (Some(result_id), Vec::new())
                        }
                        PulledDiagnostics::Changed {
                            result_id,
                            diagnostics,
                        } => (result_id, diagnostics),
                    };
                    let disk_based_sources = Cow::Owned(
                        self.language_server_adapter_for_id(server_id)
                            .as_ref()
                            .map(|adapter| adapter.disk_based_diagnostic_sources.as_slice())
                            .unwrap_or(&[])
                            .to_vec(),
                    );

                    let Some(abs_path) = uri.to_file_path().ok() else {
                        return acc;
                    };
                    let Some((worktree, relative_path)) =
                        self.worktree_store.read(cx).find_worktree(abs_path.clone(), cx)
                    else {
                        log::warn!("skipping workspace diagnostics update, no worktree found for path {abs_path:?}");
                        return acc;
                    };
                    let worktree_id = worktree.read(cx).id();
                    let project_path = ProjectPath {
                        worktree_id,
                        path: relative_path,
                    };
                    if let Some(local_lsp_store) = self.as_local_mut() {
                        local_lsp_store
                            .workspace_pull_diagnostics_result_ids
                            .entry(server_id)
                            .or_default()
                            .entry(new_registration_id.clone())
                            .or_default()
                            .insert(abs_path, result_id.clone());
                    }
                    // The LSP spec recommends that document pulls win over workspace pulls.
                    // Open buffers are pulled eagerly, so workspace pull contents for them are ignored.
                    if self.buffer_store.read(cx).get_by_path(&project_path).is_none() {
                        acc.entry(server_id)
                            .or_insert_with(HashMap::default)
                            .entry(new_registration_id.clone())
                            .or_insert_with(Vec::new)
                            .push(DocumentDiagnosticsUpdate {
                                server_id,
                                diagnostics: lsp::PublishDiagnosticsParams {
                                    uri,
                                    diagnostics,
                                    version,
                                },
                                result_id: result_id.map(SharedString::new),
                                disk_based_sources,
                                registration_id: new_registration_id,
                            });
                    }
                    acc
                },
            );

        for diagnostic_updates in workspace_diagnostics_updates.into_values() {
            for (registration_id, diagnostic_updates) in diagnostic_updates {
                self.merge_lsp_diagnostics(
                    DiagnosticSourceKind::Pulled,
                    diagnostic_updates,
                    |document_uri, old_diagnostic, _| match old_diagnostic.source_kind {
                        DiagnosticSourceKind::Pulled => {
                            old_diagnostic.registration_id != registration_id
                                || unchanged_buffers
                                    .get(&old_diagnostic.registration_id)
                                    .is_some_and(|unchanged_buffers| {
                                        unchanged_buffers.contains(&document_uri)
                                    })
                        }
                        DiagnosticSourceKind::Other | DiagnosticSourceKind::Pushed => true,
                    },
                    cx,
                )
                .log_err();
            }
        }
    }
}
