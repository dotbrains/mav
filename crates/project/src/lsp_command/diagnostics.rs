use super::*;

impl GetDocumentDiagnostics {
    pub fn diagnostics_from_proto(
        response: proto::GetDocumentDiagnosticsResponse,
    ) -> Vec<LspPullDiagnostics> {
        response
            .pulled_diagnostics
            .into_iter()
            .filter_map(|diagnostics| {
                Some(LspPullDiagnostics::Response {
                    registration_id: diagnostics.registration_id.map(SharedString::from),
                    server_id: LanguageServerId::from_proto(diagnostics.server_id),
                    uri: lsp::Uri::from_str(diagnostics.uri.as_str()).log_err()?,
                    diagnostics: if diagnostics.changed {
                        PulledDiagnostics::Unchanged {
                            result_id: SharedString::new(diagnostics.result_id?),
                        }
                    } else {
                        PulledDiagnostics::Changed {
                            result_id: diagnostics.result_id.map(SharedString::new),
                            diagnostics: diagnostics
                                .diagnostics
                                .into_iter()
                                .filter_map(|diagnostic| {
                                    GetDocumentDiagnostics::deserialize_lsp_diagnostic(diagnostic)
                                        .context("deserializing diagnostics")
                                        .log_err()
                                })
                                .collect(),
                        }
                    },
                })
            })
            .collect()
    }

    pub fn deserialize_lsp_diagnostic(diagnostic: proto::LspDiagnostic) -> Result<lsp::Diagnostic> {
        let start = diagnostic.start.context("invalid start range")?;
        let end = diagnostic.end.context("invalid end range")?;

        let range = Range::<PointUtf16> {
            start: PointUtf16 {
                row: start.row,
                column: start.column,
            },
            end: PointUtf16 {
                row: end.row,
                column: end.column,
            },
        };

        let data = diagnostic.data.and_then(|data| Value::from_str(&data).ok());
        let code = diagnostic.code.map(lsp::NumberOrString::String);

        let related_information = diagnostic
            .related_information
            .into_iter()
            .map(|info| {
                let start = info.location_range_start.unwrap();
                let end = info.location_range_end.unwrap();

                lsp::DiagnosticRelatedInformation {
                    location: lsp::Location {
                        range: lsp::Range {
                            start: point_to_lsp(PointUtf16::new(start.row, start.column)),
                            end: point_to_lsp(PointUtf16::new(end.row, end.column)),
                        },
                        uri: lsp::Uri::from_str(&info.location_url.unwrap()).unwrap(),
                    },
                    message: info.message,
                }
            })
            .collect::<Vec<_>>();

        let tags = diagnostic
            .tags
            .into_iter()
            .filter_map(|tag| match proto::LspDiagnosticTag::from_i32(tag) {
                Some(proto::LspDiagnosticTag::Unnecessary) => Some(lsp::DiagnosticTag::UNNECESSARY),
                Some(proto::LspDiagnosticTag::Deprecated) => Some(lsp::DiagnosticTag::DEPRECATED),
                _ => None,
            })
            .collect::<Vec<_>>();

        Ok(lsp::Diagnostic {
            range: language::range_to_lsp(range)?,
            severity: match proto::lsp_diagnostic::Severity::from_i32(diagnostic.severity).unwrap()
            {
                proto::lsp_diagnostic::Severity::Error => Some(lsp::DiagnosticSeverity::ERROR),
                proto::lsp_diagnostic::Severity::Warning => Some(lsp::DiagnosticSeverity::WARNING),
                proto::lsp_diagnostic::Severity::Information => {
                    Some(lsp::DiagnosticSeverity::INFORMATION)
                }
                proto::lsp_diagnostic::Severity::Hint => Some(lsp::DiagnosticSeverity::HINT),
                _ => None,
            },
            code,
            code_description: diagnostic
                .code_description
                .map(|code_description| CodeDescription {
                    href: Some(lsp::Uri::from_str(&code_description).unwrap()),
                }),
            related_information: Some(related_information),
            tags: Some(tags),
            source: diagnostic.source.clone(),
            message: diagnostic.message,
            data,
        })
    }

