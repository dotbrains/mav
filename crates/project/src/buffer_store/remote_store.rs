use super::*;

impl RemoteBufferStore {
    pub fn wait_for_remote_buffer(
        &mut self,
        id: BufferId,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        let (tx, rx) = oneshot::channel();
        self.remote_buffer_listeners.entry(id).or_default().push(tx);

        cx.spawn(async move |this, cx| {
            if let Some(buffer) = this
                .read_with(cx, |buffer_store, _| buffer_store.get(id))
                .ok()
                .flatten()
            {
                return Ok(buffer);
            }

            cx.background_spawn(async move { rx.await? }).await
        })
    }

    pub(super) fn save_remote_buffer(
        &self,
        buffer_handle: Entity<Buffer>,
        new_path: Option<proto::ProjectPath>,
        cx: &Context<BufferStore>,
    ) -> Task<Result<()>> {
        let buffer = buffer_handle.read(cx);
        let buffer_id = buffer.remote_id().into();
        let version = buffer.version();
        let rpc = self.upstream_client.clone();
        let project_id = self.project_id;
        cx.spawn(async move |_, cx| {
            let response = rpc
                .request(proto::SaveBuffer {
                    project_id,
                    buffer_id,
                    new_path,
                    version: serialize_version(&version),
                })
                .await?;
            let version = deserialize_version(&response.version);
            let mtime = response.mtime.map(|mtime| mtime.into());

            buffer_handle.update(cx, |buffer, cx| {
                buffer.did_save(version.clone(), mtime, cx);
            });

            Ok(())
        })
    }

    pub fn handle_create_buffer_for_peer(
        &mut self,
        envelope: TypedEnvelope<proto::CreateBufferForPeer>,
        replica_id: ReplicaId,
        capability: Capability,
        cx: &mut Context<BufferStore>,
    ) -> Result<Option<Entity<Buffer>>> {
        match envelope.payload.variant.context("missing variant")? {
            proto::create_buffer_for_peer::Variant::State(mut state) => {
                let buffer_id = BufferId::new(state.id)?;

                let buffer_result = maybe!({
                    let mut buffer_file = None;
                    if let Some(file) = state.file.take() {
                        let worktree_id = worktree::WorktreeId::from_proto(file.worktree_id);
                        let worktree = self
                            .worktree_store
                            .read(cx)
                            .worktree_for_id(worktree_id, cx)
                            .with_context(|| {
                                format!("no worktree found for id {}", file.worktree_id)
                            })?;
                        buffer_file = Some(Arc::new(File::from_proto(file, worktree, cx)?)
                            as Arc<dyn language::File>);
                    }
                    Buffer::from_proto(replica_id, capability, state, buffer_file)
                });

                match buffer_result {
                    Ok(buffer) => {
                        let buffer = cx.new(|_| buffer);
                        self.loading_remote_buffers_by_id.insert(buffer_id, buffer);
                    }
                    Err(error) => {
                        if let Some(listeners) = self.remote_buffer_listeners.remove(&buffer_id) {
                            for listener in listeners {
                                listener.send(Err(anyhow!(error.cloned()))).ok();
                            }
                        }
                    }
                }
            }
            proto::create_buffer_for_peer::Variant::Chunk(chunk) => {
                let buffer_id = BufferId::new(chunk.buffer_id)?;
                let buffer = self
                    .loading_remote_buffers_by_id
                    .get(&buffer_id)
                    .cloned()
                    .with_context(|| {
                        format!(
                            "received chunk for buffer {} without initial state",
                            chunk.buffer_id
                        )
                    })?;

                let result = maybe!({
                    let operations = chunk
                        .operations
                        .into_iter()
                        .map(language::proto::deserialize_operation)
                        .collect::<Result<Vec<_>>>()?;
                    buffer.update(cx, |buffer, cx| buffer.apply_ops(operations, cx));
                    anyhow::Ok(())
                });

                if let Err(error) = result {
                    self.loading_remote_buffers_by_id.remove(&buffer_id);
                    if let Some(listeners) = self.remote_buffer_listeners.remove(&buffer_id) {
                        for listener in listeners {
                            listener.send(Err(error.cloned())).ok();
                        }
                    }
                } else if chunk.is_last {
                    self.loading_remote_buffers_by_id.remove(&buffer_id);
                    if self.upstream_client.is_via_collab() {
                        // retain buffers sent by peers to avoid races.
                        self.shared_with_me.insert(buffer.clone());
                    }

                    if let Some(senders) = self.remote_buffer_listeners.remove(&buffer_id) {
                        for sender in senders {
                            sender.send(Ok(buffer.clone())).ok();
                        }
                    }
                    return Ok(Some(buffer));
                }
            }
        }
        Ok(None)
    }

