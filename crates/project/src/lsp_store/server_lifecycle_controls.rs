use super::*;

impl LspStore {
    pub(super) async fn shutdown_language_server(
        server_state: Option<LanguageServerState>,
        name: LanguageServerName,
        cx: &mut AsyncApp,
    ) {
        let server = match server_state {
            Some(LanguageServerState::Starting { startup, .. }) => {
                let mut timer = cx
                    .background_executor()
                    .timer(SERVER_LAUNCHING_BEFORE_SHUTDOWN_TIMEOUT)
                    .fuse();

                select! {
                    server = startup.fuse() => server,
                    () = timer => {
                        log::info!("timeout waiting for language server {name} to finish launching before stopping");
                        None
                    },
                }
            }

            Some(LanguageServerState::Running { server, .. }) => Some(server),

            None => None,
        };

        let Some(server) = server else { return };
        if let Some(shutdown) = server.shutdown() {
            shutdown.await;
        }
    }

    // Returns a list of all of the worktrees which no longer have a language server and the root path
    // for the stopped server
    pub(super) fn stop_local_language_server(
        &mut self,
        server_id: LanguageServerId,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let local = match &mut self.mode {
            LspStoreMode::Local(local) => local,
            _ => {
                return Task::ready(());
            }
        };

        // Remove this server ID from all entries in the given worktree.
        local
            .language_server_ids
            .retain(|_, state| state.id != server_id);
        self.buffer_store.update(cx, |buffer_store, cx| {
            for buffer in buffer_store.buffers() {
                buffer.update(cx, |buffer, cx| {
                    buffer.update_diagnostics(server_id, DiagnosticSet::new([], buffer), cx);
                    buffer.set_completion_triggers(server_id, Default::default(), cx);
                });
            }
        });

        let mut cleared_paths: Vec<ProjectPath> = Vec::new();
        for (worktree_id, summaries) in self.diagnostic_summaries.iter_mut() {
            summaries.retain(|path, summaries_by_server_id| {
                if summaries_by_server_id.remove(&server_id).is_some() {
                    if let Some((client, project_id)) = self.downstream_client.clone() {
                        client
                            .send(proto::UpdateDiagnosticSummary {
                                project_id,
                                worktree_id: worktree_id.to_proto(),
                                summary: Some(proto::DiagnosticSummary {
                                    path: path.as_ref().to_proto(),
                                    language_server_id: server_id.0 as u64,
                                    error_count: 0,
                                    warning_count: 0,
                                }),
                                more_summaries: Vec::new(),
                            })
                            .log_err();
                    }
                    cleared_paths.push(ProjectPath {
                        worktree_id: *worktree_id,
                        path: path.clone(),
                    });
                    !summaries_by_server_id.is_empty()
                } else {
                    true
                }
            });
        }
        if !cleared_paths.is_empty() {
            cx.emit(LspStoreEvent::DiagnosticsUpdated {
                server_id,
                paths: cleared_paths,
            });
        }

        let local = self.as_local_mut().unwrap();
        for diagnostics in local.diagnostics.values_mut() {
            diagnostics.retain(|_, diagnostics_by_server_id| {
                if let Ok(ix) = diagnostics_by_server_id.binary_search_by_key(&server_id, |e| e.0) {
                    diagnostics_by_server_id.remove(ix);
                    !diagnostics_by_server_id.is_empty()
                } else {
                    true
                }
            });
        }
        local.language_server_watched_paths.remove(&server_id);

        let server_state = local.language_servers.remove(&server_id);
        self.cleanup_lsp_data(server_id);
        let name = self
            .language_server_statuses
            .remove(&server_id)
            .map(|status| status.name)
            .or_else(|| {
                if let Some(LanguageServerState::Running { adapter, .. }) = server_state.as_ref() {
                    Some(adapter.name())
                } else {
                    None
                }
            });

        if let Some(name) = name {
            log::info!("stopping language server {name}");
            self.languages
                .update_lsp_binary_status(name.clone(), BinaryStatus::Stopping);
            cx.notify();

            return cx.spawn(async move |lsp_store, cx| {
                Self::shutdown_language_server(server_state, name.clone(), cx).await;
                lsp_store
                    .update(cx, |lsp_store, cx| {
                        lsp_store
                            .languages
                            .update_lsp_binary_status(name, BinaryStatus::Stopped);
                        cx.emit(LspStoreEvent::LanguageServerRemoved(server_id));
                        cx.notify();
                    })
                    .ok();
            });
        }

        if server_state.is_some() {
            cx.emit(LspStoreEvent::LanguageServerRemoved(server_id));
        }
        Task::ready(())
    }

