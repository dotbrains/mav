use super::*;

impl Project {
    pub(super) fn release(&mut self, cx: &mut App) {
        if let Some(client) = self.remote_client.take() {
            let shutdown = client.update(cx, |client, cx| {
                client.shutdown_processes(
                    Some(proto::ShutdownRemoteServer {}),
                    cx.background_executor().clone(),
                )
            });

            cx.background_spawn(async move {
                if let Some(shutdown) = shutdown {
                    shutdown.await;
                }
            })
            .detach()
        }

        match &self.client_state {
            ProjectClientState::Local => {}
            ProjectClientState::Shared { .. } => {
                let _ = self.unshare_internal(cx);
            }
            ProjectClientState::Collab { remote_id, .. } => {
                let _ = self.collab_client.send(proto::LeaveProject {
                    project_id: *remote_id,
                });
                self.disconnected_from_host_internal(cx);
            }
        }
    }

    pub fn shared(&mut self, project_id: u64, cx: &mut Context<Self>) -> Result<()> {
        anyhow::ensure!(
            matches!(self.client_state, ProjectClientState::Local),
            "project was already shared"
        );

        self.client_subscriptions.extend([
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&cx.entity(), &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.worktree_store, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.buffer_store, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.lsp_store, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.settings_observer, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.dap_store, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.breakpoint_store, &cx.to_async()),
            self.collab_client
                .subscribe_to_entity(project_id)?
                .set_entity(&self.git_store, &cx.to_async()),
        ]);

