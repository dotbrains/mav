use super::*;

impl BufferStore {
    pub fn create_buffer_for_peer(
        &mut self,
        buffer: &Entity<Buffer>,
        peer_id: proto::PeerId,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let buffer_id = buffer.read(cx).remote_id();
        let shared_buffers = self.shared_buffers.entry(peer_id).or_default();
        if shared_buffers.contains_key(&buffer_id) {
            return Task::ready(Ok(()));
        }
        shared_buffers.insert(
            buffer_id,
            SharedBuffer {
                buffer: buffer.clone(),
                lsp_handle: None,
            },
        );

        let Some((client, project_id)) = self.downstream_client.clone() else {
            return Task::ready(Ok(()));
        };

        cx.spawn(async move |this, cx| {
            let Some(buffer) = this.read_with(cx, |this, _| this.get(buffer_id))? else {
                return anyhow::Ok(());
            };

            let operations = buffer.update(cx, |b, cx| b.serialize_ops(None, cx));
            let operations = operations.await;
            let state = buffer.update(cx, |buffer, cx| buffer.to_proto(cx));

            let initial_state = proto::CreateBufferForPeer {
                project_id,
                peer_id: Some(peer_id),
                variant: Some(proto::create_buffer_for_peer::Variant::State(state)),
            };

            if client.send(initial_state).log_err().is_some() {
                let client = client.clone();
                cx.background_spawn(async move {
                    let mut chunks = split_operations(operations).peekable();
                    while let Some(chunk) = chunks.next() {
                        let is_last = chunks.peek().is_none();
                        client.send(proto::CreateBufferForPeer {
                            project_id,
                            peer_id: Some(peer_id),
                            variant: Some(proto::create_buffer_for_peer::Variant::Chunk(
                                proto::BufferChunk {
                                    buffer_id: buffer_id.into(),
                                    operations: chunk,
                                    is_last,
                                },
                            )),
                        })?;
                    }
                    anyhow::Ok(())
                })
                .await
                .log_err();
            }
            Ok(())
        })
    }

    pub fn forget_shared_buffers(&mut self) {
        self.shared_buffers.clear();
    }

    pub fn forget_shared_buffers_for(&mut self, peer_id: &proto::PeerId) {
        self.shared_buffers.remove(peer_id);
    }

    pub fn update_peer_id(&mut self, old_peer_id: &proto::PeerId, new_peer_id: proto::PeerId) {
        if let Some(buffers) = self.shared_buffers.remove(old_peer_id) {
            self.shared_buffers.insert(new_peer_id, buffers);
        }
    }

    pub fn has_shared_buffers(&self) -> bool {
        !self.shared_buffers.is_empty()
    }

    pub fn create_local_buffer(
        &mut self,
        text: &str,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<Self>,
    ) -> Entity<Buffer> {
        let buffer = cx.new(|cx| {
            let mut buffer = Buffer::local(text, cx)
                .with_language(language.unwrap_or_else(|| language::PLAIN_TEXT.clone()), cx);
            apply_initial_line_ending(&mut buffer, cx);
            buffer
        });

        self.add_buffer(buffer.clone(), cx).log_err();
        let buffer_id = buffer.read(cx).remote_id();
        if !project_searchable {
            self.non_searchable_buffers.insert(buffer_id);
        }

        if let Some(file) = File::from_dyn(buffer.read(cx).file()) {
            self.path_to_buffer_id.insert(
                ProjectPath {
                    worktree_id: file.worktree_id(cx),
                    path: file.path.clone(),
                },
                buffer_id,
            );
            let this = self
                .as_local_mut()
                .expect("local-only method called in a non-local context");
            if let Some(entry_id) = file.entry_id {
                this.local_buffer_ids_by_entry_id
                    .insert(entry_id, buffer_id);
            }
        }
        buffer
    }

    pub fn deserialize_project_transaction(
        &mut self,
        message: proto::ProjectTransaction,
        push_to_history: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<ProjectTransaction>> {
        if let Some(this) = self.as_remote_mut() {
            this.deserialize_project_transaction(message, push_to_history, cx)
        } else {
            debug_panic!("not a remote buffer store");
            Task::ready(Err(anyhow!("not a remote buffer store")))
        }
    }

    pub fn wait_for_remote_buffer(
        &mut self,
        id: BufferId,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        if let Some(this) = self.as_remote_mut() {
            this.wait_for_remote_buffer(id, cx)
        } else {
            debug_panic!("not a remote buffer store");
            Task::ready(Err(anyhow!("not a remote buffer store")))
        }
    }

    pub fn serialize_project_transaction_for_peer(
        &mut self,
        project_transaction: ProjectTransaction,
        peer_id: proto::PeerId,
        cx: &mut Context<Self>,
    ) -> proto::ProjectTransaction {
        let mut serialized_transaction = proto::ProjectTransaction {
            buffer_ids: Default::default(),
            transactions: Default::default(),
        };
        for (buffer, transaction) in project_transaction.0 {
            self.create_buffer_for_peer(&buffer, peer_id, cx)
                .detach_and_log_err(cx);
            serialized_transaction
                .buffer_ids
                .push(buffer.read(cx).remote_id().into());
            serialized_transaction
                .transactions
                .push(language::proto::serialize_transaction(&transaction));
        }
        serialized_transaction
    }

    pub(crate) fn register_project_search_result_handle(
        &mut self,
    ) -> (u64, async_channel::Receiver<BufferId>) {
        let (tx, rx) = async_channel::unbounded();
        let handle = util::post_inc(&mut self.project_search.next_id);
        let _old_entry = self.project_search.chunks.insert(handle, tx);
        debug_assert!(_old_entry.is_none());
        (handle, rx)
    }

    pub fn register_ongoing_project_search(
        &mut self,
        id: (PeerId, u64),
        search: Task<anyhow::Result<()>>,
    ) {
        let _old = self.project_search.searches_in_progress.insert(id, search);
        debug_assert!(_old.is_none());
    }

    pub async fn handle_find_search_candidates_cancel(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FindSearchCandidatesCancelled>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        let id = (
            envelope.original_sender_id.unwrap_or(envelope.sender_id),
            envelope.payload.handle,
        );
        let _ = this.update(&mut cx, |this, _| {
            this.project_search.searches_in_progress.remove(&id)
        });
        Ok(())
    }

    pub(crate) async fn handle_find_search_candidates_chunk(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FindSearchCandidatesChunk>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        use proto::find_search_candidates_chunk::Variant;
        let handle = envelope.payload.handle;

        let buffer_ids = match envelope
            .payload
            .variant
            .context("Expected non-null variant")?
        {
            Variant::Matches(find_search_candidates_matches) => find_search_candidates_matches
                .buffer_ids
                .into_iter()
                .filter_map(|buffer_id| BufferId::new(buffer_id).ok())
                .collect::<Vec<_>>(),
            Variant::Done(_) => {
                this.update(&mut cx, |this, _| {
                    this.project_search.chunks.remove(&handle)
                });
                return Ok(proto::Ack {});
            }
        };
        let Some(sender) = this.read_with(&mut cx, |this, _| {
            this.project_search.chunks.get(&handle).cloned()
        }) else {
            return Ok(proto::Ack {});
        };

        for buffer_id in buffer_ids {
            let Ok(_) = sender.send(buffer_id).await else {
                this.update(&mut cx, |this, _| {
                    this.project_search.chunks.remove(&handle)
                });
                return Ok(proto::Ack {});
            };
        }
        Ok(proto::Ack {})
    }
}
