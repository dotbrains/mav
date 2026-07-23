use super::*;

impl LocalBufferStore {
    pub(super) fn save_local_buffer(
        &self,
        buffer_handle: Entity<Buffer>,
        worktree: Entity<Worktree>,
        path: Arc<RelPath>,
        mut has_changed_file: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<()>> {
        let buffer = buffer_handle.read(cx);

        let text = buffer.as_rope().clone();
        let line_ending = buffer.line_ending();
        let encoding = buffer.encoding();
        let has_bom = buffer.has_bom();
        let version = buffer.version();
        let buffer_id = buffer.remote_id();
        let file = buffer.file().cloned();
        if file
            .as_ref()
            .is_some_and(|file| file.disk_state() == DiskState::New)
        {
            has_changed_file = true;
        }

        let save = worktree.update(cx, |worktree, cx| {
            worktree.write_file(path, text, line_ending, encoding, has_bom, cx)
        });

        cx.spawn(async move |this, cx| {
            let new_file = save.await?;
            let mtime = new_file.disk_state().mtime();
            this.update(cx, |this, cx| {
                if let Some((downstream_client, project_id)) = this.downstream_client.clone() {
                    if has_changed_file {
                        downstream_client
                            .send(proto::UpdateBufferFile {
                                project_id,
                                buffer_id: buffer_id.to_proto(),
                                file: Some(language::File::to_proto(&*new_file, cx)),
                            })
                            .log_err();
                    }
                    downstream_client
                        .send(proto::BufferSaved {
                            project_id,
                            buffer_id: buffer_id.to_proto(),
                            version: serialize_version(&version),
                            mtime: mtime.map(|time| time.into()),
                        })
                        .log_err();
                }
            })?;
            buffer_handle.update(cx, |buffer, cx| {
                if has_changed_file {
                    buffer.file_updated(new_file, cx);
                }
                buffer.did_save(version.clone(), mtime, cx);
            });
            Ok(())
        })
    }

    pub(super) fn subscribe_to_worktree(
        &mut self,
        worktree: &Entity<Worktree>,
        cx: &mut Context<BufferStore>,
    ) {
        cx.subscribe(worktree, |this, worktree, event, cx| {
            if worktree.read(cx).is_local()
                && let worktree::Event::UpdatedEntries(changes) = event
            {
                Self::local_worktree_entries_changed(this, &worktree, changes, cx);
            }
        })
        .detach();
    }

    pub(super) fn local_worktree_entries_changed(
        this: &mut BufferStore,
        worktree_handle: &Entity<Worktree>,
        changes: &[(Arc<RelPath>, ProjectEntryId, PathChange)],
        cx: &mut Context<BufferStore>,
    ) {
        let snapshot = worktree_handle.read(cx).snapshot();
        for (path, entry_id, _) in changes {
            Self::local_worktree_entry_changed(
                this,
                *entry_id,
                path,
                worktree_handle,
                &snapshot,
                cx,
            );
        }
    }

    pub(super) fn local_worktree_entry_changed(
        this: &mut BufferStore,
        entry_id: ProjectEntryId,
        path: &Arc<RelPath>,
        worktree: &Entity<worktree::Worktree>,
        snapshot: &worktree::Snapshot,
        cx: &mut Context<BufferStore>,
    ) -> Option<()> {
        let project_path = ProjectPath {
            worktree_id: snapshot.id(),
            path: path.clone(),
        };

        let buffer_id = this
            .as_local_mut()
            .and_then(|local| local.local_buffer_ids_by_entry_id.get(&entry_id))
            .copied()
            .or_else(|| this.path_to_buffer_id.get(&project_path).copied())?;

        let buffer = if let Some(buffer) = this.get(buffer_id) {
            Some(buffer)
        } else {
            this.opened_buffers.remove(&buffer_id);
            this.non_searchable_buffers.remove(&buffer_id);
            None
        };

        let buffer = if let Some(buffer) = buffer {
            buffer
        } else {
            this.path_to_buffer_id.remove(&project_path);
            let this = this.as_local_mut()?;
            this.local_buffer_ids_by_entry_id.remove(&entry_id);
            return None;
        };

        let events = buffer.update(cx, |buffer, cx| {
            let file = buffer.file()?;
            let old_file = File::from_dyn(Some(file))?;
            if old_file.worktree != *worktree {
                return None;
            }

            let snapshot_entry = old_file
                .entry_id
                .and_then(|entry_id| snapshot.entry_for_id(entry_id))
                .or_else(|| snapshot.entry_for_path(old_file.path.as_ref()));

            let new_file = if let Some(entry) = snapshot_entry {
                File {
                    disk_state: match entry.mtime {
                        Some(mtime) => DiskState::Present {
                            mtime,
                            size: entry.size,
                        },
                        None => old_file.disk_state,
                    },
                    is_local: true,
                    entry_id: Some(entry.id),
                    path: entry.path.clone(),
                    worktree: worktree.clone(),
                    is_private: entry.is_private,
                }
            } else {
                File {
                    disk_state: DiskState::Deleted,
                    is_local: true,
                    entry_id: old_file.entry_id,
                    path: old_file.path.clone(),
                    worktree: worktree.clone(),
                    is_private: old_file.is_private,
                }
            };

            if new_file == *old_file {
                return None;
            }

            let mut events = Vec::new();
            if new_file.path != old_file.path {
                this.path_to_buffer_id.remove(&ProjectPath {
                    path: old_file.path.clone(),
                    worktree_id: old_file.worktree_id(cx),
                });
                this.path_to_buffer_id.insert(
                    ProjectPath {
                        worktree_id: new_file.worktree_id(cx),
                        path: new_file.path.clone(),
                    },
                    buffer_id,
                );
                events.push(BufferStoreEvent::BufferChangedFilePath {
                    buffer: cx.entity(),
                    old_file: buffer.file().cloned(),
                });
            }
            let local = this.as_local_mut()?;
            if new_file.entry_id != old_file.entry_id {
                if let Some(entry_id) = old_file.entry_id {
                    local.local_buffer_ids_by_entry_id.remove(&entry_id);
                }
                if let Some(entry_id) = new_file.entry_id {
                    local
                        .local_buffer_ids_by_entry_id
                        .insert(entry_id, buffer_id);
                }
            }

            if let Some((client, project_id)) = &this.downstream_client {
                client
                    .send(proto::UpdateBufferFile {
                        project_id: *project_id,
                        buffer_id: buffer_id.to_proto(),
                        file: Some(new_file.to_proto(cx)),
                    })
                    .ok();
            }

            buffer.file_updated(Arc::new(new_file), cx);
            Some(events)
        })?;

        for event in events {
            cx.emit(event);
        }

        None
    }

    pub(super) fn save_buffer(
        &self,
        buffer: Entity<Buffer>,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<()>> {
        let Some(file) = File::from_dyn(buffer.read(cx).file()) else {
            return Task::ready(Err(anyhow!("buffer doesn't have a file")));
        };
        let worktree = file.worktree.clone();
        self.save_local_buffer(buffer, worktree, file.path.clone(), false, cx)
    }

    pub(super) fn save_buffer_as(
        &self,
        buffer: Entity<Buffer>,
        path: ProjectPath,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<()>> {
        let Some(worktree) = self
            .worktree_store
            .read(cx)
            .worktree_for_id(path.worktree_id, cx)
        else {
            return Task::ready(Err(anyhow!("no such worktree")));
        };
        self.save_local_buffer(buffer, worktree, path.path, true, cx)
    }

    #[ztracing::instrument(skip_all)]
    pub(super) fn open_buffer(
        &self,
        path: Arc<RelPath>,
        worktree: Entity<Worktree>,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        let load_file = worktree.update(cx, |worktree, cx| worktree.load_file(path.as_ref(), cx));
        cx.spawn(async move |this, cx| {
            let path = path.clone();
            let buffer = match load_file.await {
                Ok(loaded) => {
                    let reservation = cx.reserve_entity::<Buffer>();
                    let buffer_id = BufferId::from(reservation.entity_id().as_non_zero_u64());
                    let text_buffer = cx
                        .background_spawn(async move {
                            text::Buffer::new(ReplicaId::LOCAL, buffer_id, loaded.text)
                        })
                        .await;
                    cx.insert_entity(reservation, |_| {
                        let mut buffer =
                            Buffer::build(text_buffer, Some(loaded.file), Capability::ReadWrite);
                        buffer.set_encoding(loaded.encoding);
                        buffer.set_has_bom(loaded.has_bom);
                        buffer
                    })
                }
                Err(error) if is_not_found_error(&error) => cx.new(|cx| {
                    let buffer_id = BufferId::from(cx.entity_id().as_non_zero_u64());
                    let text_buffer = text::Buffer::new(ReplicaId::LOCAL, buffer_id, "");
                    let mut buffer = Buffer::build(
                        text_buffer,
                        Some(Arc::new(File {
                            worktree,
                            path,
                            disk_state: DiskState::New,
                            entry_id: None,
                            is_local: true,
                            is_private: false,
                        })),
                        Capability::ReadWrite,
                    );
                    apply_initial_line_ending(&mut buffer, cx);
                    buffer
                }),
                Err(e) => return Err(e),
            };
            this.update(cx, |this, cx| {
                this.add_buffer(buffer.clone(), cx)?;
                let buffer_id = buffer.read(cx).remote_id();
                if let Some(file) = File::from_dyn(buffer.read(cx).file()) {
                    let project_path = ProjectPath {
                        worktree_id: file.worktree_id(cx),
                        path: file.path.clone(),
                    };
                    let entry_id = file.entry_id;

                    // Check if the file should be read-only based on settings
                    let settings = WorktreeSettings::get(Some((&project_path).into()), cx);
                    let is_read_only = if project_path.path.is_empty() {
                        settings.is_std_path_read_only(&file.full_path(cx))
                    } else {
                        settings.is_path_read_only(&project_path.path)
                    };
                    if is_read_only {
                        buffer.update(cx, |buffer, cx| {
                            buffer.set_capability(Capability::Read, cx);
                        });
                    }

                    this.path_to_buffer_id.insert(project_path, buffer_id);
                    let this = this.as_local_mut().unwrap();
                    if let Some(entry_id) = entry_id {
                        this.local_buffer_ids_by_entry_id
                            .insert(entry_id, buffer_id);
                    }
                }

                anyhow::Ok(())
            })??;

            Ok(buffer)
        })
    }

    pub(super) fn create_buffer(
        &self,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<Entity<Buffer>>> {
        cx.spawn(async move |buffer_store, cx| {
            let buffer = cx.new(|cx| {
                let mut buffer = Buffer::local("", cx)
                    .with_language(language.unwrap_or_else(|| language::PLAIN_TEXT.clone()), cx);
                apply_initial_line_ending(&mut buffer, cx);
                buffer
            });
            buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.add_buffer(buffer.clone(), cx).log_err();
                if !project_searchable {
                    buffer_store
                        .non_searchable_buffers
                        .insert(buffer.read(cx).remote_id());
                }
            })?;
            Ok(buffer)
        })
    }

    pub(super) fn reload_buffers(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        push_to_history: bool,
        cx: &mut Context<BufferStore>,
    ) -> Task<Result<ProjectTransaction>> {
        cx.spawn(async move |_, cx| {
            let mut project_transaction = ProjectTransaction::default();
            for buffer in buffers {
                let transaction = buffer.update(cx, |buffer, cx| buffer.reload(cx)).await?;
                buffer.update(cx, |buffer, cx| {
                    if let Some(transaction) = transaction {
                        if !push_to_history {
                            buffer.forget_transaction(transaction.id);
                        }
                        project_transaction.0.insert(cx.entity(), transaction);
                    }
                });
            }

            Ok(project_transaction)
        })
    }
}