        self.buffer_store.update(cx, |buffer_store, cx| {
            buffer_store.shared(project_id, self.collab_client.clone().into(), cx)
        });
        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.shared(project_id, self.collab_client.clone().into(), cx);
        });
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.shared(project_id, self.collab_client.clone().into(), cx)
        });
        self.breakpoint_store.update(cx, |breakpoint_store, _| {
            breakpoint_store.shared(project_id, self.collab_client.clone().into())
        });
        self.dap_store.update(cx, |dap_store, cx| {
            dap_store.shared(project_id, self.collab_client.clone().into(), cx);
        });
        self.task_store.update(cx, |task_store, cx| {
            task_store.shared(project_id, self.collab_client.clone().into(), cx);
        });
        self.settings_observer.update(cx, |settings_observer, cx| {
            settings_observer.shared(project_id, self.collab_client.clone().into(), cx)
        });
        self.git_store.update(cx, |git_store, cx| {
            git_store.shared(project_id, self.collab_client.clone().into(), cx)
        });

        self.client_state = ProjectClientState::Shared {
            remote_id: project_id,
        };

        cx.emit(Event::RemoteIdChanged(Some(project_id)));
        Ok(())
    }

    pub fn reshared(
        &mut self,
        message: proto::ResharedProject,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.buffer_store
            .update(cx, |buffer_store, _| buffer_store.forget_shared_buffers());
        self.set_collaborators_from_proto(message.collaborators, cx)?;

        self.worktree_store.update(cx, |worktree_store, cx| {
            worktree_store.send_project_updates(cx);
        });
        if let Some(remote_id) = self.remote_id() {
            self.git_store.update(cx, |git_store, cx| {
                git_store.shared(remote_id, self.collab_client.clone().into(), cx)
            });
        }
        cx.emit(Event::Reshared);
        Ok(())
    }

    pub fn rejoined(
        &mut self,
        message: proto::RejoinedProject,
        message_id: u32,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        cx.update_global::<SettingsStore, _>(|store, cx| {
            for worktree_metadata in &message.worktrees {
                store
                    .clear_local_settings(WorktreeId::from_proto(worktree_metadata.id), cx)
                    .log_err();
            }
        });

        self.join_project_response_message_id = message_id;
        self.set_worktrees_from_proto(message.worktrees, cx)?;
        self.set_collaborators_from_proto(message.collaborators, cx)?;

        let project = cx.weak_entity();
        self.lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.set_language_server_statuses_from_proto(
                project,
                message.language_servers,
                message.language_server_capabilities,
                cx,
            )
        });
        self.enqueue_buffer_ordered_message(BufferOrderedMessage::Resync)
            .unwrap();
        cx.emit(Event::Rejoined);
        Ok(())
    }

    #[inline]
    pub fn unshare(&mut self, cx: &mut Context<Self>) -> Result<()> {
        self.unshare_internal(cx)?;
        cx.emit(Event::RemoteIdChanged(None));
        Ok(())
    }

    fn unshare_internal(&mut self, cx: &mut App) -> Result<()> {
        anyhow::ensure!(
            !self.is_via_collab(),
            "attempted to unshare a remote project"
        );

        if let ProjectClientState::Shared { remote_id, .. } = self.client_state {
            self.client_state = ProjectClientState::Local;
            self.collaborators.clear();
            self.client_subscriptions.clear();
            self.worktree_store.update(cx, |store, cx| {
                store.unshared(cx);
            });
            self.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.forget_shared_buffers();
                buffer_store.unshared(cx)
            });
            self.task_store.update(cx, |task_store, cx| {
                task_store.unshared(cx);
            });
            self.breakpoint_store.update(cx, |breakpoint_store, cx| {
                breakpoint_store.unshared(cx);
            });
            self.dap_store.update(cx, |dap_store, cx| {
                dap_store.unshared(cx);
            });
            self.settings_observer.update(cx, |settings_observer, cx| {
                settings_observer.unshared(cx);
            });
            self.git_store.update(cx, |git_store, cx| {
                git_store.unshared(cx);
            });

            self.collab_client
                .send(proto::UnshareProject {
                    project_id: remote_id,
                })
                .ok();
            Ok(())
        } else {
            anyhow::bail!("attempted to unshare an unshared project");
        }
    }

    pub fn disconnected_from_host(&mut self, cx: &mut Context<Self>) {
        if self.is_disconnected(cx) {
            return;
        }
        self.disconnected_from_host_internal(cx);
        cx.emit(Event::DisconnectedFromHost);
    }

    pub fn set_role(&mut self, role: proto::ChannelRole, cx: &mut Context<Self>) {
        let new_capability =
            if role == proto::ChannelRole::Member || role == proto::ChannelRole::Admin {
                Capability::ReadWrite
            } else {
                Capability::ReadOnly
            };
        if let ProjectClientState::Collab { capability, .. } = &mut self.client_state {
            if *capability == new_capability {
                return;
            }

            *capability = new_capability;
            for buffer in self.opened_buffers(cx) {
                buffer.update(cx, |buffer, cx| buffer.set_capability(new_capability, cx));
            }
        }
    }

    fn disconnected_from_host_internal(&mut self, cx: &mut App) {
        if let ProjectClientState::Collab {
            sharing_has_stopped,
            ..
        } = &mut self.client_state
        {
            *sharing_has_stopped = true;
            self.client_subscriptions.clear();
            self.collaborators.clear();
            self.worktree_store.update(cx, |store, cx| {
                store.disconnected_from_host(cx);
            });
            self.buffer_store.update(cx, |buffer_store, cx| {
                buffer_store.disconnected_from_host(cx)
            });
            self.lsp_store
                .update(cx, |lsp_store, _cx| lsp_store.disconnected_from_host());
        }
    }

    #[inline]
    pub fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(Event::Closed);
    }

    #[inline]
    pub fn is_disconnected(&self, cx: &App) -> bool {
        match &self.client_state {
            ProjectClientState::Collab {
                sharing_has_stopped,
                ..
            } => *sharing_has_stopped,
            ProjectClientState::Local if self.is_via_remote_server() => {
                self.remote_client_is_disconnected(cx)
            }
            _ => false,
        }
    }

    #[inline]
    fn remote_client_is_disconnected(&self, cx: &App) -> bool {
        self.remote_client
            .as_ref()
            .map(|remote| remote.read(cx).is_disconnected())
            .unwrap_or(false)
    }

    #[inline]
    pub fn capability(&self) -> Capability {
        match &self.client_state {
            ProjectClientState::Collab { capability, .. } => *capability,
            ProjectClientState::Shared { .. } | ProjectClientState::Local => Capability::ReadWrite,
        }
    }

    #[inline]
    pub fn is_read_only(&self, cx: &App) -> bool {
        self.is_disconnected(cx) || !self.capability().editable()
    }

    #[inline]
    pub fn is_local(&self) -> bool {
        match &self.client_state {
            ProjectClientState::Local | ProjectClientState::Shared { .. } => {
                self.remote_client.is_none()
            }
            ProjectClientState::Collab { .. } => false,
        }
    }

    /// Whether this project is a remote server (not counting collab).
    #[inline]
    pub fn is_via_remote_server(&self) -> bool {
        match &self.client_state {
            ProjectClientState::Local | ProjectClientState::Shared { .. } => {
                self.remote_client.is_some()
            }
            ProjectClientState::Collab { .. } => false,
        }
    }

    /// Whether this project is from collab (not counting remote servers).
    #[inline]
    pub fn is_via_collab(&self) -> bool {
        match &self.client_state {
            ProjectClientState::Local | ProjectClientState::Shared { .. } => false,
            ProjectClientState::Collab { .. } => true,
        }
    }

    /// `!self.is_local()`
    #[inline]
    pub fn is_remote(&self) -> bool {
        debug_assert_eq!(
            !self.is_local(),
            self.is_via_collab() || self.is_via_remote_server()
        );
        !self.is_local()
    }

    #[inline]
    pub fn is_via_wsl_with_host_interop(&self, cx: &App) -> bool {
        match &self.client_state {
            ProjectClientState::Local | ProjectClientState::Shared { .. } => {
                matches!(
                    &self.remote_client, Some(remote_client)
                    if remote_client.read(cx).has_wsl_interop()
                )
            }
            _ => false,
        }
    }

    pub fn disable_worktree_scanner(&mut self, cx: &mut Context<Self>) {
        self.worktree_store.update(cx, |worktree_store, _cx| {
            worktree_store.disable_scanner();
        });
    }
}
