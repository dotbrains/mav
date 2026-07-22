use super::*;

impl Project {
    pub fn create_buffer(
        &mut self,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.create_buffer(language, project_searchable, cx)
        })
    }

    #[inline]
    pub fn create_local_buffer(
        &mut self,
        text: &str,
        language: Option<Arc<Language>>,
        project_searchable: bool,
        cx: &mut Context<Self>,
    ) -> Entity<Buffer> {
        if self.is_remote() {
            panic!("called create_local_buffer on a remote project")
        }
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.create_local_buffer(text, language, project_searchable, cx)
        })
    }

    pub fn open_path(
        &mut self,
        path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Option<ProjectEntryId>, Entity<Buffer>)>> {
        let task = self.open_buffer(path, cx);
        cx.spawn(async move |_project, cx| {
            let buffer = task.await?;
            let project_entry_id = buffer.read_with(cx, |buffer, _cx| {
                File::from_dyn(buffer.file()).and_then(|file| file.project_entry_id())
            });

            Ok((project_entry_id, buffer))
        })
    }

    pub fn open_local_buffer(
        &mut self,
        abs_path: impl AsRef<Path>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        let worktree_task = self.find_or_create_worktree(abs_path.as_ref(), false, cx);
        cx.spawn(async move |this, cx| {
            let (worktree, relative_path) = worktree_task.await?;
            this.update(cx, |this, cx| {
                this.open_buffer((worktree.read(cx).id(), relative_path), cx)
            })?
            .await
        })
    }

    #[cfg(feature = "test-support")]
    pub fn open_local_buffer_with_lsp(
        &mut self,
        abs_path: impl AsRef<Path>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Entity<Buffer>, lsp_store::OpenLspBufferHandle)>> {
        if let Some((worktree, relative_path)) = self.find_worktree(abs_path.as_ref(), cx) {
            self.open_buffer_with_lsp((worktree.read(cx).id(), relative_path), cx)
        } else {
            Task::ready(Err(anyhow!("no such path")))
        }
    }

    pub fn download_file(
        &mut self,
        worktree_id: WorktreeId,
        path: Arc<RelPath>,
        destination_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        log::debug!(
            "download_file called: worktree_id={:?}, path={:?}, destination={:?}",
            worktree_id,
            path,
            destination_path
        );

        let Some(remote_client) = &self.remote_client else {
            log::error!("download_file: not a remote project");
            return Task::ready(Err(anyhow!("not a remote project")));
        };

        let proto_client = remote_client.read(cx).proto_client();
        // For SSH remote projects, use REMOTE_SERVER_PROJECT_ID instead of remote_id()
        // because SSH projects have client_state: Local but still need to communicate with remote server
        let project_id = self.remote_id().unwrap_or(REMOTE_SERVER_PROJECT_ID);
        let downloading_files = self.downloading_files.clone();
        let path_str = path.to_proto();

        static NEXT_FILE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let file_id = NEXT_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        // Register BEFORE sending request to avoid race condition
        let key = (worktree_id, path_str.clone());
        log::debug!(
            "download_file: pre-registering download with key={:?}, file_id={}",
            key,
            file_id
        );
        downloading_files.lock().insert(
            key,
            DownloadingFile {
                destination_path: destination_path,
                chunks: Vec::new(),
                total_size: 0,
                file_id: Some(file_id),
            },
        );
        log::debug!(
            "download_file: sending DownloadFileByPath request, path_str={}",
            path_str
        );

        cx.spawn(async move |_this, _cx| {
            log::debug!("download_file: sending request with file_id={}...", file_id);
            let response = proto_client
                .request(proto::DownloadFileByPath {
                    project_id,
                    worktree_id: worktree_id.to_proto(),
                    path: path_str.clone(),
                    file_id,
                })
                .await?;

            log::debug!("download_file: got response, file_id={}", response.file_id);
            // The file_id is set from the State message, we just confirm the request succeeded
            Ok(())
        })
    }

    #[ztracing::instrument(skip_all)]
    pub fn open_buffer(
        &mut self,
        path: impl Into<ProjectPath>,
        cx: &mut App,
    ) -> Task<Result<Entity<Buffer>>> {
        if self.is_disconnected(cx) {
            return Task::ready(Err(anyhow!(ErrorCode::Disconnected)));
        }

        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.open_buffer(path.into(), cx)
        })
    }

    #[cfg(feature = "test-support")]
    pub fn open_buffer_with_lsp(
        &mut self,
        path: impl Into<ProjectPath>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(Entity<Buffer>, lsp_store::OpenLspBufferHandle)>> {
        let buffer = self.open_buffer(path, cx);
        cx.spawn(async move |this, cx| {
            let buffer = buffer.await?;
            let handle = this.update(cx, |project, cx| {
                project.register_buffer_with_language_servers(&buffer, cx)
            })?;
            Ok((buffer, handle))
        })
    }

    pub fn register_buffer_with_language_servers(
        &self,
        buffer: &Entity<Buffer>,
        cx: &mut App,
    ) -> OpenLspBufferHandle {
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.register_buffer_with_language_servers(buffer, HashSet::default(), false, cx)
        })
    }

    pub fn open_unstaged_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        if self.is_disconnected(cx) {
            return Task::ready(Err(anyhow!(ErrorCode::Disconnected)));
        }
        self.git_store
            .update(cx, |git_store, cx| git_store.open_unstaged_diff(buffer, cx))
    }

    #[ztracing::instrument(skip_all)]
    pub fn open_staged_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        if self.is_disconnected(cx) {
            return Task::ready(Err(anyhow!(ErrorCode::Disconnected)));
        }
        self.git_store
            .update(cx, |git_store, cx| git_store.open_staged_diff(buffer, cx))
    }

    #[ztracing::instrument(skip_all)]
    pub fn open_uncommitted_diff(
        &mut self,
        buffer: Entity<Buffer>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<BufferDiff>>> {
        if self.is_disconnected(cx) {
            return Task::ready(Err(anyhow!(ErrorCode::Disconnected)));
        }
        self.git_store.update(cx, |git_store, cx| {
            git_store.open_uncommitted_diff(buffer, cx)
        })
    }

    pub fn open_buffer_by_id(
        &mut self,
        id: BufferId,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Buffer>>> {
        if let Some(buffer) = self.buffer_for_id(id, cx) {
            Task::ready(Ok(buffer))
        } else if self.is_local() || self.is_via_remote_server() {
            Task::ready(Err(anyhow!("buffer {id} does not exist")))
        } else if let Some(project_id) = self.remote_id() {
            let request = self.collab_client.request(proto::OpenBufferById {
                project_id,
                id: id.into(),
            });
            cx.spawn(async move |project, cx| {
                let buffer_id = BufferId::new(request.await?.buffer_id)?;
                project
                    .update(cx, |project, cx| {
                        project.buffer_store.update(cx, |buffer_store, cx| {
                            buffer_store.wait_for_remote_buffer(buffer_id, cx)
                        })
                    })?
                    .await
            })
        } else {
            Task::ready(Err(anyhow!("cannot open buffer while disconnected")))
        }
    }

    pub fn save_buffers(
        &self,
        buffers: HashSet<Entity<Buffer>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        cx.spawn(async move |this, cx| {
            let save_tasks = buffers.into_iter().filter_map(|buffer| {
                this.update(cx, |this, cx| this.save_buffer(buffer, cx))
                    .ok()
            });
            try_join_all(save_tasks).await?;
            Ok(())
        })
    }

    pub fn save_buffer(&self, buffer: Entity<Buffer>, cx: &mut Context<Self>) -> Task<Result<()>> {
        self.buffer_store
            .update(cx, |buffer_store, cx| buffer_store.save_buffer(buffer, cx))
    }

    pub fn save_buffer_as(
        &mut self,
        buffer: Entity<Buffer>,
        path: ProjectPath,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.save_buffer_as(buffer.clone(), path, cx)
        })
    }

    pub fn get_open_buffer(&self, path: &ProjectPath, cx: &App) -> Option<Entity<Buffer>> {
        self.buffer_store.read(cx).get_by_path(path)
    }

    fn register_buffer(&mut self, buffer: &Entity<Buffer>, cx: &mut Context<Self>) -> Result<()> {
        {
            let mut remotely_created_models = self.remotely_created_models.lock();
            if remotely_created_models.retain_count > 0 {
                remotely_created_models.buffers.push(buffer.clone())
            }
        }

        self.request_buffer_diff_recalculation(buffer, cx);

        cx.subscribe(buffer, |this, buffer, event, cx| {
            this.on_buffer_event(buffer, event, cx);
        })
        .detach();

        Ok(())
    }

    pub fn open_image(
        &mut self,
        path: impl Into<ProjectPath>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<ImageItem>>> {
        if self.is_disconnected(cx) {
            return Task::ready(Err(anyhow!(ErrorCode::Disconnected)));
        }

        let open_image_task = self.image_store.update(cx, |image_store, cx| {
            image_store.open_image(path.into(), cx)
        });

        let weak_project = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let image_item = open_image_task.await?;

            // Check if metadata already exists (e.g., for remote images)
            let needs_metadata =
                cx.read_entity(&image_item, |item, _| item.image_metadata.is_none());

            if needs_metadata {
                let project = weak_project.upgrade().context("Project dropped")?;
                let metadata =
                    ImageItem::load_image_metadata(image_item.clone(), project, cx).await?;
                image_item.update(cx, |image_item, cx| {
                    image_item.image_metadata = Some(metadata);
                    cx.emit(ImageItemEvent::MetadataUpdated);
                });
            }

            Ok(image_item)
        })
    }

    async fn send_buffer_ordered_messages(
        project: WeakEntity<Self>,
        rx: UnboundedReceiver<BufferOrderedMessage>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        const MAX_BATCH_SIZE: usize = 128;

        let mut operations_by_buffer_id = HashMap::default();
        async fn flush_operations(
            this: &WeakEntity<Project>,
            operations_by_buffer_id: &mut HashMap<BufferId, Vec<proto::Operation>>,
            needs_resync_with_host: &mut bool,
            is_local: bool,
            cx: &mut AsyncApp,
        ) -> Result<()> {
            for (buffer_id, operations) in operations_by_buffer_id.drain() {
                let request = this.read_with(cx, |this, _| {
                    let project_id = this.remote_id()?;
                    Some(this.collab_client.request(proto::UpdateBuffer {
                        buffer_id: buffer_id.into(),
                        project_id,
                        operations,
                    }))
                })?;
                if let Some(request) = request
                    && request.await.is_err()
                    && !is_local
                {
                    *needs_resync_with_host = true;
                    break;
                }
            }
            Ok(())
        }

        let mut needs_resync_with_host = false;
        let mut changes = rx.ready_chunks(MAX_BATCH_SIZE);

        while let Some(changes) = changes.next().await {
            let is_local = project.read_with(cx, |this, _| this.is_local())?;

            for change in changes {
                match change {
                    BufferOrderedMessage::Operation {
                        buffer_id,
                        operation,
                    } => {
                        if needs_resync_with_host {
                            continue;
                        }

                        operations_by_buffer_id
                            .entry(buffer_id)
                            .or_insert(Vec::new())
                            .push(operation);
                    }

                    BufferOrderedMessage::Resync => {
                        operations_by_buffer_id.clear();
                        if project
                            .update(cx, |this, cx| this.synchronize_remote_buffers(cx))?
                            .await
                            .is_ok()
                        {
                            needs_resync_with_host = false;
                        }
                    }

                    BufferOrderedMessage::LanguageServerUpdate {
                        language_server_id,
                        message,
                        name,
                    } => {
                        flush_operations(
                            &project,
                            &mut operations_by_buffer_id,
                            &mut needs_resync_with_host,
                            is_local,
                            cx,
                        )
                        .await?;

                        project.read_with(cx, |project, _| {
                            if let Some(project_id) = project.remote_id() {
                                project
                                    .collab_client
                                    .send(proto::UpdateLanguageServer {
                                        project_id,
                                        server_name: name.map(|name| String::from(name.0)),
                                        language_server_id: language_server_id.to_proto(),
                                        variant: Some(message),
                                    })
                                    .log_err();
                            }
                        })?;
                    }
                }
            }

            flush_operations(
                &project,
                &mut operations_by_buffer_id,
                &mut needs_resync_with_host,
                is_local,
                cx,
            )
            .await?;
        }

        Ok(())
    }
}
