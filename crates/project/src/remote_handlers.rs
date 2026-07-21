use super::*;

impl Project {
    // RPC message handlers

    pub(crate) async fn handle_unshare_project(
        this: Entity<Self>,
        _: TypedEnvelope<proto::UnshareProject>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            if this.is_local() || this.is_via_remote_server() {
                this.unshare(cx)?;
            } else {
                this.disconnected_from_host(cx);
            }
            Ok(())
        })
    }

    pub(crate) async fn handle_add_collaborator(
        this: Entity<Self>,
        mut envelope: TypedEnvelope<proto::AddProjectCollaborator>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let collaborator = envelope
            .payload
            .collaborator
            .take()
            .context("empty collaborator")?;

        let collaborator = Collaborator::from_proto(collaborator)?;
        this.update(&mut cx, |this, cx| {
            this.buffer_store.update(cx, |buffer_store, _| {
                buffer_store.forget_shared_buffers_for(&collaborator.peer_id);
            });
            this.breakpoint_store.read(cx).broadcast();
            cx.emit(Event::CollaboratorJoined(collaborator.peer_id));
            this.collaborators
                .insert(collaborator.peer_id, collaborator);
        });

        Ok(())
    }

    pub(crate) async fn handle_update_project_collaborator(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateProjectCollaborator>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let old_peer_id = envelope
            .payload
            .old_peer_id
            .context("missing old peer id")?;
        let new_peer_id = envelope
            .payload
            .new_peer_id
            .context("missing new peer id")?;
        this.update(&mut cx, |this, cx| {
            let collaborator = this
                .collaborators
                .remove(&old_peer_id)
                .context("received UpdateProjectCollaborator for unknown peer")?;
            let is_host = collaborator.is_host;
            this.collaborators.insert(new_peer_id, collaborator);

            log::info!("peer {} became {}", old_peer_id, new_peer_id,);
            this.buffer_store.update(cx, |buffer_store, _| {
                buffer_store.update_peer_id(&old_peer_id, new_peer_id)
            });

            if is_host {
                this.buffer_store
                    .update(cx, |buffer_store, _| buffer_store.discard_incomplete());
                this.enqueue_buffer_ordered_message(BufferOrderedMessage::Resync)
                    .unwrap();
                cx.emit(Event::HostReshared);
            }

            cx.emit(Event::CollaboratorUpdated {
                old_peer_id,
                new_peer_id,
            });
            Ok(())
        })
    }

    pub(crate) async fn handle_remove_collaborator(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RemoveProjectCollaborator>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            let peer_id = envelope.payload.peer_id.context("invalid peer id")?;
            let replica_id = this
                .collaborators
                .remove(&peer_id)
                .with_context(|| format!("unknown peer {peer_id:?}"))?
                .replica_id;
            this.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.forget_shared_buffers_for(&peer_id);
                for buffer in buffer_store.buffers() {
                    buffer.update(cx, |buffer, cx| buffer.remove_peer(replica_id, cx));
                }
            });
            this.git_store.update(cx, |git_store, _| {
                git_store.forget_shared_diffs_for(&peer_id);
            });

            cx.emit(Event::CollaboratorLeft(peer_id));
            Ok(())
        })
    }

    pub(crate) async fn handle_update_project(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateProject>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            // Don't handle messages that were sent before the response to us joining the project
            if envelope.message_id > this.join_project_response_message_id {
                cx.update_global::<SettingsStore, _>(|store, cx| {
                    for worktree_metadata in &envelope.payload.worktrees {
                        store
                            .clear_local_settings(WorktreeId::from_proto(worktree_metadata.id), cx)
                            .log_err();
                    }
                });

                this.set_worktrees_from_proto(envelope.payload.worktrees, cx)?;
            }
            Ok(())
        })
    }

    pub(crate) async fn handle_toast(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::Toast>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |_, cx| {
            cx.emit(Event::Toast {
                notification_id: envelope.payload.notification_id.into(),
                message: envelope.payload.message,
                link: None,
            });
            Ok(())
        })
    }

    pub(crate) async fn handle_telemetry_event(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::TelemetryEvent>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let payload = envelope.payload;
        this.update(&mut cx, |this, cx| {
            // The remote connection type, OS, version, and architecture are all
            // already known from connection setup, so they don't need to be sent
            // with each event.
            let Some((connection_type, platform, os_version)) =
                this.remote_client.as_ref().map(|client| {
                    let client = client.read(cx);
                    (
                        client.connection_type(),
                        client.remote_platform(),
                        client.remote_os_version(),
                    )
                })
            else {
                return;
            };
            this.client()
                .telemetry()
                .report_remote_event(
                    &payload.event_json,
                    connection_type,
                    platform.os.display_name().to_string(),
                    os_version,
                    platform.arch.as_str().to_string(),
                )
                .log_err();
        });
        Ok(())
    }

    pub(crate) async fn handle_language_server_prompt_request(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::LanguageServerPromptRequest>,
        mut cx: AsyncApp,
    ) -> Result<proto::LanguageServerPromptResponse> {
        let (tx, rx) = async_channel::bounded(1);
        let actions: Vec<_> = envelope
            .payload
            .actions
            .into_iter()
            .map(|action| MessageActionItem {
                title: action,
                properties: Default::default(),
            })
            .collect();
        this.update(&mut cx, |_, cx| {
            cx.emit(Event::LanguageServerPrompt(
                LanguageServerPromptRequest::new(
                    proto_to_prompt(envelope.payload.level.context("Invalid prompt level")?),
                    envelope.payload.message,
                    actions.clone(),
                    envelope.payload.lsp_name,
                    tx,
                ),
            ));

            anyhow::Ok(())
        })?;

        // We drop `this` to avoid holding a reference in this future for too
        // long.
        // If we keep the reference, we might not drop the `Project` early
        // enough when closing a window and it will only get releases on the
        // next `flush_effects()` call.
        drop(this);

        let mut rx = pin!(rx);
        let answer = rx.next().await;

        Ok(LanguageServerPromptResponse {
            action_response: answer.and_then(|answer| {
                actions
                    .iter()
                    .position(|action| *action == answer)
                    .map(|index| index as u64)
            }),
        })
    }

    pub(crate) async fn handle_hide_toast(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::HideToast>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |_, cx| {
            cx.emit(Event::HideToast {
                notification_id: envelope.payload.notification_id.into(),
            });
            Ok(())
        })
    }

    // Collab sends UpdateWorktree protos as messages
    pub(crate) async fn handle_update_worktree(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateWorktree>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |project, cx| {
            let worktree_id = WorktreeId::from_proto(envelope.payload.worktree_id);
            if let Some(worktree) = project.worktree_for_id(worktree_id, cx) {
                worktree.update(cx, |worktree, _| {
                    let worktree = worktree.as_remote_mut().unwrap();
                    worktree.update_from_remote(envelope.payload);
                });
            }
            Ok(())
        })
    }

    pub(crate) async fn handle_update_buffer_from_remote_server(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateBuffer>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let buffer_store = this.read_with(&cx, |this, cx| {
            if let Some(remote_id) = this.remote_id() {
                let mut payload = envelope.payload.clone();
                payload.project_id = remote_id;
                cx.background_spawn(this.collab_client.request(payload))
                    .detach_and_log_err(cx);
            }
            this.buffer_store.clone()
        });
        BufferStore::handle_update_buffer(buffer_store, envelope, cx).await
    }

    pub(crate) async fn handle_trust_worktrees(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::TrustWorktrees>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        if this.read_with(&cx, |project, _| project.is_via_collab()) {
            return Ok(proto::Ack {});
        }

        let trusted_worktrees = cx
            .update(|cx| TrustedWorktrees::try_get_global(cx))
            .context("missing trusted worktrees")?;
        trusted_worktrees.update(&mut cx, |trusted_worktrees, cx| {
            trusted_worktrees.trust(
                &this.read(cx).worktree_store(),
                envelope
                    .payload
                    .trusted_paths
                    .into_iter()
                    .filter_map(|proto_path| PathTrust::from_proto(proto_path))
                    .collect(),
                cx,
            );
        });
        Ok(proto::Ack {})
    }

    pub(crate) async fn handle_restrict_worktrees(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::RestrictWorktrees>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        if this.read_with(&cx, |project, _| project.is_via_collab()) {
            return Ok(proto::Ack {});
        }

        let trusted_worktrees = cx
            .update(|cx| TrustedWorktrees::try_get_global(cx))
            .context("missing trusted worktrees")?;
        trusted_worktrees.update(&mut cx, |trusted_worktrees, cx| {
            let worktree_store = this.read(cx).worktree_store().downgrade();
            let restricted_paths = envelope
                .payload
                .worktree_ids
                .into_iter()
                .map(WorktreeId::from_proto)
                .map(PathTrust::Worktree)
                .collect::<HashSet<_>>();
            trusted_worktrees.restrict(worktree_store, restricted_paths, cx);
        });
        Ok(proto::Ack {})
    }

    // Goes from host to client.
    pub(crate) async fn handle_find_search_candidates_chunk(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FindSearchCandidatesChunk>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let buffer_store = this.read_with(&mut cx, |this, _| this.buffer_store.clone());
        BufferStore::handle_find_search_candidates_chunk(buffer_store, envelope, cx).await
    }

    // Goes from client to host.
    pub(crate) async fn handle_find_search_candidates_cancel(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FindSearchCandidatesCancelled>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let buffer_store = this.read_with(&mut cx, |this, _| this.buffer_store.clone());
        BufferStore::handle_find_search_candidates_cancel(buffer_store, envelope, cx).await
    }

    pub(crate) async fn handle_update_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateBuffer>,
        cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let buffer_store = this.read_with(&cx, |this, cx| {
            if let Some(ssh) = &this.remote_client {
                let mut payload = envelope.payload.clone();
                payload.project_id = REMOTE_SERVER_PROJECT_ID;
                cx.background_spawn(ssh.read(cx).proto_client().request(payload))
                    .detach_and_log_err(cx);
            }
            this.buffer_store.clone()
        });
        BufferStore::handle_update_buffer(buffer_store, envelope, cx).await
    }

    pub(crate) fn retain_remotely_created_models(
        &mut self,
        cx: &mut Context<Self>,
    ) -> RemotelyCreatedModelGuard {
        Self::retain_remotely_created_models_impl(
            &self.remotely_created_models,
            &self.buffer_store,
            &self.worktree_store,
            cx,
        )
    }

    pub(crate) fn retain_remotely_created_models_impl(
        models: &Arc<Mutex<RemotelyCreatedModels>>,
        buffer_store: &Entity<BufferStore>,
        worktree_store: &Entity<WorktreeStore>,
        cx: &mut App,
    ) -> RemotelyCreatedModelGuard {
        {
            let mut remotely_create_models = models.lock();
            if remotely_create_models.retain_count == 0 {
                remotely_create_models.buffers = buffer_store.read(cx).buffers().collect();
                remotely_create_models.worktrees = worktree_store.read(cx).worktrees().collect();
            }
            remotely_create_models.retain_count += 1;
        }
        RemotelyCreatedModelGuard {
            remote_models: Arc::downgrade(&models),
        }
    }

    pub(crate) async fn handle_create_buffer_for_peer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::CreateBufferForPeer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            this.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.handle_create_buffer_for_peer(
                    envelope,
                    this.replica_id(),
                    this.capability(),
                    cx,
                )
            })
        })
    }

    pub(crate) async fn handle_toggle_lsp_logs(
        project: Entity<Self>,
        envelope: TypedEnvelope<proto::ToggleLspLogs>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let toggled_log_kind =
            match proto::toggle_lsp_logs::LogType::from_i32(envelope.payload.log_type)
                .context("invalid log type")?
            {
                proto::toggle_lsp_logs::LogType::Log => LogKind::Logs,
                proto::toggle_lsp_logs::LogType::Trace => LogKind::Trace,
                proto::toggle_lsp_logs::LogType::Rpc => LogKind::Rpc,
            };
        project.update(&mut cx, |_, cx| {
            cx.emit(Event::ToggleLspLogs {
                server_id: LanguageServerId::from_proto(envelope.payload.server_id),
                enabled: envelope.payload.enabled,
                toggled_log_kind,
            })
        });
        Ok(())
    }
}
