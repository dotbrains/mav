use super::*;

impl BufferStore {
    pub fn init(client: &AnyProtoClient) {
        client.add_entity_message_handler(Self::handle_buffer_reloaded);
        client.add_entity_message_handler(Self::handle_buffer_saved);
        client.add_entity_message_handler(Self::handle_update_buffer_file);
        client.add_entity_request_handler(Self::handle_save_buffer);
        client.add_entity_request_handler(Self::handle_reload_buffers);
    }

    /// Creates a buffer store, optionally retaining its buffers.
    pub fn local(worktree_store: Entity<WorktreeStore>, cx: &mut Context<Self>) -> Self {
        Self {
            state: BufferStoreState::Local(LocalBufferStore {
                local_buffer_ids_by_entry_id: Default::default(),
                worktree_store: worktree_store.clone(),
                _subscription: cx.subscribe(&worktree_store, |this, _, event, cx| {
                    if let WorktreeStoreEvent::WorktreeAdded(worktree) = event {
                        let this = this.as_local_mut().unwrap();
                        this.subscribe_to_worktree(worktree, cx);
                    }
                }),
            }),
            downstream_client: None,
            opened_buffers: Default::default(),
            path_to_buffer_id: Default::default(),
            shared_buffers: Default::default(),
            loading_buffers: Default::default(),
            non_searchable_buffers: Default::default(),
            worktree_store,
            project_search: Default::default(),
        }
    }