    pub fn stop_all_language_servers(&mut self, cx: &mut Context<Self>) {
        if let Some(local) = self.as_local_mut() {
            local.all_language_servers_stopped = true;
        }
        self.shutdown_all_language_servers(cx).detach();
    }

    pub fn shutdown_all_language_servers(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::StopLanguageServers {
                project_id,
                buffer_ids: Vec::new(),
                also_servers: Vec::new(),
                all: true,
            });
            cx.background_spawn(async move {
                request.await.ok();
            })
        } else {
            let Some(local) = self.as_local_mut() else {
                return Task::ready(());
            };
            let language_servers_to_stop = local
                .language_server_ids
                .values()
                .map(|state| state.id)
                .collect();
            local.lsp_tree.remove_nodes(&language_servers_to_stop);
            let tasks = language_servers_to_stop
                .into_iter()
                .map(|server| self.stop_local_language_server(server, cx))
                .collect::<Vec<_>>();
            cx.background_spawn(async move {
                futures::future::join_all(tasks).await;
            })
        }
    }

    pub fn restart_all_language_servers(&mut self, cx: &mut Context<Self>) {
        if let Some(local) = self.as_local_mut() {
            local.all_language_servers_stopped = false;
        }
        // `restart_language_servers_for_buffers` with empty selectors and `clear_stopped`
        // clears `stopped_language_servers` for us.
        let buffers = self.buffer_store.read(cx).buffers().collect();
        self.restart_language_servers_for_buffers(buffers, HashSet::default(), true, cx);
    }

    pub fn restart_language_servers_for_buffers(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        only_restart_servers: HashSet<LanguageServerSelector>,
        clear_stopped: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::RestartLanguageServers {
                project_id,
                buffer_ids: buffers
                    .into_iter()
                    .map(|b| b.read(cx).remote_id().to_proto())
                    .collect(),
                only_servers: only_restart_servers
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
                all: false,
            });
            cx.background_spawn(request).detach_and_log_err(cx);
        } else {
            let (stopped_names, stop_task) = if only_restart_servers.is_empty() {
                self.stop_local_language_servers_for_buffers(&buffers, HashSet::default(), cx)
            } else {
                self.stop_local_language_servers_for_buffers(&[], only_restart_servers.clone(), cx)
            };
            cx.spawn(async move |lsp_store, cx| {
                stop_task.await;
                lsp_store.update(cx, |lsp_store, cx| {
                    if clear_stopped {
                        if let Some(local) = lsp_store.as_local_mut() {
                            if only_restart_servers.is_empty() {
                                // A full restart of these buffers un-suppresses every
                                // manually-stopped server, even ones that are no longer
                                // running (and so weren't returned in `stopped_names`).
                                local.stopped_language_servers.clear();
                            } else {
                                for name in &stopped_names {
                                    local.stopped_language_servers.remove(name);
                                }
                                for selector in &only_restart_servers {
                                    if let LanguageServerSelector::Name(name) = selector {
                                        local.stopped_language_servers.remove(name);
                                    }
                                }
                            }
                        }
                    }
                    for buffer in buffers {
                        lsp_store.register_buffer_with_language_servers(
                            &buffer,
                            only_restart_servers.clone(),
                            true,
                            cx,
                        );
                    }
                })
            })
            .detach();
        }
    }

    pub fn stop_language_servers_for_buffers(
        &mut self,
        buffers: Vec<Entity<Buffer>>,
        also_stop_servers: HashSet<LanguageServerSelector>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::StopLanguageServers {
                project_id,
                buffer_ids: buffers
                    .into_iter()
                    .map(|b| b.read(cx).remote_id().to_proto())
                    .collect(),
                also_servers: also_stop_servers
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
                all: false,
            });
            cx.background_spawn(async move {
                let _ = request.await?;
                Ok(())
            })
        } else {
            let (stopped_names, task) =
                self.stop_local_language_servers_for_buffers(&buffers, also_stop_servers, cx);
            if let Some(local) = self.as_local_mut() {
                local.stopped_language_servers.extend(stopped_names);
            }
            cx.background_spawn(async move {
                task.await;
                Ok(())
            })
        }
    }

    pub(super) fn stop_local_language_servers_for_buffers(
        &mut self,
        buffers: &[Entity<Buffer>],
        also_stop_servers: HashSet<LanguageServerSelector>,
        cx: &mut Context<Self>,
    ) -> (HashSet<LanguageServerName>, Task<()>) {
        let Some(local) = self.as_local_mut() else {
            return (HashSet::default(), Task::ready(()));
        };
        let mut language_server_names_to_stop = BTreeSet::default();
        let mut language_servers_to_stop = also_stop_servers
            .into_iter()
            .flat_map(|selector| match selector {
                LanguageServerSelector::Id(id) => Some(id),
                LanguageServerSelector::Name(name) => {
                    language_server_names_to_stop.insert(name);
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        let mut covered_worktrees = HashSet::default();
        for buffer in buffers {
            buffer.update(cx, |buffer, cx| {
                language_servers_to_stop.extend(local.language_server_ids_for_buffer(buffer, cx));
                if let Some(worktree_id) = buffer.file().map(|f| f.worktree_id(cx))
                    && covered_worktrees.insert(worktree_id)
                {
                    language_server_names_to_stop.retain(|name| {
                        let old_ids_count = language_servers_to_stop.len();
                        let all_language_servers_with_this_name = local
                            .language_server_ids
                            .iter()
                            .filter_map(|(seed, state)| seed.name.eq(name).then(|| state.id));
                        language_servers_to_stop.extend(all_language_servers_with_this_name);
                        old_ids_count == language_servers_to_stop.len()
                    });
                }
            });
        }
        for name in language_server_names_to_stop {
            language_servers_to_stop.extend(
                local
                    .language_server_ids
                    .iter()
                    .filter_map(|(seed, v)| seed.name.eq(&name).then(|| v.id)),
            );
        }

        let stopped_names: HashSet<LanguageServerName> = language_servers_to_stop
            .iter()
            .filter_map(|id| {
                local
                    .language_server_ids
                    .iter()
                    .find(|(_, state)| state.id == *id)
                    .map(|(seed, _)| seed.name.clone())
            })
            .collect();

        local.lsp_tree.remove_nodes(&language_servers_to_stop);
        let tasks = language_servers_to_stop
            .into_iter()
            .map(|server| self.stop_local_language_server(server, cx))
            .collect::<Vec<_>>();

        (
            stopped_names,
            cx.background_spawn(futures::future::join_all(tasks).map(|_| ())),
        )
    }

    pub(crate) fn cancel_language_server_work_for_buffers(
        &mut self,
        buffers: impl IntoIterator<Item = Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) {
        if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::CancelLanguageServerWork {
                project_id,
                work: Some(proto::cancel_language_server_work::Work::Buffers(
                    proto::cancel_language_server_work::Buffers {
                        buffer_ids: buffers
                            .into_iter()
                            .map(|b| b.read(cx).remote_id().to_proto())
                            .collect(),
                    },
                )),
            });
            cx.background_spawn(request).detach_and_log_err(cx);
        } else if let Some(local) = self.as_local() {
            let servers = buffers
                .into_iter()
                .flat_map(|buffer| {
                    buffer.update(cx, |buffer, cx| {
                        local.language_server_ids_for_buffer(buffer, cx).into_iter()
                    })
                })
                .collect::<HashSet<_>>();
            for server_id in servers {
                self.cancel_language_server_work(server_id, None, cx);
            }
        }
    }

    pub(crate) fn cancel_language_server_work(
        &mut self,
        server_id: LanguageServerId,
        token_to_cancel: Option<ProgressToken>,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.as_local() {
            let status = self.language_server_statuses.get(&server_id);
            let server = local.language_servers.get(&server_id);
            if let Some((LanguageServerState::Running { server, .. }, status)) = server.zip(status)
            {
                for (token, progress) in &status.pending_work {
                    if let Some(token_to_cancel) = token_to_cancel.as_ref()
                        && token != token_to_cancel
                    {
                        continue;
                    }
                    if progress.is_cancellable {
                        server
                            .notify::<lsp::notification::WorkDoneProgressCancel>(
                                WorkDoneProgressCancelParams {
                                    token: token.to_lsp(),
                                },
                            )
                            .ok();
                    }
                }
            }
        } else if let Some((client, project_id)) = self.upstream_client() {
            let request = client.request(proto::CancelLanguageServerWork {
                project_id,
                work: Some(
                    proto::cancel_language_server_work::Work::LanguageServerWork(
                        proto::cancel_language_server_work::LanguageServerWork {
                            language_server_id: server_id.to_proto(),
                            token: token_to_cancel.map(|token| token.to_proto()),
                        },
                    ),
                ),
            });
            cx.background_spawn(request).detach_and_log_err(cx);
        }
    }
}
