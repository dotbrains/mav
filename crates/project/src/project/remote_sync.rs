use super::*;

impl Project {
    fn synchronize_remote_buffers(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let project_id = match self.client_state {
            ProjectClientState::Collab {
                sharing_has_stopped,
                remote_id,
                ..
            } => {
                if sharing_has_stopped {
                    return Task::ready(Err(anyhow!(
                        "can't synchronize remote buffers on a readonly project"
                    )));
                } else {
                    remote_id
                }
            }
            ProjectClientState::Shared { .. } | ProjectClientState::Local => {
                return Task::ready(Err(anyhow!(
                    "can't synchronize remote buffers on a local project"
                )));
            }
        };

        let client = self.collab_client.clone();
        cx.spawn(async move |this, cx| {
            let (buffers, incomplete_buffer_ids) = this.update(cx, |this, cx| {
                this.buffer_store.read(cx).buffer_version_info(cx)
            })?;
            let response = client
                .request(proto::SynchronizeBuffers {
                    project_id,
                    buffers,
                })
                .await?;

            let send_updates_for_buffers = this.update(cx, |this, cx| {
                response
                    .buffers
                    .into_iter()
                    .map(|buffer| {
                        let client = client.clone();
                        let buffer_id = match BufferId::new(buffer.id) {
                            Ok(id) => id,
                            Err(e) => {
                                return Task::ready(Err(e));
                            }
                        };
                        let remote_version = language::proto::deserialize_version(&buffer.version);
                        if let Some(buffer) = this.buffer_for_id(buffer_id, cx) {
                            let operations =
                                buffer.read(cx).serialize_ops(Some(remote_version), cx);
                            cx.background_spawn(async move {
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
                            })
                        } else {
                            Task::ready(Ok(()))
                        }
                    })
                    .collect::<Vec<_>>()
            })?;

            // Any incomplete buffers have open requests waiting. Request that the host sends
            // creates these buffers for us again to unblock any waiting futures.
            for id in incomplete_buffer_ids {
                cx.background_spawn(client.request(proto::OpenBufferById {
                    project_id,
                    id: id.into(),
                }))
                .detach();
            }

            futures::future::join_all(send_updates_for_buffers)
                .await
                .into_iter()
                .collect()
        })
    }

    pub fn worktree_metadata_protos(&self, cx: &App) -> Vec<proto::WorktreeMetadata> {
        self.worktree_store.read(cx).worktree_metadata_protos(cx)
    }

    /// Iterator of all open buffers that have unsaved changes
    pub fn dirty_buffers<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = ProjectPath> + 'a {
        self.buffer_store.read(cx).buffers().filter_map(|buf| {
            let buf = buf.read(cx);
            if buf.is_dirty() {
                buf.project_path(cx)
            } else {
                None
            }
        })
    }

    fn set_worktrees_from_proto(
        &mut self,
        worktrees: Vec<proto::WorktreeMetadata>,
        cx: &mut Context<Project>,
    ) -> Result<()> {
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.set_worktrees_from_proto(worktrees, self.replica_id(), cx)
        })
    }

    fn set_collaborators_from_proto(
        &mut self,
        messages: Vec<proto::Collaborator>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let mut collaborators = HashMap::default();
        for message in messages {
            let collaborator = Collaborator::from_proto(message)?;
            collaborators.insert(collaborator.peer_id, collaborator);
        }
        for old_peer_id in self.collaborators.keys() {
            if !collaborators.contains_key(old_peer_id) {
                cx.emit(Event::CollaboratorLeft(*old_peer_id));
            }
        }
        self.collaborators = collaborators;
        Ok(())
    }
}
