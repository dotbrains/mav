use super::*;

impl LspStore {
    #[cfg(any(test, feature = "test-support"))]
    pub fn update_diagnostics(
        &mut self,
        server_id: LanguageServerId,
        diagnostics: lsp::PublishDiagnosticsParams,
        result_id: Option<SharedString>,
        source_kind: DiagnosticSourceKind,
        disk_based_sources: &[String],
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.merge_lsp_diagnostics(
            source_kind,
            vec![DocumentDiagnosticsUpdate {
                diagnostics,
                result_id,
                server_id,
                disk_based_sources: Cow::Borrowed(disk_based_sources),
                registration_id: None,
            }],
            |_, _, _| false,
            cx,
        )
    }

    pub fn merge_lsp_diagnostics(
        &mut self,
        source_kind: DiagnosticSourceKind,
        lsp_diagnostics: Vec<DocumentDiagnosticsUpdate<lsp::PublishDiagnosticsParams>>,
        merge: impl Fn(&lsp::Uri, &Diagnostic, &App) -> bool + Clone,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        anyhow::ensure!(self.mode.is_local(), "called update_diagnostics on remote");
        let updates = lsp_diagnostics
            .into_iter()
            .filter_map(|update| {
                let abs_path = update.diagnostics.uri.to_file_path().ok()?;
                Some(DocumentDiagnosticsUpdate {
                    diagnostics: self.lsp_to_document_diagnostics(
                        abs_path,
                        source_kind,
                        update.server_id,
                        update.diagnostics,
                        &update.disk_based_sources,
                        update.registration_id.clone(),
                    ),
                    result_id: update.result_id,
                    server_id: update.server_id,
                    disk_based_sources: update.disk_based_sources,
                    registration_id: update.registration_id,
                })
            })
            .collect();
        self.merge_diagnostic_entries(updates, merge, cx)?;
        Ok(())
    }

    fn lsp_to_document_diagnostics(
        &mut self,
        document_abs_path: PathBuf,
        source_kind: DiagnosticSourceKind,
        server_id: LanguageServerId,
        mut lsp_diagnostics: lsp::PublishDiagnosticsParams,
        disk_based_sources: &[String],
        registration_id: Option<SharedString>,
    ) -> DocumentDiagnostics {
        let mut diagnostics = Vec::default();
        let mut primary_diagnostic_group_ids = HashMap::default();
        let mut sources_by_group_id = HashMap::default();
        let mut supporting_diagnostics = HashMap::default();

        let adapter = self.language_server_adapter_for_id(server_id);

        // Ensure that primary diagnostics are always the most severe.
        lsp_diagnostics
            .diagnostics
            .sort_by_key(|item| item.severity);

        for diagnostic in &lsp_diagnostics.diagnostics {
            let source = diagnostic.source.as_ref();
            let range = range_from_lsp(diagnostic.range);
            let is_supporting = diagnostic
                .related_information
                .as_ref()
                .is_some_and(|infos| {
                    infos.iter().any(|info| {
                        primary_diagnostic_group_ids.contains_key(&(
                            source,
                            diagnostic.code.clone(),
                            range_from_lsp(info.location.range),
                        ))
                    })
                });

            let is_unnecessary = diagnostic
                .tags
                .as_ref()
                .is_some_and(|tags| tags.contains(&DiagnosticTag::UNNECESSARY));

            let underline = self
                .language_server_adapter_for_id(server_id)
                .is_none_or(|adapter| adapter.underline_diagnostic(diagnostic));

            if is_supporting {
                supporting_diagnostics.insert(
                    (source, diagnostic.code.clone(), range),
                    (diagnostic.severity, is_unnecessary),
                );
            } else {
                let group_id = post_inc(&mut self.as_local_mut().unwrap().next_diagnostic_group_id);
                let is_disk_based =
                    source.is_some_and(|source| disk_based_sources.contains(source));

                sources_by_group_id.insert(group_id, source);
                primary_diagnostic_group_ids
                    .insert((source, diagnostic.code.clone(), range.clone()), group_id);

                diagnostics.push(DiagnosticEntry {
                    range,
                    diagnostic: Diagnostic {
                        source: diagnostic.source.clone(),
                        source_kind,
                        code: diagnostic.code.clone(),
                        code_description: diagnostic
                            .code_description
                            .as_ref()
                            .and_then(|d| d.href.clone()),
                        severity: diagnostic.severity.unwrap_or(DiagnosticSeverity::ERROR),
                        markdown: adapter.as_ref().and_then(|adapter| {
                            adapter.diagnostic_message_to_markdown(&diagnostic.message)
                        }),
                        message: diagnostic.message.trim().to_string(),
                        group_id,
                        is_primary: true,
                        is_disk_based,
                        is_unnecessary,
                        underline,
                        data: diagnostic.data.clone(),
                        registration_id: registration_id.clone(),
                    },
                });
                if let Some(infos) = &diagnostic.related_information {
                    for info in infos {
                        if info.location.uri == lsp_diagnostics.uri && !info.message.is_empty() {
                            let range = range_from_lsp(info.location.range);
                            diagnostics.push(DiagnosticEntry {
                                range,
                                diagnostic: Diagnostic {
                                    source: diagnostic.source.clone(),
                                    source_kind,
                                    code: diagnostic.code.clone(),
                                    code_description: diagnostic
                                        .code_description
                                        .as_ref()
                                        .and_then(|d| d.href.clone()),
                                    severity: DiagnosticSeverity::INFORMATION,
                                    markdown: adapter.as_ref().and_then(|adapter| {
                                        adapter.diagnostic_message_to_markdown(&info.message)
                                    }),
                                    message: info.message.trim().to_string(),
                                    group_id,
                                    is_primary: false,
                                    is_disk_based,
                                    is_unnecessary: false,
                                    underline,
                                    data: diagnostic.data.clone(),
                                    registration_id: registration_id.clone(),
                                },
                            });
                        }
                    }
                }
            }
        }

        for entry in &mut diagnostics {
            let diagnostic = &mut entry.diagnostic;
            if !diagnostic.is_primary {
                let source = *sources_by_group_id.get(&diagnostic.group_id).unwrap();
                if let Some(&(severity, is_unnecessary)) = supporting_diagnostics.get(&(
                    source,
                    diagnostic.code.clone(),
                    entry.range.clone(),
                )) {
                    if let Some(severity) = severity {
                        diagnostic.severity = severity;
                    }
                    diagnostic.is_unnecessary = is_unnecessary;
                }
            }
        }

        DocumentDiagnostics {
            diagnostics,
            document_abs_path,
            version: lsp_diagnostics.version,
        }
    }

    pub fn language_servers_running_disk_based_diagnostics(
        &self,
    ) -> impl Iterator<Item = LanguageServerId> + '_ {
        self.language_server_statuses
            .iter()
            .filter_map(|(id, status)| {
                if status.has_pending_diagnostic_updates {
                    Some(*id)
                } else {
                    None
                }
            })
    }
}