    pub fn remote(
        worktree_store: Entity<WorktreeStore>,
        upstream_client: AnyProtoClient,
        remote_id: u64,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            state: BufferStoreState::Remote(RemoteBufferStore {
                shared_with_me: Default::default(),
                loading_remote_buffers_by_id: Default::default(),
                remote_buffer_listeners: Default::default(),
                project_id: remote_id,
                upstream_client,
                worktree_store: worktree_store.clone(),
            }),
            downstream_client: None,
            opened_buffers: Default::default(),
            path_to_buffer_id: Default::default(),
            loading_buffers: Default::default(),
            shared_buffers: Default::default(),
            non_searchable_buffers: Default::default(),
            worktree_store,
            project_search: Default::default(),
        }
    }

    pub(super) fn as_local_mut(&mut self) -> Option<&mut LocalBufferStore> {
        match &mut self.state {
            BufferStoreState::Local(state) => Some(state),
            _ => None,
        }
    }

    pub(super) fn as_remote_mut(&mut self) -> Option<&mut RemoteBufferStore> {
        match &mut self.state {
            BufferStoreState::Remote(state) => Some(state),
            _ => None,
        }
    }

    pub(super) fn as_remote(&self) -> Option<&RemoteBufferStore> {
        match &self.state {
            BufferStoreState::Remote(state) => Some(state),
            _ => None,
        }
    }

    #[ztracing::instrument(skip_all)]
    pub fn open_buffer(
        &mut self,
        project_path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        if let Some(buffer) = self.get_by_path(&project_path) {
            return Task::ready(Ok(buffer));
        }

        let task = match self.loading_buffers.entry(project_path.clone()) {
            hash_map::Entry::Occupied(e) => e.get().clone(),
            hash_map::Entry::Vacant(entry) => {
                let path = project_path.path.clone();
                let Some(worktree) = self
                    .worktree_store
                    .read(cx)
                    .worktree_for_id(project_path.worktree_id, cx)
                else {
                    return Task::ready(Err(anyhow!("no such worktree")));
                };
                let load_buffer = match &self.state {
                    BufferStoreState::Local(this) => this.open_buffer(path, worktree, cx),
                    BufferStoreState::Remote(this) => this.open_buffer(path, worktree, cx),
                };

                entry
                    .insert(
                        cx.spawn(async move |this, cx| {
                            let load_result = load_buffer.await;
                            this.update(cx, |this, _cx| {
                                // Record the fact that the buffer is no longer loading.
                                this.loading_buffers.remove(&project_path);

                                let buffer = load_result.map_err(Arc::new)?;
                                Ok(buffer)
                            })?
                        })
                        .shared(),
                    )
                    .clone()
            }
        };

        cx.background_spawn(async move {
            task.await.map_err(|e| {
                if e.error_code() != ErrorCode::Internal {
                    anyhow!(e.error_code())
                } else {
                    anyhow!("{e}")
                }
            })
        })
    }

    pub fn create_buffer(
        &mut self,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        match &self.state {
            BufferStoreState::Local(this) => this.create_buffer(language, project_searchable, cx),
            BufferStoreState::Remote(this) => this.create_buffer(language, project_searchable, cx),
        }
    }

    pub fn save_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        match &mut self.state {
            BufferStoreState::Local(this) => this.save_buffer(buffer, cx),
            BufferStoreState::Remote(this) => this.save_remote_buffer(buffer, None, cx),
        }
    }

    pub fn save_buffer_as(
        &mut self,
        buffer: Entity<Buffer>,
        path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let old_file = buffer.read(cx).file().cloned();
        let task = match &self.state {
            BufferStoreState::Local(this) => this.save_buffer_as(buffer.clone(), path, cx),
            BufferStoreState::Remote(this) => {
                this.save_remote_buffer(buffer.clone(), Some(path.to_proto()), cx)
            }
        };
        cx.spawn(async move |this, cx| {
            task.await?;
            this.update(cx, |this, cx| {
                old_file.clone().and_then(|file| {
                    this.path_to_buffer_id.remove(&ProjectPath {
                        worktree_id: file.worktree_id(cx),
                        path: file.path().clone(),
                    })
                });

                cx.emit(BufferStoreEvent::BufferChangedFilePath { buffer, old_file });
            })
        })
    }

    pub(super) fn add_buffer(
        &mut self,
        buffer_entity: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let buffer = buffer_entity.read(cx);
        let remote_id = buffer.remote_id();
        let path = File::from_dyn(buffer.file()).map(|file| ProjectPath {
            path: file.path.clone(),
            worktree_id: file.worktree_id(cx),
        });
        let is_remote = buffer.replica_id().is_remote();
        let open_buffer = OpenBuffer::Complete {
            buffer: buffer_entity.downgrade(),
        };

        let handle = cx.entity().downgrade();
        buffer_entity.update(cx, move |_, cx| {
            cx.on_release(move |buffer, cx| {
                handle
                    .update(cx, |_, cx| {
                        cx.emit(BufferStoreEvent::BufferDropped(buffer.remote_id()))
                    })
                    .ok();
            })
            .detach()
        });
        let _expect_path_to_exist;
        match self.opened_buffers.entry(remote_id) {
            hash_map::Entry::Vacant(entry) => {
                entry.insert(open_buffer);
                _expect_path_to_exist = false;
            }
            hash_map::Entry::Occupied(mut entry) => {
                if let OpenBuffer::Operations(operations) = entry.get_mut() {
                    buffer_entity.update(cx, |b, cx| b.apply_ops(operations.drain(..), cx));
                } else if entry.get().upgrade().is_some() {
                    if is_remote {
                        return Ok(());
                    } else {
                        debug_panic!("buffer {remote_id} was already registered");
                        anyhow::bail!("buffer {remote_id} was already registered");
                    }
                }
                entry.insert(open_buffer);
                _expect_path_to_exist = true;
            }
        }

        if let Some(path) = path {
            self.path_to_buffer_id.insert(path, remote_id);
        }

        cx.subscribe(&buffer_entity, Self::on_buffer_event).detach();
        cx.emit(BufferStoreEvent::BufferAdded(buffer_entity));
        Ok(())
    }

    pub fn buffers(&self) -> impl '_ + Iterator<Item = Entity<Buffer>> {
        self.opened_buffers
            .values()
            .filter_map(|buffer| buffer.upgrade())
    }

    pub(crate) fn is_searchable(&self, id: &BufferId) -> bool {
        !self.non_searchable_buffers.contains(&id)
    }

    pub fn loading_buffers(
        &self,
    ) -> impl Iterator<Item = (&ProjectPath, impl Future<Output = Result<Entity<Buffer>>>)> {
        self.loading_buffers.iter().map(|(path, task)| {
            let task = task.clone();
            (path, async move {
                task.await.map_err(|e| {
                    if e.error_code() != ErrorCode::Internal {
                        anyhow!(e.error_code())
                    } else {
                        anyhow!("{e}")
                    }
                })
            })
        })
    }

    pub fn buffer_id_for_project_path(&self, project_path: &ProjectPath) -> Option<&BufferId> {
        self.path_to_buffer_id.get(project_path)
    }

    pub fn get_by_path(&self, path: &ProjectPath) -> Option<Entity<Buffer>> {
        self.path_to_buffer_id
            .get(path)
            .and_then(|buffer_id| self.get(*buffer_id))
    }

    pub fn get(&self, buffer_id: BufferId) -> Option<Entity<Buffer>> {
        self.opened_buffers.get(&buffer_id)?.upgrade()
    }

    pub fn get_existing(&self, buffer_id: BufferId) -> Result<Entity<Buffer>> {
        self.get(buffer_id)
            .with_context(|| format!("unknown buffer id {buffer_id}"))
    }

    pub fn get_possibly_incomplete(&self, buffer_id: BufferId) -> Option<Entity<Buffer>> {
        self.get(buffer_id).or_else(|| {
            self.as_remote()
                .and_then(|remote| remote.loading_remote_buffers_by_id.get(&buffer_id).cloned())
        })
    }

    pub fn buffer_version_info(&self, cx: &App) -> (Vec<proto::BufferVersion>, Vec<BufferId>) {
        let buffers = self
            .buffers()
            .map(|buffer| {
                let buffer = buffer.read(cx);
                proto::BufferVersion {
                    id: buffer.remote_id().into(),
                    version: language::proto::serialize_version(&buffer.version),
                }
            })
            .collect();
        let incomplete_buffer_ids = self
            .as_remote()
            .map(|remote| remote.incomplete_buffer_ids())
            .unwrap_or_default();
        (buffers, incomplete_buffer_ids)
    }

    pub fn disconnected_from_host(&mut self, cx: &mut App) {
        for open_buffer in self.opened_buffers.values_mut() {
            if let Some(buffer) = open_buffer.upgrade() {
                buffer.update(cx, |buffer, _| buffer.give_up_waiting());
            }
        }

        for buffer in self.buffers() {
            buffer.update(cx, |buffer, cx| {
                buffer.set_capability(Capability::ReadOnly, cx)
            });
        }

        if let Some(remote) = self.as_remote_mut() {
            // Wake up all futures currently waiting on a buffer to get opened,
            // to give them a chance to fail now that we've disconnected.
            remote.remote_buffer_listeners.clear()
        }
    }

    pub fn shared(&mut self, remote_id: u64, downstream_client: AnyProtoClient, _cx: &mut App) {
        self.downstream_client = Some((downstream_client, remote_id));
    }

    pub fn unshared(&mut self, _cx: &mut Context<Self>) {
        self.downstream_client.take();
        self.forget_shared_buffers();
    }

    pub fn discard_incomplete(&mut self) {
        self.opened_buffers
            .retain(|_, buffer| !matches!(buffer, OpenBuffer::Operations(_)));
    }

    pub(super) fn buffer_changed_file(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut App,
    ) -> Option<()> {
        let file = File::from_dyn(buffer.read(cx).file())?;

        let remote_id = buffer.read(cx).remote_id();
        if let Some(entry_id) = file.entry_id {
            if let Some(local) = self.as_local_mut() {
                match local.local_buffer_ids_by_entry_id.get(&entry_id) {
                    Some(_) => {
                        return None;
                    }
                    None => {
                        local
                            .local_buffer_ids_by_entry_id
                            .insert(entry_id, remote_id);
                    }
                }
            }
            self.path_to_buffer_id.insert(
                ProjectPath {
                    worktree_id: file.worktree_id(cx),
                    path: file.path.clone(),
                },
                remote_id,
            );
        };

        Some(())
    }

    pub(super) fn on_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &BufferEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferEvent::FileHandleChanged => {
                self.buffer_changed_file(buffer, cx);
            }
            BufferEvent::Reloaded => {
                let Some((downstream_client, project_id)) = self.downstream_client.as_ref() else {
                    return;
                };
                let buffer = buffer.read(cx);
                downstream_client
                    .send(proto::BufferReloaded {
                        project_id: *project_id,
                        buffer_id: buffer.remote_id().to_proto(),
                        version: serialize_version(&buffer.version()),
                        mtime: buffer.saved_mtime().map(|t| t.into()),
                        line_ending: serialize_line_ending(buffer.line_ending()) as i32,
                    })
                    .log_err();
            }
            BufferEvent::LanguageChanged(_) => {}
            _ => {}
        }
    }
}
