use super::*;

impl Room {
    pub(crate) fn call(
        &mut self,
        called_user_id: u64,
        initial_project_id: Option<u64>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        cx.notify();
        let client = self.client.clone();
        let room_id = self.id;
        self.pending_call_count += 1;
        cx.spawn(async move |this, cx| {
            let result = client
                .request(proto::Call {
                    room_id,
                    called_user_id,
                    initial_project_id,
                })
                .await;
            this.update(cx, |this, cx| {
                this.pending_call_count -= 1;
                if this.should_leave() {
                    this.leave(cx).detach_and_log_err(cx);
                }
            })?;
            result?;
            Ok(())
        })
    }

    pub fn join_project(
        &mut self,
        id: u64,
        language_registry: Arc<LanguageRegistry>,
        fs: Arc<dyn Fs>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Project>>> {
        let client = self.client.clone();
        let user_store = self.user_store.clone();
        cx.emit(Event::RemoteProjectJoined { project_id: id });
        cx.spawn(async move |this, cx| {
            let project =
                Project::in_room(id, client, user_store, language_registry, fs, cx.clone()).await?;

            this.update(cx, |this, cx| {
                this.joined_projects.retain(|project| {
                    if let Some(project) = project.upgrade() {
                        !project.read(cx).is_disconnected(cx)
                    } else {
                        false
                    }
                });
                this.joined_projects.insert(project.downgrade());
            })?;
            Ok(project)
        })
    }

    pub fn share_project(
        &mut self,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Result<u64>> {
        if let Some(project_id) = project.read(cx).remote_id() {
            return Task::ready(Ok(project_id));
        }

        let request = self.client.request(proto::ShareProject {
            room_id: self.id(),
            worktrees: project.read(cx).worktree_metadata_protos(cx),
            is_ssh_project: project.read(cx).is_via_remote_server(),
            windows_paths: Some(project.read(cx).path_style(cx) == PathStyle::Windows),
            features: CURRENT_PROJECT_FEATURES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        });

        cx.spawn(async move |this, cx| {
            let response = request.await?;

            project.update(cx, |project, cx| project.shared(response.project_id, cx))?;

            // If the user's location is in this project, it changes from UnsharedProject to SharedProject.
            this.update(cx, |this, cx| {
                this.shared_projects.insert(project.downgrade());
                let active_project = this.local_participant.active_project.as_ref();
                if active_project.is_some_and(|location| *location == project) {
                    this.set_location(Some(&project), cx)
                } else {
                    Task::ready(Ok(()))
                }
            })?
            .await?;

            Ok(response.project_id)
        })
    }

    pub(crate) fn unshare_project(
        &mut self,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        let project_id = match project.read(cx).remote_id() {
            Some(project_id) => project_id,
            None => return Ok(()),
        };

        self.client.send(proto::UnshareProject { project_id })?;
        project.update(cx, |this, cx| this.unshare(cx))?;

        if self.local_participant.active_project == Some(project.downgrade()) {
            self.set_location(Some(&project), cx).detach_and_log_err(cx);
        }
        Ok(())
    }

    pub(crate) fn set_location(
        &mut self,
        project: Option<&Entity<Project>>,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        if self.status.is_offline() {
            return Task::ready(Err(anyhow!("room is offline")));
        }

        let client = self.client.clone();
        let room_id = self.id;
        let location = if let Some(project) = project {
            self.local_participant.active_project = Some(project.downgrade());
            if let Some(project_id) = project.read(cx).remote_id() {
                proto::participant_location::Variant::SharedProject(
                    proto::participant_location::SharedProject { id: project_id },
                )
            } else {
                proto::participant_location::Variant::UnsharedProject(
                    proto::participant_location::UnsharedProject {},
                )
            }
        } else {
            self.local_participant.active_project = None;
            proto::participant_location::Variant::External(proto::participant_location::External {})
        };

        cx.notify();
        cx.background_spawn(async move {
            client
                .request(proto::UpdateParticipantLocation {
                    room_id,
                    location: Some(proto::ParticipantLocation {
                        variant: Some(location),
                    }),
                })
                .await?;
            Ok(())
        })
    }
}