    pub fn serialize_lsp_diagnostic(diagnostic: lsp::Diagnostic) -> Result<proto::LspDiagnostic> {
        let range = language::range_from_lsp(diagnostic.range);
        let related_information = diagnostic
            .related_information
            .unwrap_or_default()
            .into_iter()
            .map(|related_information| {
                let location_range_start =
                    point_from_lsp(related_information.location.range.start).0;
                let location_range_end = point_from_lsp(related_information.location.range.end).0;

                Ok(proto::LspDiagnosticRelatedInformation {
                    location_url: Some(related_information.location.uri.to_string()),
                    location_range_start: Some(proto::PointUtf16 {
                        row: location_range_start.row,
                        column: location_range_start.column,
                    }),
                    location_range_end: Some(proto::PointUtf16 {
                        row: location_range_end.row,
                        column: location_range_end.column,
                    }),
                    message: related_information.message,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let tags = diagnostic
            .tags
            .unwrap_or_default()
            .into_iter()
            .map(|tag| match tag {
                lsp::DiagnosticTag::UNNECESSARY => proto::LspDiagnosticTag::Unnecessary,
                lsp::DiagnosticTag::DEPRECATED => proto::LspDiagnosticTag::Deprecated,
                _ => proto::LspDiagnosticTag::None,
            } as i32)
            .collect();

        Ok(proto::LspDiagnostic {
            start: Some(proto::PointUtf16 {
                row: range.start.0.row,
                column: range.start.0.column,
            }),
            end: Some(proto::PointUtf16 {
                row: range.end.0.row,
                column: range.end.0.column,
            }),
            severity: match diagnostic.severity {
                Some(lsp::DiagnosticSeverity::ERROR) => proto::lsp_diagnostic::Severity::Error,
                Some(lsp::DiagnosticSeverity::WARNING) => proto::lsp_diagnostic::Severity::Warning,
                Some(lsp::DiagnosticSeverity::INFORMATION) => {
                    proto::lsp_diagnostic::Severity::Information
                }
                Some(lsp::DiagnosticSeverity::HINT) => proto::lsp_diagnostic::Severity::Hint,
                _ => proto::lsp_diagnostic::Severity::None,
            } as i32,
            code: diagnostic.code.as_ref().map(|code| match code {
                lsp::NumberOrString::Number(code) => code.to_string(),
                lsp::NumberOrString::String(code) => code.clone(),
            }),
            source: diagnostic.source.clone(),
            related_information,
            tags,
            code_description: diagnostic
                .code_description
                .and_then(|desc| desc.href.map(|url| url.to_string())),
            message: diagnostic.message,
            data: diagnostic.data.as_ref().map(|data| data.to_string()),
        })
    }

    pub fn deserialize_workspace_diagnostics_report(
        report: lsp::WorkspaceDiagnosticReportResult,
        server_id: LanguageServerId,
        registration_id: Option<SharedString>,
    ) -> Vec<WorkspaceLspPullDiagnostics> {
        let mut pulled_diagnostics = HashMap::default();
        match report {
            lsp::WorkspaceDiagnosticReportResult::Report(workspace_diagnostic_report) => {
                for report in workspace_diagnostic_report.items {
                    match report {
                        lsp::WorkspaceDocumentDiagnosticReport::Full(report) => {
                            process_full_workspace_diagnostics_report(
                                &mut pulled_diagnostics,
                                server_id,
                                report,
                                registration_id.clone(),
                            )
                        }
                        lsp::WorkspaceDocumentDiagnosticReport::Unchanged(report) => {
                            process_unchanged_workspace_diagnostics_report(
                                &mut pulled_diagnostics,
                                server_id,
                                report,
                                registration_id.clone(),
                            )
                        }
                    }
                }
            }
            lsp::WorkspaceDiagnosticReportResult::Partial(
                workspace_diagnostic_report_partial_result,
            ) => {
                for report in workspace_diagnostic_report_partial_result.items {
                    match report {
                        lsp::WorkspaceDocumentDiagnosticReport::Full(report) => {
                            process_full_workspace_diagnostics_report(
                                &mut pulled_diagnostics,
                                server_id,
                                report,
                                registration_id.clone(),
                            )
                        }
                        lsp::WorkspaceDocumentDiagnosticReport::Unchanged(report) => {
                            process_unchanged_workspace_diagnostics_report(
                                &mut pulled_diagnostics,
                                server_id,
                                report,
                                registration_id.clone(),
                            )
                        }
                    }
                }
            }
        }
        pulled_diagnostics.into_values().collect()
    }
}

#[derive(Debug)]
pub struct WorkspaceLspPullDiagnostics {
    pub version: Option<i32>,
    pub diagnostics: LspPullDiagnostics,
}

pub(super) fn process_full_workspace_diagnostics_report(
    diagnostics: &mut HashMap<lsp::Uri, WorkspaceLspPullDiagnostics>,
    server_id: LanguageServerId,
    report: lsp::WorkspaceFullDocumentDiagnosticReport,
    registration_id: Option<SharedString>,
) {
    let mut new_diagnostics = HashMap::default();
    process_full_diagnostics_report(
        &mut new_diagnostics,
        server_id,
        report.uri,
        report.full_document_diagnostic_report,
        registration_id,
    );
    diagnostics.extend(new_diagnostics.into_iter().map(|(uri, diagnostics)| {
        (
            uri,
            WorkspaceLspPullDiagnostics {
                version: report.version.map(|v| v as i32),
                diagnostics,
            },
        )
    }));
}

pub(super) fn process_unchanged_workspace_diagnostics_report(
    diagnostics: &mut HashMap<lsp::Uri, WorkspaceLspPullDiagnostics>,
    server_id: LanguageServerId,
    report: lsp::WorkspaceUnchangedDocumentDiagnosticReport,
    registration_id: Option<SharedString>,
) {
    let mut new_diagnostics = HashMap::default();
    process_unchanged_diagnostics_report(
        &mut new_diagnostics,
        server_id,
        report.uri,
        report.unchanged_document_diagnostic_report,
        registration_id,
    );
    diagnostics.extend(new_diagnostics.into_iter().map(|(uri, diagnostics)| {
        (
            uri,
            WorkspaceLspPullDiagnostics {
                version: report.version.map(|v| v as i32),
                diagnostics,
            },
        )
    }));
}

pub(super) fn process_related_documents(
    diagnostics: &mut HashMap<lsp::Uri, LspPullDiagnostics>,
    server_id: LanguageServerId,
    documents: impl IntoIterator<Item = (lsp::Uri, lsp::DocumentDiagnosticReportKind)>,
    registration_id: Option<SharedString>,
) {
    for (url, report_kind) in documents {
        match report_kind {
            lsp::DocumentDiagnosticReportKind::Full(report) => process_full_diagnostics_report(
                diagnostics,
                server_id,
                url,
                report,
                registration_id.clone(),
            ),
            lsp::DocumentDiagnosticReportKind::Unchanged(report) => {
                process_unchanged_diagnostics_report(
                    diagnostics,
                    server_id,
                    url,
                    report,
                    registration_id.clone(),
                )
            }
        }
    }
}

pub(super) fn process_unchanged_diagnostics_report(
    diagnostics: &mut HashMap<lsp::Uri, LspPullDiagnostics>,
    server_id: LanguageServerId,
    uri: lsp::Uri,
    report: lsp::UnchangedDocumentDiagnosticReport,
    registration_id: Option<SharedString>,
) {
    let result_id = SharedString::new(report.result_id);
    match diagnostics.entry(uri.clone()) {
        hash_map::Entry::Occupied(mut o) => match o.get_mut() {
            LspPullDiagnostics::Default => {
                o.insert(LspPullDiagnostics::Response {
                    server_id,
                    uri,
                    diagnostics: PulledDiagnostics::Unchanged { result_id },
                    registration_id,
                });
            }
            LspPullDiagnostics::Response {
                server_id: existing_server_id,
                uri: existing_uri,
                diagnostics: existing_diagnostics,
                ..
            } => {
                if server_id != *existing_server_id || &uri != existing_uri {
                    debug_panic!(
                        "Unexpected state: file {uri} has two different sets of diagnostics reported"
                    );
                }
                match existing_diagnostics {
                    PulledDiagnostics::Unchanged { .. } => {
                        *existing_diagnostics = PulledDiagnostics::Unchanged { result_id };
                    }
                    PulledDiagnostics::Changed { .. } => {}
                }
            }
        },
        hash_map::Entry::Vacant(v) => {
            v.insert(LspPullDiagnostics::Response {
                server_id,
                uri,
                diagnostics: PulledDiagnostics::Unchanged { result_id },
                registration_id,
            });
        }
    }
}

pub(super) fn process_full_diagnostics_report(
    diagnostics: &mut HashMap<lsp::Uri, LspPullDiagnostics>,
    server_id: LanguageServerId,
    uri: lsp::Uri,
    report: lsp::FullDocumentDiagnosticReport,
    registration_id: Option<SharedString>,
) {
    let result_id = report.result_id.map(SharedString::new);
    match diagnostics.entry(uri.clone()) {
        hash_map::Entry::Occupied(mut o) => match o.get_mut() {
            LspPullDiagnostics::Default => {
                o.insert(LspPullDiagnostics::Response {
                    server_id,
                    uri,
                    diagnostics: PulledDiagnostics::Changed {
                        result_id,
                        diagnostics: report.items,
                    },
                    registration_id,
                });
            }
            LspPullDiagnostics::Response {
                server_id: existing_server_id,
                uri: existing_uri,
                diagnostics: existing_diagnostics,
                ..
            } => {
                if server_id != *existing_server_id || &uri != existing_uri {
                    debug_panic!(
                        "Unexpected state: file {uri} has two different sets of diagnostics reported"
                    );
                }
                match existing_diagnostics {
                    PulledDiagnostics::Unchanged { .. } => {
                        *existing_diagnostics = PulledDiagnostics::Changed {
                            result_id,
                            diagnostics: report.items,
                        };
                    }
                    PulledDiagnostics::Changed {
                        result_id: existing_result_id,
                        diagnostics: existing_diagnostics,
                    } => {
                        if result_id.is_some() {
                            *existing_result_id = result_id;
                        }
                        existing_diagnostics.extend(report.items);
                    }
                }
            }
        },
        hash_map::Entry::Vacant(v) => {
            v.insert(LspPullDiagnostics::Response {
                server_id,
                uri,
                diagnostics: PulledDiagnostics::Changed {
                    result_id,
                    diagnostics: report.items,
                },
                registration_id,
            });
        }
    }
}
