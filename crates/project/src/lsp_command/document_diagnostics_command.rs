use super::*;

impl LspCommand for GetDocumentDiagnostics {
    type Response = Vec<LspPullDiagnostics>;
    type LspRequest = lsp::request::DocumentDiagnosticRequest;
    type ProtoRequest = proto::GetDocumentDiagnostics;

    fn display_name(&self) -> &str {
        "Get diagnostics"
    }

    fn check_capabilities(&self, _: AdapterServerCapabilities) -> bool {
        true
    }

    fn to_lsp(
        &self,
        path: &Path,
        _: &Buffer,
        _: &Arc<LanguageServer>,
        _: &App,
    ) -> Result<lsp::DocumentDiagnosticParams> {
        Ok(lsp::DocumentDiagnosticParams {
            text_document: lsp::TextDocumentIdentifier {
                uri: file_path_to_lsp_url(path)?,
            },
            identifier: self.identifier.as_ref().map(ToString::to_string),
            previous_result_id: self.previous_result_id.as_ref().map(ToString::to_string),
            partial_result_params: Default::default(),
            work_done_progress_params: Default::default(),
        })
    }

    async fn response_from_lsp(
        self,
        message: lsp::DocumentDiagnosticReportResult,
        _: Entity<LspStore>,
        buffer: Entity<Buffer>,
        server_id: LanguageServerId,
        cx: AsyncApp,
    ) -> Result<Self::Response> {
        let url = buffer.read_with(&cx, |buffer, cx| {
            buffer
                .file()
                .and_then(|file| file.as_local())
                .map(|file| {
                    let abs_path = file.abs_path(cx);
                    file_path_to_lsp_url(&abs_path)
                })
                .transpose()?
                .with_context(|| format!("missing url on buffer {}", buffer.remote_id()))
        })?;

        let mut pulled_diagnostics = HashMap::default();
        match message {
            lsp::DocumentDiagnosticReportResult::Report(report) => match report {
                lsp::DocumentDiagnosticReport::Full(report) => {
                    if let Some(related_documents) = report.related_documents {
                        process_related_documents(
                            &mut pulled_diagnostics,
                            server_id,
                            related_documents,
                            self.registration_id.clone(),
                        );
                    }
                    process_full_diagnostics_report(
                        &mut pulled_diagnostics,
                        server_id,
                        url,
                        report.full_document_diagnostic_report,
                        self.registration_id,
                    );
                }
                lsp::DocumentDiagnosticReport::Unchanged(report) => {
                    if let Some(related_documents) = report.related_documents {
                        process_related_documents(
                            &mut pulled_diagnostics,
                            server_id,
                            related_documents,
                            self.registration_id.clone(),
                        );
                    }
                    process_unchanged_diagnostics_report(
                        &mut pulled_diagnostics,
                        server_id,
                        url,
                        report.unchanged_document_diagnostic_report,
                        self.registration_id,
                    );
                }
            },
            lsp::DocumentDiagnosticReportResult::Partial(report) => {
                if let Some(related_documents) = report.related_documents {
                    process_related_documents(
                        &mut pulled_diagnostics,
                        server_id,
                        related_documents,
                        self.registration_id,
                    );
                }
            }
        }

        Ok(pulled_diagnostics.into_values().collect())
    }

    fn to_proto(&self, project_id: u64, buffer: &Buffer) -> proto::GetDocumentDiagnostics {
        proto::GetDocumentDiagnostics {
            project_id,
            buffer_id: buffer.remote_id().into(),
            version: serialize_version(&buffer.version()),
        }
    }

    async fn from_proto(
        _: proto::GetDocumentDiagnostics,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self> {
        anyhow::bail!(
            "proto::GetDocumentDiagnostics is not expected to be converted from proto directly, as it needs `previous_result_id` fetched first"
        )
    }

    fn response_to_proto(
        response: Self::Response,
        _: &mut LspStore,
        _: PeerId,
        _: &clock::Global,
        _: &mut App,
    ) -> proto::GetDocumentDiagnosticsResponse {
        let pulled_diagnostics = response
            .into_iter()
            .filter_map(|diagnostics| match diagnostics {
                LspPullDiagnostics::Default => None,
                LspPullDiagnostics::Response {
                    server_id,
                    uri,
                    diagnostics,
                    registration_id,
                } => {
                    let mut changed = false;
                    let (diagnostics, result_id) = match diagnostics {
                        PulledDiagnostics::Unchanged { result_id } => (Vec::new(), Some(result_id)),
                        PulledDiagnostics::Changed {
                            result_id,
                            diagnostics,
                        } => {
                            changed = true;
                            (diagnostics, result_id)
                        }
                    };
                    Some(proto::PulledDiagnostics {
                        changed,
                        result_id: result_id.map(|id| id.to_string()),
                        uri: uri.to_string(),
                        server_id: server_id.to_proto(),
                        diagnostics: diagnostics
                            .into_iter()
                            .filter_map(|diagnostic| {
                                GetDocumentDiagnostics::serialize_lsp_diagnostic(diagnostic)
                                    .context("serializing diagnostics")
                                    .log_err()
                            })
                            .collect(),
                        registration_id: registration_id.as_ref().map(ToString::to_string),
                    })
                }
            })
            .collect();

        proto::GetDocumentDiagnosticsResponse { pulled_diagnostics }
    }

    async fn response_from_proto(
        self,
        response: proto::GetDocumentDiagnosticsResponse,
        _: Entity<LspStore>,
        _: Entity<Buffer>,
        _: AsyncApp,
    ) -> Result<Self::Response> {
        Ok(Self::diagnostics_from_proto(response))
    }

    fn buffer_id_from_proto(message: &proto::GetDocumentDiagnostics) -> Result<BufferId> {
        BufferId::new(message.buffer_id)
    }
}
