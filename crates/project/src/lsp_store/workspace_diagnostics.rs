use futures::{
    FutureExt,
    channel::oneshot,
    future::{Either, pending, select},
};
use gpui::{Context, SharedString};
use lsp::{DEFAULT_LSP_REQUEST_TIMEOUT, DiagnosticServerCapabilities, LanguageServer};
use postage::{mpsc, sink::Sink, stream::Stream};
use settings::Settings;
use std::{future::ready, pin::pin, sync::Arc, time::Duration};
use util::ConnectionResult;

use super::{LspStore, server_state::WorkspaceRefreshTask};
use crate::{lsp_command::file_path_to_lsp_url, project_settings::ProjectSettings};

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
