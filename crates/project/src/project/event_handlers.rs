use super::*;

impl Project {
    pub(super) fn on_buffer_store_event(
        &mut self,
        _: Entity<BufferStore>,
        event: &BufferStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            BufferStoreEvent::BufferAdded(buffer) => {
                self.register_buffer(buffer, cx).log_err();
            }
            BufferStoreEvent::BufferDropped(buffer_id) => {
                if let Some(ref remote_client) = self.remote_client {
                    remote_client
                        .read(cx)
                        .proto_client()
                        .send(proto::CloseBuffer {
                            project_id: 0,
                            buffer_id: buffer_id.to_proto(),
                        })
                        .log_err();
                }
            }
            _ => {}
        }
    }

    pub(super) fn on_image_store_event(
        &mut self,
        _: Entity<ImageStore>,
        event: &ImageStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            ImageStoreEvent::ImageAdded(image) => {
                cx.subscribe(image, |this, image, event, cx| {
                    this.on_image_event(image, event, cx);
                })
                .detach();
            }
        }
    }

    pub(super) fn on_dap_store_event(
        &mut self,
        _: Entity<DapStore>,
        event: &DapStoreEvent,
        cx: &mut Context<Self>,
    ) {
        if let DapStoreEvent::Notification(message) = event {
            cx.emit(Event::Toast {
                notification_id: "dap".into(),
                message: message.clone(),
                link: None,
            });
        }
    }

    pub(super) fn on_lsp_store_event(
        &mut self,
        _: Entity<LspStore>,
        event: &LspStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            LspStoreEvent::DiagnosticsUpdated { server_id, paths } => {
                cx.emit(Event::DiagnosticsUpdated {
                    paths: paths.clone(),
                    language_server_id: *server_id,
                })
            }
            LspStoreEvent::LanguageServerAdded(server_id, name, worktree_id) => cx.emit(
                Event::LanguageServerAdded(*server_id, name.clone(), *worktree_id),
            ),
            LspStoreEvent::LanguageServerRemoved(server_id) => {
                cx.emit(Event::LanguageServerRemoved(*server_id))
            }
            LspStoreEvent::LanguageServerLog(server_id, log_type, string) => cx.emit(
                Event::LanguageServerLog(*server_id, log_type.clone(), string.clone()),
            ),
            LspStoreEvent::LanguageDetected {
                buffer,
                new_language,
            } => {
                let Some(_) = new_language else {
                    cx.emit(Event::LanguageNotFound(buffer.clone()));
                    return;
                };
            }
            LspStoreEvent::RefreshInlayHints {
                server_id,
                request_id,
            } => cx.emit(Event::RefreshInlayHints {
                server_id: *server_id,
                request_id: *request_id,
            }),
            LspStoreEvent::RefreshSemanticTokens {
                server_id,
                request_id,
            } => cx.emit(Event::RefreshSemanticTokens {
                server_id: *server_id,
                request_id: *request_id,
            }),
            LspStoreEvent::RefreshCodeLens => cx.emit(Event::RefreshCodeLens),
            LspStoreEvent::LanguageServerPrompt(prompt) => {
                cx.emit(Event::LanguageServerPrompt(prompt.clone()))
            }
            LspStoreEvent::DiskBasedDiagnosticsStarted { language_server_id } => {
                cx.emit(Event::DiskBasedDiagnosticsStarted {
                    language_server_id: *language_server_id,
                });
            }
            LspStoreEvent::DiskBasedDiagnosticsFinished { language_server_id } => {
                cx.emit(Event::DiskBasedDiagnosticsFinished {
                    language_server_id: *language_server_id,
                });
            }
            LspStoreEvent::LanguageServerUpdate {
                language_server_id,
                name,
                message,
            } => {
                if self.is_local() {
                    self.enqueue_buffer_ordered_message(
                        BufferOrderedMessage::LanguageServerUpdate {
                            language_server_id: *language_server_id,
                            message: message.clone(),
                            name: name.clone(),
                        },
                    )
                    .ok();
                }

                match message {
                    proto::update_language_server::Variant::MetadataUpdated(update) => {
                        self.lsp_store.update(cx, |lsp_store, _| {
                            if let Some(capabilities) = update
                                .capabilities
                                .as_ref()
                                .and_then(|capabilities| serde_json::from_str(capabilities).ok())
                            {
                                lsp_store
                                    .lsp_server_capabilities
                                    .insert(*language_server_id, capabilities);
                            }

                            if let Some(language_server_status) = lsp_store
                                .language_server_statuses
                                .get_mut(language_server_id)
                            {
                                if let Some(binary) = &update.binary {
                                    language_server_status.binary = Some(LanguageServerBinary {
                                        path: PathBuf::from(&binary.path),
                                        arguments: binary
                                            .arguments
                                            .iter()
                                            .map(OsString::from)
                                            .collect(),
                                        env: None,
                                    });
                                }

                                language_server_status.configuration = update
                                    .configuration
                                    .as_ref()
                                    .and_then(|config_str| serde_json::from_str(config_str).ok());

                                language_server_status.workspace_folders = update
                                    .workspace_folders
                                    .iter()
                                    .filter_map(|uri_str| lsp::Uri::from_str(uri_str).ok())
                                    .collect();
                            }
                        });
                    }
                    proto::update_language_server::Variant::RegisteredForBuffer(update) => {
                        if let Some(buffer_id) = BufferId::new(update.buffer_id).ok() {
                            cx.emit(Event::LanguageServerBufferRegistered {
                                buffer_id,
                                server_id: *language_server_id,
                                buffer_abs_path: PathBuf::from(&update.buffer_abs_path),
                                name: name.clone(),
                            });
                        }
                    }
                    _ => (),
                }
            }
            LspStoreEvent::Notification(message) => cx.emit(Event::Toast {
                notification_id: "lsp".into(),
                message: message.clone(),
                link: None,
            }),
            LspStoreEvent::SnippetEdit {
                buffer_id,
                edits,
                most_recent_edit,
            } => {
                if most_recent_edit.replica_id == self.replica_id() {
                    cx.emit(Event::SnippetEdit(*buffer_id, edits.clone()))
                }
            }
            LspStoreEvent::WorkspaceEditApplied(transaction) => {
                cx.emit(Event::WorkspaceEditApplied(transaction.clone()))
            }
        }
    }

    pub(super) fn on_remote_client_event(
        &mut self,
        _: Entity<RemoteClient>,
        event: &remote::RemoteClientEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            &remote::RemoteClientEvent::Disconnected { server_not_running } => {
                self.worktree_store.update(cx, |store, cx| {
                    store.disconnected_from_host(cx);
                });
                self.buffer_store.update(cx, |buffer_store, cx| {
                    buffer_store.disconnected_from_host(cx)
                });
                self.lsp_store.update(cx, |lsp_store, _cx| {
                    lsp_store.disconnected_from_ssh_remote()
                });
                cx.emit(Event::DisconnectedFromRemote { server_not_running });
            }
        }
    }

    pub(super) fn on_settings_observer_event(
        &mut self,
        _: Entity<SettingsObserver>,
        event: &SettingsObserverEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            SettingsObserverEvent::LocalSettingsUpdated(result) => match result {
                Err(InvalidSettingsError::LocalSettings { message, path }) => {
                    let message = format!("Failed to set local settings in {path:?}:\n{message}");
                    cx.emit(Event::Toast {
                        notification_id: format!("local-settings-{path:?}").into(),
                        link: None,
                        message,
                    });
                }
                Ok(path) => cx.emit(Event::HideToast {
                    notification_id: format!("local-settings-{path:?}").into(),
                }),
                Err(_) => {}
            },
            SettingsObserverEvent::LocalTasksUpdated(result) => match result {
                Err(InvalidSettingsError::Tasks { message, path }) => {
                    let message = format!("Failed to set local tasks in {path:?}:\n{message}");
                    cx.emit(Event::Toast {
                        notification_id: format!("local-tasks-{path:?}").into(),
                        link: Some(ToastLink {
                            label: "Open Tasks Documentation",
                            url: "https://mav.dev/docs/tasks",
                        }),
                        message,
                    });
                }
                Ok(path) => cx.emit(Event::HideToast {
                    notification_id: format!("local-tasks-{path:?}").into(),
                }),
                Err(_) => {}
            },
            SettingsObserverEvent::LocalDebugScenariosUpdated(result) => match result {
                Err(InvalidSettingsError::Debug { message, path }) => {
                    let message =
                        format!("Failed to set local debug scenarios in {path:?}:\n{message}");
                    cx.emit(Event::Toast {
                        notification_id: format!("local-debug-scenarios-{path:?}").into(),
                        link: None,
                        message,
                    });
                }
                Ok(path) => cx.emit(Event::HideToast {
                    notification_id: format!("local-debug-scenarios-{path:?}").into(),
                }),
                Err(_) => {}
            },
        }
    }

    pub(super) fn on_worktree_store_event(
        &mut self,
        _: Entity<WorktreeStore>,
        event: &WorktreeStoreEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            WorktreeStoreEvent::WorktreeAdded(worktree) => {
                self.on_worktree_added(worktree, cx);
                cx.emit(Event::WorktreeAdded(worktree.read(cx).id()));
                self.emit_group_key_changed_if_needed(cx);
            }
            WorktreeStoreEvent::WorktreeRemoved(_, id) => {
                cx.emit(Event::WorktreeRemoved(*id));
                self.emit_group_key_changed_if_needed(cx);
            }
            WorktreeStoreEvent::WorktreeReleased(_, id) => {
                self.on_worktree_released(*id, cx);
            }
            WorktreeStoreEvent::WorktreeOrderChanged => cx.emit(Event::WorktreeOrderChanged),
            WorktreeStoreEvent::WorktreeUpdateSent(_) => {}
            WorktreeStoreEvent::WorktreeUpdatedEntries(worktree_id, changes) => {
                self.client()
                    .telemetry()
                    .report_discovered_project_type_events(*worktree_id, changes);
                cx.emit(Event::WorktreeUpdatedEntries(*worktree_id, changes.clone()))
            }
            WorktreeStoreEvent::WorktreeDeletedEntry(worktree_id, id) => {
                cx.emit(Event::DeletedEntry(*worktree_id, *id))
            }
            // Listen to the GitStore instead.
            WorktreeStoreEvent::WorktreeUpdatedGitRepositories(_, _) => {}
            WorktreeStoreEvent::WorktreeUpdatedRootRepoCommonDir(worktree_id) => {
                cx.emit(Event::WorktreeUpdatedRootRepoCommonDir(*worktree_id));
                self.emit_group_key_changed_if_needed(cx);
            }
        }
    }

    fn on_worktree_added(&mut self, worktree: &Entity<Worktree>, _: &mut Context<Self>) {
        let mut remotely_created_models = self.remotely_created_models.lock();
        if remotely_created_models.retain_count > 0 {
            remotely_created_models.worktrees.push(worktree.clone())
        }
    }

    fn on_worktree_released(&mut self, id_to_remove: WorktreeId, cx: &mut Context<Self>) {
        if let Some(remote) = &self.remote_client {
            remote
                .read(cx)
                .proto_client()
                .send(proto::RemoveWorktree {
                    worktree_id: id_to_remove.to_proto(),
                })
                .log_err();
        }
    }

    pub(super) fn on_buffer_event(
        &mut self,
        buffer: Entity<Buffer>,
        event: &BufferEvent,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        if matches!(event, BufferEvent::Edited { .. } | BufferEvent::Reloaded) {
            self.request_buffer_diff_recalculation(&buffer, cx);
        }

        if let BufferEvent::Edited { source } = event {
            cx.emit(Event::BufferEdited { source: *source });
        }

        let buffer_id = buffer.read(cx).remote_id();
        match event {
            BufferEvent::ReloadNeeded => {
                if !self.is_via_collab() {
                    self.reload_buffers([buffer.clone()].into_iter().collect(), true, cx)
                        .detach_and_log_err(cx);
                }
            }
            BufferEvent::Operation {
                operation,
                is_local: true,
            } => {
                let operation = language::proto::serialize_operation(operation);

                if let Some(remote) = &self.remote_client {
                    remote
                        .read(cx)
                        .proto_client()
                        .send(proto::UpdateBuffer {
                            project_id: 0,
                            buffer_id: buffer_id.to_proto(),
                            operations: vec![operation.clone()],
                        })
                        .ok();
                }

                self.enqueue_buffer_ordered_message(BufferOrderedMessage::Operation {
                    buffer_id,
                    operation,
                })
                .ok();
            }

            _ => {}
        }

        None
    }

    fn on_image_event(
        &mut self,
        image: Entity<ImageItem>,
        event: &ImageItemEvent,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        // TODO: handle image events from remote
        if let ImageItemEvent::ReloadNeeded = event
            && !self.is_via_collab()
        {
            self.reload_images([image].into_iter().collect(), cx)
                .detach_and_log_err(cx);
        }

        None
    }

    pub(super) fn request_buffer_diff_recalculation(
        &mut self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Self>,
    ) {
        self.buffers_needing_diff.insert(buffer.downgrade());
        let first_insertion = self.buffers_needing_diff.len() == 1;
        let settings = ProjectSettings::get_global(cx);
        let delay = settings.git.gutter_debounce;

        if delay == 0 {
            if first_insertion {
                let this = cx.weak_entity();
                cx.defer(move |cx| {
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.recalculate_buffer_diffs(cx).detach();
                        });
                    }
                });
            }
            return;
        }

        const MIN_DELAY: u64 = 50;
        let delay = delay.max(MIN_DELAY);
        let duration = Duration::from_millis(delay);

        self.git_diff_debouncer
            .fire_new(duration, cx, move |this, cx| {
                this.recalculate_buffer_diffs(cx)
            });
    }

    fn recalculate_buffer_diffs(&mut self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                let task = this
                    .update(cx, |this, cx| {
                        let buffers = this
                            .buffers_needing_diff
                            .drain()
                            .filter_map(|buffer| buffer.upgrade())
                            .collect::<Vec<_>>();
                        if buffers.is_empty() {
                            None
                        } else {
                            Some(this.git_store.update(cx, |git_store, cx| {
                                git_store.recalculate_buffer_diffs(buffers, cx)
                            }))
                        }
                    })
                    .ok()
                    .flatten();

                if let Some(task) = task {
                    task.await;
                } else {
                    break;
                }
            }
        })
    }
}
