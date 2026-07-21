use super::*;

impl LspStore {
    pub fn on_buffer_edited(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let language_servers: Vec<_> = buffer.update(cx, |buffer, cx| {
            Some(
                self.as_local()?
                    .language_servers_for_buffer(buffer, cx)
                    .map(|i| i.1.clone())
                    .collect(),
            )
        })?;

        let buffer = buffer.read(cx);
        let file = File::from_dyn(buffer.file())?;
        let abs_path = file.as_local()?.abs_path(cx);
        let uri = lsp::Uri::from_file_path(&abs_path)
            .ok()
            .with_context(|| format!("Failed to convert path to URI: {}", abs_path.display()))
            .log_err()?;
        let next_snapshot = buffer.text_snapshot();
        for language_server in language_servers {
            let language_server = language_server.clone();

            let buffer_snapshots = self
                .as_local_mut()?
                .buffer_snapshots
                .get_mut(&buffer.remote_id())
                .and_then(|m| m.get_mut(&language_server.server_id()))?;
            let previous_snapshot = buffer_snapshots.last()?;

            let build_incremental_change = || {
                buffer
                    .edits_since::<Dimensions<PointUtf16, usize>>(
                        previous_snapshot.snapshot.version(),
                    )
                    .map(|edit| {
                        let edit_start = edit.new.start.0;
                        let edit_end = edit_start + (edit.old.end.0 - edit.old.start.0);
                        let new_text = next_snapshot
                            .text_for_range(edit.new.start.1..edit.new.end.1)
                            .collect();
                        lsp::TextDocumentContentChangeEvent {
                            range: Some(lsp::Range::new(
                                point_to_lsp(edit_start),
                                point_to_lsp(edit_end),
                            )),
                            range_length: None,
                            text: new_text,
                        }
                    })
                    .collect()
            };

            let document_sync_kind = language_server
                .capabilities()
                .text_document_sync
                .as_ref()
                .and_then(|sync| match sync {
                    lsp::TextDocumentSyncCapability::Kind(kind) => Some(*kind),
                    lsp::TextDocumentSyncCapability::Options(options) => options.change,
                });

            let content_changes: Vec<_> = match document_sync_kind {
                Some(lsp::TextDocumentSyncKind::FULL) => {
                    vec![lsp::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: next_snapshot.text(),
                    }]
                }
                Some(lsp::TextDocumentSyncKind::INCREMENTAL) => build_incremental_change(),
                _ => {
                    #[cfg(any(test, feature = "test-support"))]
                    {
                        build_incremental_change()
                    }

                    #[cfg(not(any(test, feature = "test-support")))]
                    {
                        continue;
                    }
                }
            };

            let next_version = previous_snapshot.version + 1;
            buffer_snapshots.push(LspBufferSnapshot {
                version: next_version,
                snapshot: next_snapshot.clone(),
            });

            language_server
                .notify::<lsp::notification::DidChangeTextDocument>(
                    lsp::DidChangeTextDocumentParams {
                        text_document: lsp::VersionedTextDocumentIdentifier::new(
                            uri.clone(),
                            next_version,
                        ),
                        content_changes,
                    },
                )
                .ok();
            self.pull_workspace_diagnostics(language_server.server_id());
        }

        None
    }

    pub fn on_buffer_saved(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let file = File::from_dyn(buffer.read(cx).file())?;
        let worktree_id = file.worktree_id(cx);
        let abs_path = file.as_local()?.abs_path(cx);
        let text_document = lsp::TextDocumentIdentifier {
            uri: file_path_to_lsp_url(&abs_path).log_err()?,
        };
        let local = self.as_local()?;

        for server in local.language_servers_for_worktree(worktree_id) {
            if let Some(include_text) = include_text(server.as_ref()) {
                let text = if include_text {
                    Some(buffer.read(cx).text())
                } else {
                    None
                };
                server
                    .notify::<lsp::notification::DidSaveTextDocument>(
                        lsp::DidSaveTextDocumentParams {
                            text_document: text_document.clone(),
                            text,
                        },
                    )
                    .ok();
            }
        }

        let language_servers = buffer.update(cx, |buffer, cx| {
            local.language_server_ids_for_buffer(buffer, cx)
        });
        for language_server_id in language_servers {
            self.simulate_disk_based_diagnostics_events_if_needed(language_server_id, cx);
        }

        None
    }
}
