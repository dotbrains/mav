use super::*;

impl BufferStore {
    pub async fn handle_update_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateBuffer>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let payload = envelope.payload;
        let buffer_id = BufferId::new(payload.buffer_id)?;
        let ops = payload
            .operations
            .into_iter()
            .map(language::proto::deserialize_operation)
            .collect::<Result<Vec<_>, _>>()?;
        this.update(&mut cx, |this, cx| {
            match this.opened_buffers.entry(buffer_id) {
                hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                    OpenBuffer::Operations(operations) => operations.extend_from_slice(&ops),
                    OpenBuffer::Complete { buffer, .. } => {
                        if let Some(buffer) = buffer.upgrade() {
                            buffer.update(cx, |buffer, cx| buffer.apply_ops(ops, cx));
                        }
                    }
                },
                hash_map::Entry::Vacant(e) => {
                    e.insert(OpenBuffer::Operations(ops));
                }
            }
            Ok(proto::Ack {})
        })
    }

    pub fn register_shared_lsp_handle(
        &mut self,
        peer_id: proto::PeerId,
        buffer_id: BufferId,
        handle: OpenLspBufferHandle,
    ) {
        if let Some(shared_buffers) = self.shared_buffers.get_mut(&peer_id)
            && let Some(buffer) = shared_buffers.get_mut(&buffer_id)
        {
            buffer.lsp_handle = Some(handle);
            return;
        }
        debug_panic!("tried to register shared lsp handle, but buffer was not shared")
    }

    pub fn handle_synchronize_buffers(
        &mut self,
        envelope: TypedEnvelope<proto::SynchronizeBuffers>,
        cx: &mut Context<Self>,
        client: Arc<Client>,
    ) -> Result<proto::SynchronizeBuffersResponse> {
        let project_id = envelope.payload.project_id;
        let mut response = proto::SynchronizeBuffersResponse {
            buffers: Default::default(),
        };
        let Some(guest_id) = envelope.original_sender_id else {
            anyhow::bail!("missing original_sender_id on SynchronizeBuffers request");
        };

        self.shared_buffers.entry(guest_id).or_default().clear();
        for buffer in envelope.payload.buffers {
            let buffer_id = BufferId::new(buffer.id)?;
            let remote_version = language::proto::deserialize_version(&buffer.version);
            if let Some(buffer) = self.get(buffer_id) {
                self.shared_buffers
                    .entry(guest_id)
                    .or_default()
                    .entry(buffer_id)
                    .or_insert_with(|| SharedBuffer {
                        buffer: buffer.clone(),
                        lsp_handle: None,
                    });

                let buffer = buffer.read(cx);
                response.buffers.push(proto::BufferVersion {
                    id: buffer_id.into(),
                    version: language::proto::serialize_version(&buffer.version),
                });

                let operations = buffer.serialize_ops(Some(remote_version), cx);
                let client = client.clone();
                if let Some(file) = buffer.file() {
                    client
                        .send(proto::UpdateBufferFile {
                            project_id,
                            buffer_id: buffer_id.into(),
                            file: Some(file.to_proto(cx)),
                        })
                        .log_err();
                }

                // TODO(max): do something
                // client
                //     .send(proto::UpdateStagedText {
                //         project_id,
                //         buffer_id: buffer_id.into(),
                //         diff_base: buffer.diff_base().map(ToString::to_string),
                //     })
                //     .log_err();

                client
                    .send(proto::BufferReloaded {
                        project_id,
                        buffer_id: buffer_id.into(),
                        version: language::proto::serialize_version(buffer.saved_version()),
                        mtime: buffer.saved_mtime().map(|time| time.into()),
                        line_ending: language::proto::serialize_line_ending(buffer.line_ending())
                            as i32,
                    })
                    .log_err();

                cx.background_spawn(
                    async move {
                        let operations = operations.await;
                        for chunk in split_operations(operations) {
                            client
                                .request(proto::UpdateBuffer {
                                    project_id,
                                    buffer_id: buffer_id.into(),
                                    operations: chunk,
                                })
                                .await?;
                        }
                        anyhow::Ok(())
                    }
                    .log_err(),
                )
                .detach();
            }
        }
        Ok(response)
    }

    pub fn handle_create_buffer_for_peer(
        &mut self,
        envelope: TypedEnvelope<proto::CreateBufferForPeer>,
        replica_id: ReplicaId,
        capability: Capability,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let remote = self
            .as_remote_mut()
            .context("buffer store is not a remote")?;

        if let Some(buffer) =
            remote.handle_create_buffer_for_peer(envelope, replica_id, capability, cx)?
        {
            self.add_buffer(buffer, cx)?;
        }

        Ok(())
    }

    pub async fn handle_update_buffer_file(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::UpdateBufferFile>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let buffer_id = envelope.payload.buffer_id;
        let buffer_id = BufferId::new(buffer_id)?;

        this.update(&mut cx, |this, cx| {
            let payload = envelope.payload.clone();
            if let Some(buffer) = this.get_possibly_incomplete(buffer_id) {
                let file = payload.file.context("invalid file")?;
                let worktree = this
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(WorktreeId::from_proto(file.worktree_id), cx)
                    .context("no such worktree")?;
                let file = File::from_proto(file, worktree, cx)?;
                let old_file = buffer.update(cx, |buffer, cx| {
                    let old_file = buffer.file().cloned();
                    let new_path = file.path.clone();

                    buffer.file_updated(Arc::new(file), cx);
                    if old_file.as_ref().is_none_or(|old| *old.path() != new_path) {
                        Some(old_file)
                    } else {
                        None
                    }
                });
                if let Some(old_file) = old_file {
                    cx.emit(BufferStoreEvent::BufferChangedFilePath { buffer, old_file });
                }
            }
            if let Some((downstream_client, project_id)) = this.downstream_client.as_ref() {
                downstream_client
                    .send(proto::UpdateBufferFile {
                        project_id: *project_id,
                        buffer_id: buffer_id.into(),
                        file: envelope.payload.file,
                    })
                    .log_err();
            }
            Ok(())
        })
    }

    pub async fn handle_save_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::SaveBuffer>,
        mut cx: AsyncApp,
    ) -> Result<proto::BufferSaved> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let (buffer, project_id) = this.read_with(&cx, |this, _| {
            anyhow::Ok((
                this.get_existing(buffer_id)?,
                this.downstream_client
                    .as_ref()
                    .map(|(_, project_id)| *project_id)
                    .context("project is not shared")?,
            ))
        })?;
        buffer
            .update(&mut cx, |buffer, _| {
                buffer.wait_for_version(deserialize_version(&envelope.payload.version))
            })
            .await?;
        let buffer_id = buffer.read_with(&cx, |buffer, _| buffer.remote_id());

        if let Some(new_path) = envelope.payload.new_path
            && let Some(new_path) = ProjectPath::from_proto(new_path)
        {
            this.update(&mut cx, |this, cx| {
                this.save_buffer_as(buffer.clone(), new_path, cx)
            })
            .await?;
        } else {
            this.update(&mut cx, |this, cx| this.save_buffer(buffer.clone(), cx))
                .await?;
        }

        Ok(buffer.read_with(&cx, |buffer, _| proto::BufferSaved {
            project_id,
            buffer_id: buffer_id.into(),
            version: serialize_version(buffer.saved_version()),
            mtime: buffer.saved_mtime().map(|time| time.into()),
        }))
    }

    pub async fn handle_close_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::CloseBuffer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let peer_id = envelope.sender_id;
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        this.update(&mut cx, |this, cx| {
            if let Some(shared) = this.shared_buffers.get_mut(&peer_id)
                && shared.remove(&buffer_id).is_some()
            {
                cx.emit(BufferStoreEvent::SharedBufferClosed(peer_id, buffer_id));
                if shared.is_empty() {
                    this.shared_buffers.remove(&peer_id);
                }
                return;
            }
            debug_panic!(
                "peer_id {} closed buffer_id {} which was either not open or already closed",
                peer_id,
                buffer_id
            )
        });
        Ok(())
    }

    pub async fn handle_buffer_saved(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::BufferSaved>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let version = deserialize_version(&envelope.payload.version);
        let mtime = envelope.payload.mtime.clone().map(|time| time.into());
        this.update(&mut cx, move |this, cx| {
            if let Some(buffer) = this.get_possibly_incomplete(buffer_id) {
                buffer.update(cx, |buffer, cx| {
                    buffer.did_save(version, mtime, cx);
                });
            }

            if let Some((downstream_client, project_id)) = this.downstream_client.as_ref() {
                downstream_client
                    .send(proto::BufferSaved {
                        project_id: *project_id,
                        buffer_id: buffer_id.into(),
                        mtime: envelope.payload.mtime,
                        version: envelope.payload.version,
                    })
                    .log_err();
            }
        });
        Ok(())
    }

    pub async fn handle_buffer_reloaded(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::BufferReloaded>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let buffer_id = BufferId::new(envelope.payload.buffer_id)?;
        let version = deserialize_version(&envelope.payload.version);
        let mtime = envelope.payload.mtime.clone().map(|time| time.into());
        let line_ending = deserialize_line_ending(
            proto::LineEnding::from_i32(envelope.payload.line_ending)
                .context("missing line ending")?,
        );
        this.update(&mut cx, |this, cx| {
            if let Some(buffer) = this.get_possibly_incomplete(buffer_id) {
                buffer.update(cx, |buffer, cx| {
                    buffer.did_reload(version, line_ending, mtime, cx);
                });
            }

            if let Some((downstream_client, project_id)) = this.downstream_client.as_ref() {
                downstream_client
                    .send(proto::BufferReloaded {
                        project_id: *project_id,
                        buffer_id: buffer_id.into(),
                        mtime: envelope.payload.mtime,
                        version: envelope.payload.version,
                        line_ending: envelope.payload.line_ending,
                    })
                    .log_err();
            }
        });
        Ok(())
    }

    pub fn reload_buffers(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        if buffers.is_empty() {
            return Task::ready(Ok(ProjectTransaction::default()));
        }
        match &self.state {
            BufferStoreState::Local(this) => this.reload_buffers(buffers, push_to_history, cx),
            BufferStoreState::Remote(this) => this.reload_buffers(buffers, push_to_history, cx),
        }
    }

    pub(super) async fn handle_reload_buffers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::ReloadBuffers>,
        mut cx: AsyncApp,
    ) -> Result<proto::ReloadBuffersResponse> {
        let sender_id = envelope.original_sender_id().unwrap_or_default();
        let reload = this.update(&mut cx, |this, cx| {
            let mut buffers = HashSet::default();
            for buffer_id in &envelope.payload.buffer_ids {
                let buffer_id = BufferId::new(*buffer_id)?;
                buffers.insert(this.get_existing(buffer_id)?);
            }
            anyhow::Ok(this.reload_buffers(buffers, false, cx))
        })?;

        let project_transaction = reload.await?;
        let project_transaction = this.update(&mut cx, |this, cx| {
            this.serialize_project_transaction_for_peer(project_transaction, sender_id, cx)
        });
        Ok(proto::ReloadBuffersResponse {
            transaction: Some(project_transaction),
        })
    }
}
