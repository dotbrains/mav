use super::*;

impl Project {
    pub(crate) async fn handle_synchronize_buffers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::SynchronizeBuffers>,
        mut cx: AsyncApp,
    ) -> Result<proto::SynchronizeBuffersResponse> {
        let response = this.update(&mut cx, |this, cx| {
            let client = this.collab_client.clone();
            this.buffer_store.update(cx, |this, cx| {
                this.handle_synchronize_buffers(envelope, cx, client)
            })
        })?;

        Ok(response)
    }

    // Goes from client to host.
    pub(crate) async fn handle_search_candidate_buffers(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::FindSearchCandidates>,
        mut cx: AsyncApp,
    ) -> Result<proto::Ack> {
        let peer_id = envelope.original_sender_id.unwrap_or(envelope.sender_id);
        let message = envelope.payload;
        let project_id = message.project_id;
        let path_style = this.read_with(&cx, |this, cx| this.path_style(cx));
        let query =
            SearchQuery::from_proto(message.query.context("missing query field")?, path_style)?;

        let handle = message.handle;
        let buffer_store = this.read_with(&cx, |this, _| this.buffer_store().clone());
        let client = this.read_with(&cx, |this, _| this.client());
        let task = cx.spawn(async move |cx| {
            let results = this.update(cx, |this, cx| {
                this.search_impl(query, cx).matching_buffers(cx)
            });
            let (batcher, batches) = project_search::AdaptiveBatcher::new(cx.background_executor());
            let mut new_matches = Box::pin(results.rx);

            let sender_task = cx.background_executor().spawn({
                let client = client.clone();
                async move {
                    let mut batches = std::pin::pin!(batches);
                    while let Some(buffer_ids) = batches.next().await {
                        client
                            .request(proto::FindSearchCandidatesChunk {
                                handle,
                                peer_id: Some(peer_id),
                                project_id,
                                variant: Some(
                                    proto::find_search_candidates_chunk::Variant::Matches(
                                        proto::FindSearchCandidatesMatches { buffer_ids },
                                    ),
                                ),
                            })
                            .await?;
                    }
                    anyhow::Ok(())
                }
            });

            while let Some(buffer) = new_matches.next().await {
                let buffer_id = this.update(cx, |this, cx| {
                    this.create_buffer_for_peer(&buffer, peer_id, cx).to_proto()
                });
                batcher.push(buffer_id).await;
            }
            batcher.flush().await;

            sender_task.await?;

            let _ = client
                .request(proto::FindSearchCandidatesChunk {
                    handle,
                    peer_id: Some(peer_id),
                    project_id,
                    variant: Some(proto::find_search_candidates_chunk::Variant::Done(
                        proto::FindSearchCandidatesDone {},
                    )),
                })
                .await?;
            anyhow::Ok(())
        });
        buffer_store.update(&mut cx, |this, _| {
            this.register_ongoing_project_search((peer_id, handle), task);
        });

        Ok(proto::Ack {})
    }

    pub(crate) async fn handle_open_buffer_by_id(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OpenBufferById>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenBufferResponse> {
        let peer_id = envelope.original_sender_id()?;
        let buffer_id = BufferId::new(envelope.payload.id)?;
        let buffer = this
            .update(&mut cx, |this, cx| this.open_buffer_by_id(buffer_id, cx))
            .await?;
        Project::respond_to_open_buffer_request(this, buffer, peer_id, &mut cx)
    }

    pub(crate) async fn handle_open_buffer_by_path(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OpenBufferByPath>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenBufferResponse> {
        let peer_id = envelope.original_sender_id()?;
        let worktree_id = WorktreeId::from_proto(envelope.payload.worktree_id);
        let path = RelPath::from_proto(&envelope.payload.path)?;
        let open_buffer = this
            .update(&mut cx, |this, cx| {
                this.open_buffer(ProjectPath { worktree_id, path }, cx)
            })
            .await?;
        Project::respond_to_open_buffer_request(this, open_buffer, peer_id, &mut cx)
    }

    pub(crate) async fn handle_open_new_buffer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::OpenNewBuffer>,
        mut cx: AsyncApp,
    ) -> Result<proto::OpenBufferResponse> {
        let buffer = this
            .update(&mut cx, |this, cx| this.create_buffer(None, true, cx))
            .await?;
        let peer_id = envelope.original_sender_id()?;

        Project::respond_to_open_buffer_request(this, buffer, peer_id, &mut cx)
    }

    pub(crate) fn respond_to_open_buffer_request(
        this: Entity<Self>,
        buffer: Entity<Buffer>,
        peer_id: proto::PeerId,
        cx: &mut AsyncApp,
    ) -> Result<proto::OpenBufferResponse> {
        this.update(cx, |this, cx| {
            let is_private = buffer
                .read(cx)
                .file()
                .map(|f| f.is_private())
                .unwrap_or_default();
            anyhow::ensure!(!is_private, ErrorCode::UnsharedItem);
            Ok(proto::OpenBufferResponse {
                buffer_id: this.create_buffer_for_peer(&buffer, peer_id, cx).into(),
            })
        })
    }

    pub(crate) fn create_buffer_for_peer(
        &mut self,
        buffer: &Entity<Buffer>,
        peer_id: proto::PeerId,
        cx: &mut App,
    ) -> BufferId {
        self.buffer_store
            .update(cx, |buffer_store, cx| {
                buffer_store.create_buffer_for_peer(buffer, peer_id, cx)
            })
            .detach_and_log_err(cx);
        buffer.read(cx).remote_id()
    }

    pub(crate) async fn handle_create_image_for_peer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::CreateImageForPeer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        this.update(&mut cx, |this, cx| {
            this.image_store.update(cx, |image_store, cx| {
                image_store.handle_create_image_for_peer(envelope, cx)
            })
        })
    }

    pub(crate) async fn handle_create_file_for_peer(
        this: Entity<Self>,
        envelope: TypedEnvelope<proto::CreateFileForPeer>,
        mut cx: AsyncApp,
    ) -> Result<()> {
        use proto::create_file_for_peer::Variant;
        log::debug!("handle_create_file_for_peer: received message");

        let downloading_files: Arc<Mutex<HashMap<(WorktreeId, String), DownloadingFile>>> =
            this.update(&mut cx, |this, _| this.downloading_files.clone());

        match &envelope.payload.variant {
            Some(Variant::State(state)) => {
                log::debug!(
                    "handle_create_file_for_peer: got State: id={}, content_size={}",
                    state.id,
                    state.content_size
                );

                // Extract worktree_id and path from the File field
                if let Some(ref file) = state.file {
                    let worktree_id = WorktreeId::from_proto(file.worktree_id);
                    let path = file.path.clone();
                    let key = (worktree_id, path);
                    log::debug!("handle_create_file_for_peer: looking up key={:?}", key);

                    let empty_file_destination: Option<PathBuf> = {
                        let mut files = downloading_files.lock();
                        log::trace!(
                            "handle_create_file_for_peer: current downloading_files keys: {:?}",
                            files.keys().collect::<Vec<_>>()
                        );

                        if let Some(file_entry) = files.get_mut(&key) {
                            file_entry.total_size = state.content_size;
                            file_entry.file_id = Some(state.id);
                            log::debug!(
                                "handle_create_file_for_peer: updated file entry: total_size={}, file_id={}",
                                state.content_size,
                                state.id
                            );
                        } else {
                            log::warn!(
                                "handle_create_file_for_peer: key={:?} not found in downloading_files",
                                key
                            );
                        }

                        if state.content_size == 0 {
                            // No chunks will arrive for an empty file; write it now.
                            files.remove(&key).map(|entry| entry.destination_path)
                        } else {
                            None
                        }
                    };

                    if let Some(destination) = empty_file_destination {
                        log::debug!(
                            "handle_create_file_for_peer: writing empty file to {:?}",
                            destination
                        );
                        match smol::fs::write(&destination, &[] as &[u8]).await {
                            Ok(_) => log::info!(
                                "handle_create_file_for_peer: successfully wrote file to {:?}",
                                destination
                            ),
                            Err(e) => log::error!(
                                "handle_create_file_for_peer: failed to write empty file: {:?}",
                                e
                            ),
                        }
                    }
                } else {
                    log::warn!("handle_create_file_for_peer: State has no file field");
                }
            }
            Some(Variant::Chunk(chunk)) => {
                log::debug!(
                    "handle_create_file_for_peer: got Chunk: file_id={}, data_len={}",
                    chunk.file_id,
                    chunk.data.len()
                );

                // Extract data while holding the lock, then release it before await
                let (key_to_remove, write_info): (
                    Option<(WorktreeId, String)>,
                    Option<(PathBuf, Vec<u8>)>,
                ) = {
                    let mut files = downloading_files.lock();
                    let mut found_key: Option<(WorktreeId, String)> = None;
                    let mut write_data: Option<(PathBuf, Vec<u8>)> = None;

                    for (key, file_entry) in files.iter_mut() {
                        if file_entry.file_id == Some(chunk.file_id) {
                            file_entry.chunks.extend_from_slice(&chunk.data);
                            log::debug!(
                                "handle_create_file_for_peer: accumulated {} bytes, total_size={}",
                                file_entry.chunks.len(),
                                file_entry.total_size
                            );

                            if file_entry.chunks.len() as u64 >= file_entry.total_size
                                && file_entry.total_size > 0
                            {
                                let destination = file_entry.destination_path.clone();
                                let content = std::mem::take(&mut file_entry.chunks);
                                found_key = Some(key.clone());
                                write_data = Some((destination, content));
                            }
                            break;
                        }
                    }
                    (found_key, write_data)
                }; // MutexGuard is dropped here

                // Perform the async write outside the lock
                if let Some((destination, content)) = write_info {
                    log::debug!(
                        "handle_create_file_for_peer: writing {} bytes to {:?}",
                        content.len(),
                        destination
                    );
                    match smol::fs::write(&destination, &content).await {
                        Ok(_) => log::info!(
                            "handle_create_file_for_peer: successfully wrote file to {:?}",
                            destination
                        ),
                        Err(e) => log::error!(
                            "handle_create_file_for_peer: failed to write file: {:?}",
                            e
                        ),
                    }
                }

                // Remove the completed entry
                if let Some(key) = key_to_remove {
                    downloading_files.lock().remove(&key);
                    log::debug!("handle_create_file_for_peer: removed completed download entry");
                }
            }
            None => {
                log::warn!("handle_create_file_for_peer: got None variant");
            }
        }

        Ok(())
    }
}