    pub fn incomplete_buffer_ids(&self) -> Vec<BufferId> {
        self.loading_remote_buffers_by_id
            .keys()
            .copied()
            .collect::<Vec<_>>()
    }

    pub fn deserialize_project_transaction(
        &self,
        message: proto::ProjectTransaction,
        push_to_history: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<ProjectTransaction>> {
        cx.spawn(async move |this, cx| {
            let mut project_transaction = ProjectTransaction::default();
            for (buffer_id, transaction) in message.buffer_ids.into_iter().zip(message.transactions)
            {
                let buffer_id = BufferId::new(buffer_id)?;
                let buffer = this
                    .update(cx, |this, cx| this.wait_for_remote_buffer(buffer_id, cx))?
                    .await?;
                let transaction = language::proto::deserialize_transaction(transaction)?;
                project_transaction.0.insert(buffer, transaction);
            }

            for (buffer, transaction) in &project_transaction.0 {
                buffer
                    .update(cx, |buffer, _| {
                        buffer.wait_for_edits(transaction.edit_ids.iter().copied())
                    })
                    .await?;

                if push_to_history {
                    buffer.update(cx, |buffer, _| {
                        buffer.push_transaction(transaction.clone(), Instant::now());
                        buffer.finalize_last_transaction();
                    });
                }
            }

            Ok(project_transaction)
        })
    }

    pub(super) fn open_buffer(
        &self,
        path: Arc<RelPath>,
        worktree: Entity<Worktree>,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        let worktree_id = worktree.read(cx).id().to_proto();
        let project_id = self.project_id;
        let client = self.upstream_client.clone();
        cx.spawn(async move |this, cx| {
            let response = client
                .request(proto::OpenBufferByPath {
                    project_id,
                    worktree_id,
                    path: path.to_proto(),
                })
                .await?;
            let buffer_id = BufferId::new(response.buffer_id)?;

            let buffer = this
                .update(cx, {
                    |this, cx| this.wait_for_remote_buffer(buffer_id, cx)
                })?
                .await?;

            Ok(buffer)
        })
    }

    pub(super) fn create_buffer(
        &self,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        let create = self.upstream_client.request(proto::OpenNewBuffer {
            project_id: self.project_id,
        });
        cx.spawn(async move |this, cx| {
            let response = create.await?;
            let buffer_id = BufferId::new(response.buffer_id)?;

            let buffer = this
                .update(cx, |this, cx| {
                    if !project_searchable {
                        this.non_searchable_buffers.insert(buffer_id);
                    }
                    this.wait_for_remote_buffer(buffer_id, cx)
                })?
                .await?;
            if let Some(language) = language {
                buffer.update(cx, |buffer, cx| {
                    buffer.set_language(Some(language), cx);
                });
            }
            Ok(buffer)
        })
    }

    pub(super) fn reload_buffers(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        push_to_history: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<ProjectTransaction>> {
        let request = self.upstream_client.request(proto::ReloadBuffers {
            project_id: self.project_id,
            buffer_ids: buffers
                .iter()
                .map(|buffer| buffer.read(cx).remote_id().to_proto())
                .collect(),
        });

        cx.spawn(async move |this, cx| {
            let response = request.await?.transaction.context("missing transaction")?;
            this.update(cx, |this, cx| {
                this.deserialize_project_transaction(response, push_to_history, cx)
            })?
            .await
        })
    }
}
