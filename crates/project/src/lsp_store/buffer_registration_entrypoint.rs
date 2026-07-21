use super::*;

impl LspStore {
    pub(crate) fn register_buffer_with_language_servers(
        &mut self,
        buffer: &Entity<Buffer>,
        only_register_servers: HashSet<LanguageServerSelector>,
        ignore_refcounts: bool,
        cx: &mut Context<Self>,
    ) -> OpenLspBufferHandle {
        let buffer_id = buffer.read(cx).remote_id();
        let handle = OpenLspBufferHandle(cx.new(|_| OpenLspBuffer(buffer.clone())));
        if let Some(local) = self.as_local_mut() {
            let refcount = local.registered_buffers.entry(buffer_id).or_insert(0);
            if !ignore_refcounts {
                *refcount += 1;
            }

            // We run early exits on non-existing buffers AFTER we mark the buffer as registered in order to handle buffer saving.
            // When a new unnamed buffer is created and saved, we will start loading it's language. Once the language is loaded, we go over all "language-less" buffers and try to fit that new language
            // with them. However, we do that only for the buffers that we think are open in at least one editor; thus, we need to keep tab of unnamed buffers as well, even though they're not actually registered with any language
            // servers in practice (we don't support non-file URI schemes in our LSP impl).
            let Some(file) = File::from_dyn(buffer.read(cx).file()) else {
                return handle;
            };
            if !file.is_local() {
                return handle;
            }

            if ignore_refcounts || *refcount == 1 {
                local.register_buffer_with_language_servers(buffer, only_register_servers, cx);
            }
            if !ignore_refcounts {
                cx.observe_release(&handle.0, move |lsp_store, buffer, cx| {
                    let refcount = {
                        let local = lsp_store.as_local_mut().unwrap();
                        let Some(refcount) = local.registered_buffers.get_mut(&buffer_id) else {
                            debug_panic!("bad refcounting");
                            return;
                        };

                        *refcount -= 1;
                        *refcount
                    };
                    if refcount == 0 {
                        lsp_store.lsp_data.remove(&buffer_id);
                        lsp_store.buffer_reload_tasks.remove(&buffer_id);
                        let local = lsp_store.as_local_mut().unwrap();
                        local.registered_buffers.remove(&buffer_id);

                        local.buffers_opened_in_servers.remove(&buffer_id);
                        if let Some(file) = File::from_dyn(buffer.0.read(cx).file()).cloned() {
                            local.unregister_old_buffer_from_language_servers(&buffer.0, &file, cx);

                            let buffer_abs_path = file.abs_path(cx);
                            for (_, buffer_pull_diagnostics_result_ids) in
                                &mut local.buffer_pull_diagnostics_result_ids
                            {
                                buffer_pull_diagnostics_result_ids.retain(
                                    |_, buffer_result_ids| {
                                        buffer_result_ids.remove(&buffer_abs_path);
                                        !buffer_result_ids.is_empty()
                                    },
                                );
                            }

                            let diagnostic_updates = local
                                .language_servers
                                .keys()
                                .cloned()
                                .map(|server_id| DocumentDiagnosticsUpdate {
                                    diagnostics: DocumentDiagnostics {
                                        document_abs_path: buffer_abs_path.clone(),
                                        version: None,
                                        diagnostics: Vec::new(),
                                    },
                                    result_id: None,
                                    registration_id: None,
                                    server_id,
                                    disk_based_sources: Cow::Borrowed(&[]),
                                })
                                .collect::<Vec<_>>();

                            lsp_store
                                .merge_diagnostic_entries(
                                    diagnostic_updates,
                                    |_, diagnostic, _| {
                                        diagnostic.source_kind != DiagnosticSourceKind::Pulled
                                    },
                                    cx,
                                )
                                .context("Clearing diagnostics for the closed buffer")
                                .log_err();
                        }
                    }
                })
                .detach();
            }
        } else if let Some((upstream_client, upstream_project_id)) = self.upstream_client() {
            let buffer_id = buffer.read(cx).remote_id().to_proto();
            cx.background_spawn(async move {
                upstream_client
                    .request(proto::RegisterBufferWithLanguageServers {
                        project_id: upstream_project_id,
                        buffer_id,
                        only_servers: only_register_servers
                            .into_iter()
                            .map(|selector| {
                                let selector = match selector {
                                    LanguageServerSelector::Id(language_server_id) => {
                                        proto::language_server_selector::Selector::ServerId(
                                            language_server_id.to_proto(),
                                        )
                                    }
                                    LanguageServerSelector::Name(language_server_name) => {
                                        proto::language_server_selector::Selector::Name(
                                            language_server_name.to_string(),
                                        )
                                    }
                                };
                                proto::LanguageServerSelector {
                                    selector: Some(selector),
                                }
                            })
                            .collect(),
                    })
                    .await
            })
            .detach();
        } else {
            // Our remote connection got closed
        }
        handle
    }
}
