use super::*;

impl LspStore {
    pub(super) fn on_lsp_progress(
        &mut self,
        progress_params: lsp::ProgressParams,
        language_server_id: LanguageServerId,
        disk_based_diagnostics_progress_token: Option<String>,
        cx: &mut Context<Self>,
    ) {
        match progress_params.value {
            lsp::ProgressParamsValue::WorkDone(progress) => {
                self.handle_work_done_progress(
                    progress,
                    language_server_id,
                    disk_based_diagnostics_progress_token,
                    ProgressToken::from_lsp(progress_params.token),
                    cx,
                );
            }
            lsp::ProgressParamsValue::WorkspaceDiagnostic(report) => {
                let registration_id = match progress_params.token {
                    lsp::NumberOrString::Number(_) => None,
                    lsp::NumberOrString::String(token) => token
                        .split_once(WORKSPACE_DIAGNOSTICS_TOKEN_START)
                        .map(|(_, id)| id.to_owned()),
                };
                if let Some(LanguageServerState::Running {
                    workspace_diagnostics_refresh_tasks,
                    ..
                }) = self
                    .as_local_mut()
                    .and_then(|local| local.language_servers.get_mut(&language_server_id))
                    && let Some(workspace_diagnostics) =
                        workspace_diagnostics_refresh_tasks.get_mut(&registration_id)
                {
                    workspace_diagnostics.progress_tx.try_send(()).ok();
                    self.apply_workspace_diagnostic_report(
                        language_server_id,
                        report,
                        registration_id.map(SharedString::from),
                        cx,
                    )
                }
            }
        }
    }

    pub(super) fn handle_work_done_progress(
        &mut self,
        progress: lsp::WorkDoneProgress,
        language_server_id: LanguageServerId,
        disk_based_diagnostics_progress_token: Option<String>,
        token: ProgressToken,
        cx: &mut Context<Self>,
    ) {
        let language_server_status =
            if let Some(status) = self.language_server_statuses.get_mut(&language_server_id) {
                status
            } else {
                return;
            };

        if !language_server_status.progress_tokens.contains(&token) {
            return;
        }

        let is_disk_based_diagnostics_progress =
            if let (Some(disk_based_token), ProgressToken::String(token)) =
                (&disk_based_diagnostics_progress_token, &token)
            {
                token.starts_with(disk_based_token)
            } else {
                false
            };

        match progress {
            lsp::WorkDoneProgress::Begin(report) => {
                if is_disk_based_diagnostics_progress {
                    self.disk_based_diagnostics_started(language_server_id, cx);
                }
                self.on_lsp_work_start(
                    language_server_id,
                    token.clone(),
                    LanguageServerProgress {
                        title: Some(report.title),
                        is_disk_based_diagnostics_progress,
                        is_cancellable: report.cancellable.unwrap_or(false),
                        message: report.message.clone(),
                        percentage: report.percentage.map(|p| p as usize),
                        last_update_at: cx.background_executor().now(),
                    },
                    cx,
                );
            }
            lsp::WorkDoneProgress::Report(report) => self.on_lsp_work_progress(
                language_server_id,
                token,
                LanguageServerProgress {
                    title: None,
                    is_disk_based_diagnostics_progress,
                    is_cancellable: report.cancellable.unwrap_or(false),
                    message: report.message,
                    percentage: report.percentage.map(|p| p as usize),
                    last_update_at: cx.background_executor().now(),
                },
                cx,
            ),
            lsp::WorkDoneProgress::End(_) => {
                language_server_status.progress_tokens.remove(&token);
                self.on_lsp_work_end(language_server_id, token.clone(), cx);
                if is_disk_based_diagnostics_progress {
                    self.disk_based_diagnostics_finished(language_server_id, cx);
                }
            }
        }
    }

    pub(super) fn on_lsp_work_start(
        &mut self,
        language_server_id: LanguageServerId,
        token: ProgressToken,
        progress: LanguageServerProgress,
        cx: &mut Context<Self>,
    ) {
        if let Some(status) = self.language_server_statuses.get_mut(&language_server_id) {
            status.pending_work.insert(token.clone(), progress.clone());
            cx.notify();
        }
        cx.emit(LspStoreEvent::LanguageServerUpdate {
            language_server_id,
            name: self
                .language_server_adapter_for_id(language_server_id)
                .map(|adapter| adapter.name()),
            message: proto::update_language_server::Variant::WorkStart(proto::LspWorkStart {
                token: Some(token.to_proto()),
                title: progress.title,
                message: progress.message,
                percentage: progress.percentage.map(|p| p as u32),
                is_cancellable: Some(progress.is_cancellable),
            }),
        })
    }

    pub(super) fn on_lsp_work_progress(
        &mut self,
        language_server_id: LanguageServerId,
        token: ProgressToken,
        progress: LanguageServerProgress,
        cx: &mut Context<Self>,
    ) {
        let mut did_update = false;
        if let Some(status) = self.language_server_statuses.get_mut(&language_server_id) {
            match status.pending_work.entry(token.clone()) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(progress.clone());
                    did_update = true;
                }
                btree_map::Entry::Occupied(mut entry) => {
                    let entry = entry.get_mut();
                    if (progress.last_update_at - entry.last_update_at)
                        >= SERVER_PROGRESS_THROTTLE_TIMEOUT
                    {
                        entry.last_update_at = progress.last_update_at;
                        if progress.message.is_some() {
                            entry.message = progress.message.clone();
                        }
                        if progress.percentage.is_some() {
                            entry.percentage = progress.percentage;
                        }
                        if progress.is_cancellable != entry.is_cancellable {
                            entry.is_cancellable = progress.is_cancellable;
                        }
                        did_update = true;
                    }
                }
            }
        }

        if did_update {
            cx.emit(LspStoreEvent::LanguageServerUpdate {
                language_server_id,
                name: self
                    .language_server_adapter_for_id(language_server_id)
                    .map(|adapter| adapter.name()),
                message: proto::update_language_server::Variant::WorkProgress(
                    proto::LspWorkProgress {
                        token: Some(token.to_proto()),
                        message: progress.message,
                        percentage: progress.percentage.map(|p| p as u32),
                        is_cancellable: Some(progress.is_cancellable),
                    },
                ),
            })
        }
    }

    pub(super) fn on_lsp_work_end(
        &mut self,
        language_server_id: LanguageServerId,
        token: ProgressToken,
        cx: &mut Context<Self>,
    ) {
        if let Some(status) = self.language_server_statuses.get_mut(&language_server_id) {
            if let Some(work) = status.pending_work.remove(&token)
                && !work.is_disk_based_diagnostics_progress
            {
                cx.emit(LspStoreEvent::RefreshInlayHints {
                    server_id: language_server_id,
                    request_id: None,
                });
            }
            cx.notify();
        }

        cx.emit(LspStoreEvent::LanguageServerUpdate {
            language_server_id,
            name: self
                .language_server_adapter_for_id(language_server_id)
                .map(|adapter| adapter.name()),
            message: proto::update_language_server::Variant::WorkEnd(proto::LspWorkEnd {
                token: Some(token.to_proto()),
            }),
        })
    }
}
