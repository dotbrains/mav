use super::*;

impl LspStore {
    pub fn diagnostic_summary(&self, include_ignored: bool, cx: &App) -> DiagnosticSummary {
        let mut summary = DiagnosticSummary::default();
        for (_, _, path_summary) in self.diagnostic_summaries(include_ignored, cx) {
            summary.error_count += path_summary.error_count;
            summary.warning_count += path_summary.warning_count;
        }
        summary
    }

    /// Returns the diagnostic summary for a specific project path.
    pub fn diagnostic_summary_for_path(
        &self,
        project_path: &ProjectPath,
        _: &App,
    ) -> DiagnosticSummary {
        if let Some(summaries) = self
            .diagnostic_summaries
            .get(&project_path.worktree_id)
            .and_then(|map| map.get(&project_path.path))
        {
            let (error_count, warning_count) = summaries.iter().fold(
                (0, 0),
                |(error_count, warning_count), (_language_server_id, summary)| {
                    (
                        error_count + summary.error_count,
                        warning_count + summary.warning_count,
                    )
                },
            );

            DiagnosticSummary {
                error_count,
                warning_count,
            }
        } else {
            DiagnosticSummary::default()
        }
    }

    pub fn diagnostic_summaries<'a>(
        &'a self,
        include_ignored: bool,
        cx: &'a App,
    ) -> impl Iterator<Item = (ProjectPath, LanguageServerId, DiagnosticSummary)> + 'a {
        self.worktree_store
            .read(cx)
            .visible_worktrees(cx)
            .filter_map(|worktree| {
                let worktree = worktree.read(cx);
                Some((worktree, self.diagnostic_summaries.get(&worktree.id())?))
            })
            .flat_map(move |(worktree, summaries)| {
                let worktree_id = worktree.id();
                summaries
                    .iter()
                    .filter(move |(path, _)| {
                        include_ignored
                            || worktree
                                .entry_for_path(path.as_ref())
                                .is_some_and(|entry| !entry.is_ignored)
                    })
                    .flat_map(move |(path, summaries)| {
                        summaries.iter().map(move |(server_id, summary)| {
                            (
                                ProjectPath {
                                    worktree_id,
                                    path: path.clone(),
                                },
                                *server_id,
                                *summary,
                            )
                        })
                    })
            })
    }

    #[cfg(feature = "test-support")]
    pub fn update_diagnostic_entries(
        &mut self,
        server_id: LanguageServerId,
        abs_path: PathBuf,
        result_id: Option<SharedString>,
        version: Option<i32>,
        diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        self.merge_diagnostic_entries(
            vec![DocumentDiagnosticsUpdate {
                diagnostics: DocumentDiagnostics {
                    diagnostics,
                    document_abs_path: abs_path,
                    version,
                },
                result_id,
                server_id,
                disk_based_sources: Cow::Borrowed(&[]),
                registration_id: None,
            }],
            |_, _, _| false,
            cx,
        )?;
        Ok(())
    }

    pub fn merge_diagnostic_entries<'a>(
        &mut self,
        diagnostic_updates: Vec<DocumentDiagnosticsUpdate<'a, DocumentDiagnostics>>,
        merge: impl Fn(&lsp::Uri, &Diagnostic, &App) -> bool + Clone,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        let mut diagnostics_summary = None::<proto::UpdateDiagnosticSummary>;
        let mut updated_diagnostics_paths = HashMap::default();
        for mut update in diagnostic_updates {
            let abs_path = &update.diagnostics.document_abs_path;
            let server_id = update.server_id;
            let Some((worktree, relative_path)) =
                self.worktree_store.read(cx).find_worktree(abs_path, cx)
            else {
                log::warn!("skipping diagnostics update, no worktree found for path {abs_path:?}");
                return Ok(());
            };

            let worktree_id = worktree.read(cx).id();
            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let document_uri = lsp::Uri::from_file_path(abs_path)
                .map_err(|()| anyhow!("Failed to convert buffer path {abs_path:?} to lsp Uri"))?;
            if let Some(buffer_handle) = self.buffer_store.read(cx).get_by_path(&project_path) {
                let snapshot = buffer_handle.read(cx).snapshot();
                let buffer = buffer_handle.read(cx);
                let reused_diagnostics = buffer
                    .buffer_diagnostics(Some(server_id))
                    .iter()
                    .filter(|v| merge(&document_uri, &v.diagnostic, cx))
                    .map(|v| {
                        let start = Unclipped(v.range.start.to_point_utf16(&snapshot));
                        let end = Unclipped(v.range.end.to_point_utf16(&snapshot));
                        DiagnosticEntry {
                            range: start..end,
                            diagnostic: v.diagnostic.clone(),
                        }
                    })
                    .collect::<Vec<_>>();

                self.as_local_mut()
                    .context("cannot merge diagnostics on a remote LspStore")?
                    .update_buffer_diagnostics(
                        &buffer_handle,
                        server_id,
                        Some(update.registration_id),
                        update.result_id,
                        update.diagnostics.version,
                        update.diagnostics.diagnostics.clone(),
                        reused_diagnostics.clone(),
                        cx,
                    )?;

                update.diagnostics.diagnostics.extend(reused_diagnostics);
            } else if let Some(local) = self.as_local() {
                let reused_diagnostics = local
                    .diagnostics
                    .get(&worktree_id)
                    .and_then(|diagnostics_for_tree| diagnostics_for_tree.get(&project_path.path))
                    .and_then(|diagnostics_by_server_id| {
                        diagnostics_by_server_id
                            .binary_search_by_key(&server_id, |e| e.0)
                            .ok()
                            .map(|ix| &diagnostics_by_server_id[ix].1)
                    })
                    .into_iter()
                    .flatten()
                    .filter(|v| merge(&document_uri, &v.diagnostic, cx));

                update
                    .diagnostics
                    .diagnostics
                    .extend(reused_diagnostics.cloned());
            }

            let updated = worktree.update(cx, |worktree, cx| {
                self.update_worktree_diagnostics(
                    worktree.id(),
                    server_id,
                    project_path.path.clone(),
                    update.diagnostics.diagnostics,
                    cx,
                )
            })?;
            match updated {
                ControlFlow::Continue(new_summary) => {
                    if let Some((project_id, new_summary)) = new_summary {
                        match &mut diagnostics_summary {
                            Some(diagnostics_summary) => {
                                diagnostics_summary
                                    .more_summaries
                                    .push(proto::DiagnosticSummary {
                                        path: project_path.path.as_ref().to_proto(),
                                        language_server_id: server_id.0 as u64,
                                        error_count: new_summary.error_count,
                                        warning_count: new_summary.warning_count,
                                    })
                            }
                            None => {
                                diagnostics_summary = Some(proto::UpdateDiagnosticSummary {
                                    project_id,
                                    worktree_id: worktree_id.to_proto(),
                                    summary: Some(proto::DiagnosticSummary {
                                        path: project_path.path.as_ref().to_proto(),
                                        language_server_id: server_id.0 as u64,
                                        error_count: new_summary.error_count,
                                        warning_count: new_summary.warning_count,
                                    }),
                                    more_summaries: Vec::new(),
                                })
                            }
                        }
                    }
                    updated_diagnostics_paths
                        .entry(server_id)
                        .or_insert_with(Vec::new)
                        .push(project_path);
                }
                ControlFlow::Break(()) => {}
            }
        }

        if let Some((diagnostics_summary, (downstream_client, _))) =
            diagnostics_summary.zip(self.downstream_client.as_ref())
        {
            downstream_client.send(diagnostics_summary).log_err();
        }
        for (server_id, paths) in updated_diagnostics_paths {
            cx.emit(LspStoreEvent::DiagnosticsUpdated { server_id, paths });
        }
        Ok(())
    }

    pub(super) fn update_worktree_diagnostics(
        &mut self,
        worktree_id: WorktreeId,
        server_id: LanguageServerId,
        path_in_worktree: Arc<RelPath>,
        diagnostics: Vec<DiagnosticEntry<Unclipped<PointUtf16>>>,
        _: &mut Context<Worktree>,
    ) -> Result<ControlFlow<(), Option<(u64, proto::DiagnosticSummary)>>> {
        let local = match &mut self.mode {
            LspStoreMode::Local(local_lsp_store) => local_lsp_store,
            _ => anyhow::bail!("update_worktree_diagnostics called on remote"),
        };

        let summaries_for_tree = self.diagnostic_summaries.entry(worktree_id).or_default();
        let diagnostics_for_tree = local.diagnostics.entry(worktree_id).or_default();
        let summaries_by_server_id = summaries_for_tree
            .entry(path_in_worktree.clone())
            .or_default();

        let old_summary = summaries_by_server_id
            .remove(&server_id)
            .unwrap_or_default();

        let new_summary = DiagnosticSummary::new(&diagnostics);
        if diagnostics.is_empty() {
            if let Some(diagnostics_by_server_id) = diagnostics_for_tree.get_mut(&path_in_worktree)
            {
                if let Ok(ix) = diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                    diagnostics_by_server_id.remove(ix);
                }
                if diagnostics_by_server_id.is_empty() {
                    diagnostics_for_tree.remove(&path_in_worktree);
                }
            }
        } else {
            summaries_by_server_id.insert(server_id, new_summary);
            let diagnostics_by_server_id = diagnostics_for_tree
                .entry(path_in_worktree.clone())
                .or_default();
            match diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                Ok(ix) => {
                    diagnostics_by_server_id[ix] = (server_id, diagnostics);
                }
                Err(ix) => {
                    diagnostics_by_server_id.insert(ix, (server_id, diagnostics));
                }
            }
        }

        if !old_summary.is_empty() || !new_summary.is_empty() {
            if let Some((_, project_id)) = &self.downstream_client {
                Ok(ControlFlow::Continue(Some((
                    *project_id,
                    proto::DiagnosticSummary {
                        path: path_in_worktree.to_proto(),
                        language_server_id: server_id.0 as u64,
                        error_count: new_summary.error_count as u32,
                        warning_count: new_summary.warning_count as u32,
                    },
                ))))
            } else {
                Ok(ControlFlow::Continue(None))
            }
        } else {
            Ok(ControlFlow::Break(()))
        }
    }
    pub(super) async fn handle_update_diagnostic_summary(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateDiagnosticSummary>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |lsp_store, cx| {
            let worktree_id = WorktreeId::from_proto(envelope.payload.worktree_id);
            let mut updated_diagnostics_paths = HashMap::default();
            let mut diagnostics_summary = None::<proto::UpdateDiagnosticSummary>;
            for message_summary in envelope
                .payload
                .summary
                .into_iter()
                .chain(envelope.payload.more_summaries)
            {
                let project_path = ProjectPath {
                    worktree_id,
                    path: RelPath::from_proto(&message_summary.path).context("invalid path")?,
                };
                let path = project_path.path.clone();
                let server_id = LanguageServerId(message_summary.language_server_id as usize);
                let summary = DiagnosticSummary {
                    error_count: message_summary.error_count as usize,
                    warning_count: message_summary.warning_count as usize,
                };

                if summary.is_empty() {
                    if let Some(worktree_summaries) =
                        lsp_store.diagnostic_summaries.get_mut(&worktree_id)
                        && let Some(summaries) = worktree_summaries.get_mut(&path)
                    {
                        summaries.remove(&server_id);
                        if summaries.is_empty() {
                            worktree_summaries.remove(&path);
                        }
                    }
                } else {
                    lsp_store
                        .diagnostic_summaries
                        .entry(worktree_id)
                        .or_default()
                        .entry(path)
                        .or_default()
                        .insert(server_id, summary);
                }

                if let Some((_, project_id)) = &lsp_store.downstream_client {
                    match &mut diagnostics_summary {
                        Some(diagnostics_summary) => {
                            diagnostics_summary
                                .more_summaries
                                .push(proto::DiagnosticSummary {
                                    path: project_path.path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: summary.error_count as u32,
                                    warning_count: summary.warning_count as u32,
                                })
                        }
                        None => {
                            diagnostics_summary = Some(proto::UpdateDiagnosticSummary {
                                project_id: *project_id,
                                worktree_id: worktree_id.to_proto(),
                                summary: Some(proto::DiagnosticSummary {
                                    path: project_path.path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: summary.error_count as u32,
                                    warning_count: summary.warning_count as u32,
                                }),
                                more_summaries: Vec::new(),
                            })
                        }
                    }
                }
                updated_diagnostics_paths
                    .entry(server_id)
                    .or_insert_with(Vec::new)
                    .push(project_path);
            }

            if let Some((diagnostics_summary, (downstream_client, _))) =
                diagnostics_summary.zip(lsp_store.downstream_client.as_ref())
            {
                downstream_client.send(diagnostics_summary).log_err();
            }
            for (server_id, paths) in updated_diagnostics_paths {
                cx.emit(LspStoreEvent::DiagnosticsUpdated { server_id, paths });
            }
            Ok(())
        })
    }
}
