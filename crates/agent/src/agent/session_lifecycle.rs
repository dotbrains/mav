use super::*;

impl NativeAgent {
    pub fn load_thread(
        &mut self,
        id: acp::SessionId,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Thread>>> {
        let database_future = ThreadsDatabase::connect(cx);
        cx.spawn(async move |this, cx| {
            let database = database_future.await.map_err(|err| anyhow!(err))?;
            let db_thread = database
                .load_thread(id.clone())
                .await?
                .with_context(|| format!("no thread found with ID: {id:?}"))?;

            this.update(cx, |this, cx| {
                let project_id = this.get_or_create_project_state(&project, cx);
                let project_state = this
                    .projects
                    .get(&project_id)
                    .context("project state not found")?;
                let summarization_model = LanguageModelRegistry::read_global(cx)
                    .thread_summary_model(cx)
                    .map(|c| c.model);

                Ok(cx.new(|cx| {
                    let mut thread = Thread::from_db(
                        id.clone(),
                        db_thread,
                        project_state.project.clone(),
                        project_state.project_context.clone(),
                        project_state.context_server_registry.clone(),
                        this.templates.clone(),
                        cx,
                    );
                    thread.set_summarization_model(summarization_model, cx);
                    thread
                }))
            })?
        })
    }

    pub fn open_thread(
        &mut self,
        id: acp::SessionId,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<AcpThread>>> {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.ref_count += 1;
            return Task::ready(Ok(session.acp_thread.clone()));
        }

        if let Some(pending) = self.pending_sessions.get_mut(&id) {
            pending.ref_count += 1;
            let task = pending.task.clone();
            return cx.background_spawn(async move { task.await.map_err(|err| anyhow!(err)) });
        }

        let task = self.load_thread(id.clone(), project.clone(), cx);
        let shared_task = cx
            .spawn({
                let id = id.clone();
                async move |this, cx| {
                    let thread = match task.await {
                        Ok(thread) => thread,
                        Err(err) => {
                            this.update(cx, |this, _cx| {
                                this.pending_sessions.remove(&id);
                            })
                            .ok();
                            return Err(Arc::new(err));
                        }
                    };
                    let acp_thread = this
                        .update(cx, |this, cx| {
                            let project_id = this.get_or_create_project_state(&project, cx);
                            let ref_count = this
                                .pending_sessions
                                .remove(&id)
                                .map_or(1, |pending| pending.ref_count);
                            this.register_session(thread.clone(), project_id, ref_count, cx)
                        })
                        .map_err(Arc::new)?;
                    let events = thread.update(cx, |thread, cx| thread.replay(cx));
                    cx.update(|cx| {
                        NativeAgentConnection::handle_thread_events(
                            events,
                            acp_thread.downgrade(),
                            None,
                            cx,
                        )
                    })
                    .await
                    .map_err(Arc::new)?;
                    acp_thread.update(cx, |thread, cx| {
                        thread.snapshot_completed_plan(cx);
                    });
                    Ok(acp_thread)
                }
            })
            .shared();
        self.pending_sessions.insert(
            id,
            PendingSession {
                task: shared_task.clone(),
                ref_count: 1,
            },
        );

        cx.background_spawn(async move { shared_task.await.map_err(|err| anyhow!(err)) })
    }

    pub fn thread_summary(
        &mut self,
        id: acp::SessionId,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Task<Result<SharedString>> {
        let thread = self.open_thread(id.clone(), project, cx);
        cx.spawn(async move |this, cx| {
            let acp_thread = thread.await?;
            let result = this
                .update(cx, |this, cx| {
                    this.sessions
                        .get(&id)
                        .unwrap()
                        .thread
                        .update(cx, |thread, cx| thread.summary(cx))
                })?
                .await
                .context("Failed to generate summary")?;

            this.update(cx, |this, cx| this.close_session(&id, cx))?
                .await?;
            drop(acp_thread);
            Ok(result)
        })
    }

    pub(super) fn close_session(
        &mut self,
        session_id: &acp::SessionId,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return Task::ready(Ok(()));
        };

        session.ref_count -= 1;
        if session.ref_count > 0 {
            return Task::ready(Ok(()));
        }

        let thread = session.thread.clone();
        self.save_thread(thread, cx);
        let Some(session) = self.sessions.remove(session_id) else {
            return Task::ready(Ok(()));
        };
        let project_id = session.project_id;

        let has_remaining = self.sessions.values().any(|s| s.project_id == project_id);
        if !has_remaining {
            self.projects.remove(&project_id);
            self.publish_skill_index(cx);
        }

        session.pending_save
    }

    pub(super) fn save_thread(&mut self, thread: Entity<Thread>, cx: &mut Context<Self>) {
        let id = thread.read(cx).id().clone();
        let Some(session) = self.sessions.get(&id) else {
            return;
        };
        let Some((id, folder_paths, db_thread)) = self.thread_save_payload(session, cx) else {
            return;
        };

        let database_future = ThreadsDatabase::connect(cx);
        let thread_store = self.thread_store.clone();
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        session.pending_save = cx.spawn(async move |_, cx| {
            let Some(database) = database_future.await.map_err(|err| anyhow!(err)).log_err() else {
                return Ok(());
            };
            let db_thread = db_thread.await;
            database
                .save_thread(id, db_thread, folder_paths)
                .await
                .log_err();
            thread_store.update(cx, |store, cx| store.reload(cx));
            Ok(())
        });
    }

    /// Builds everything needed to persist a session's thread content,
    /// capturing the current draft prompt from the ACP thread. Returns `None`
    /// if the thread is empty or its project state is gone.
    fn thread_save_payload(
        &self,
        session: &Session,
        cx: &mut App,
    ) -> Option<(acp::SessionId, PathList, Task<DbThread>)> {
        if session.thread.read(cx).is_empty() {
            return None;
        }
        let state = self.projects.get(&session.project_id)?;
        let folder_paths = PathList::new(
            &state
                .project
                .read(cx)
                .visible_worktrees(cx)
                .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
                .collect::<Vec<_>>(),
        );
        let draft_prompt = session.acp_thread.read(cx).draft_prompt().map(Vec::from);
        let id = session.thread.read(cx).id().clone();
        let db_thread = session.thread.update(cx, |thread, cx| {
            thread.set_draft_prompt(draft_prompt);
            thread.to_db(cx)
        });
        Some((id, folder_paths, db_thread))
    }

    /// Commits every non-empty thread's content on shutdown so the async
    /// `save_thread` losing the race can't leave metadata without content.
    pub(super) fn flush_threads_on_quit(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl Future<Output = ()> + use<> {
        let database_future = ThreadsDatabase::connect(cx);

        let mut saves = Vec::new();
        for session in self.sessions.values() {
            saves.extend(self.thread_save_payload(session, cx));
        }

        async move {
            let Some(database) = database_future.await.map_err(|err| anyhow!(err)).log_err() else {
                return;
            };
            // All quit observers share `gpui::SHUTDOWN_TIMEOUT`, so run the
            // saves concurrently instead of one at a time.
            future::join_all(saves.into_iter().map(|(id, folder_paths, db_thread)| {
                let database = database.clone();
                async move {
                    let db_thread = db_thread.await;
                    database
                        .save_thread(id, db_thread, folder_paths)
                        .await
                        .log_err();
                }
            }))
            .await;
        }
    }
}
