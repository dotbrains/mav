use super::*;

impl EditPredictionStore {
    pub fn clear_history_for_project(&mut self, project: &Entity<Project>) {
        if let Some(project_state) = self.projects.get_mut(&project.entity_id()) {
            project_state.clear_history();
        }
    }

    pub fn edit_history_for_project(
        &self,
        project: &Entity<Project>,
        cx: &App,
    ) -> Vec<StoredEvent> {
        self.projects
            .get(&project.entity_id())
            .map(|project_state| project_state.events(cx))
            .unwrap_or_default()
    }

    pub fn context_for_project<'a>(
        &'a self,
        project: &Entity<Project>,
        cx: &'a mut App,
    ) -> Vec<RelatedFile> {
        self.projects
            .get(&project.entity_id())
            .map(|project_state| {
                project_state.context.update(cx, |context, cx| {
                    context
                        .related_files_with_buffers(cx)
                        .map(|(mut related_file, buffer)| {
                            related_file.in_open_source_repo = buffer
                                .read(cx)
                                .file()
                                .map_or(false, |file| self.is_file_open_source(&project, file, cx));
                            related_file
                        })
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    pub fn copilot_for_project(&self, project: &Entity<Project>) -> Option<Entity<Copilot>> {
        self.projects
            .get(&project.entity_id())
            .and_then(|project| project.copilot.clone())
    }

    pub fn start_copilot_for_project(
        &mut self,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Copilot>> {
        if DisableAiSettings::get(None, cx).disable_ai {
            return None;
        }
        let state = self.get_or_init_project(project, cx);

        if state.copilot.is_some() {
            return state.copilot.clone();
        }
        let _project = project.clone();
        let project = project.read(cx);

        let node = project.node_runtime().cloned();
        if let Some(node) = node {
            let next_id = project.languages().next_language_server_id();
            let fs = project.fs().clone();

            let copilot = cx.new(|cx| Copilot::new(Some(_project), next_id, fs, node, cx));
            state.copilot = Some(copilot.clone());
            Some(copilot)
        } else {
            None
        }
    }

    pub fn context_for_project_with_buffers<'a>(
        &'a self,
        project: &Entity<Project>,
        cx: &'a mut App,
    ) -> Vec<(RelatedFile, Entity<Buffer>)> {
        self.projects
            .get(&project.entity_id())
            .map(|project| {
                project.context.update(cx, |context, cx| {
                    context.related_files_with_buffers(cx).collect()
                })
            })
            .unwrap_or_default()
    }

    pub fn usage(&self, cx: &App) -> Option<EditPredictionUsage> {
        if matches!(self.edit_prediction_model, EditPredictionModel::Zeta) {
            self.user_store.read(cx).edit_prediction_usage()
        } else {
            None
        }
    }

    pub fn register_project(&mut self, project: &Entity<Project>, cx: &mut Context<Self>) {
        self.get_or_init_project(project, cx);
    }

    pub fn register_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) {
        let opened_path = buffer
            .read(cx)
            .file()
            .map(|file| ProjectPath::from_file(file.as_ref(), cx));
        let project_state = self.get_or_init_project(project, cx);
        if let Some(path) = opened_path {
            push_recent_file(
                &mut project_state.recently_opened_files,
                RecentFile {
                    path: path.path.as_std_path().into(),
                    cursor_position: None,
                },
            );
        }
        Self::register_buffer_impl(project_state, buffer, project, cx);
    }

    pub(crate) fn ensure_git_changed_file_sets_loading(
        file_context: &Entity<StoredFileContext>,
        project: &Entity<Project>,
        project_path: &ProjectPath,
        cx: &mut Context<Self>,
    ) {
        let should_start = file_context.update(cx, |file_context, _| {
            file_context.git_changed_file_sets.is_none()
                && file_context.git_changed_file_sets_task.is_none()
        });
        if !should_start {
            return;
        }

        let Some((repository, repo_path)) = project
            .read(cx)
            .git_store()
            .read(cx)
            .repository_and_path_for_project_path(project_path, cx)
        else {
            file_context.update(cx, |file_context, _| {
                file_context.git_changed_file_sets = Some(Arc::default());
            });
            return;
        };

        let receiver = repository.update(cx, |repository, _| {
            repository
                .file_history_changed_files(vec![repo_path], GIT_CHANGED_FILE_SETS_COMMIT_LIMIT)
        });
        let task = cx.spawn({
            let file_context = file_context.downgrade();
            async move |_, cx| {
                let result = receiver.await;
                let Some(file_context) = file_context.upgrade() else {
                    return;
                };
                file_context.update(cx, |file_context, _| {
                    file_context.git_changed_file_sets = result
                        .context("failed to receive git changed file sets")
                        .flatten()
                        .log_with_level(log::Level::Trace)
                        .map(|mut file_sets| file_sets.pop().unwrap_or_default())
                        .context("failed to load git changed file sets")
                        .map(Arc::new)
                        .log_err();
                    file_context.git_changed_file_sets_task = None;
                });
            }
        });
        file_context.update(cx, |file_context, _| {
            file_context.git_changed_file_sets_task = Some(task);
        });
    }

    pub(crate) fn get_or_init_project(
        &mut self,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) -> &mut ProjectState {
        let entity_id = project.entity_id();
        self.projects
            .entry(entity_id)
            .or_insert_with(|| ProjectState {
                context: {
                    let related_excerpt_store = cx.new(|cx| RelatedExcerptStore::new(project, cx));
                    cx.subscribe(&related_excerpt_store, move |this, _, event, _| {
                        this.handle_excerpt_store_event(entity_id, event);
                    })
                    .detach();
                    related_excerpt_store
                },
                events: VecDeque::new(),
                last_event: None,
                next_last_event_seq: 0,
                recently_viewed_files: VecDeque::new(),
                recently_opened_files: VecDeque::new(),
                debug_tx: None,
                registered_buffers: HashMap::default(),
                file_contexts: HashMap::default(),
                current_prediction: None,
                last_edit_source: None,
                cancelled_predictions: HashSet::default(),
                pending_predictions: ArrayVec::new(),
                pending_prediction_captures: Vec::new(),
                next_pending_prediction_id: 0,
                last_edit_prediction_refresh: None,
                license_detection_watchers: HashMap::default(),
                _subscriptions: [
                    cx.subscribe(&project, Self::handle_project_event),
                    cx.observe_release(&project, move |this, _, cx| {
                        this.projects.remove(&entity_id);
                        cx.notify();
                    }),
                ],
                copilot: None,
            })
    }

    pub fn remove_project(&mut self, project: &Entity<Project>) {
        self.projects.remove(&project.entity_id());
    }

    pub(crate) fn handle_excerpt_store_event(
        &mut self,
        project_entity_id: EntityId,
        event: &RelatedExcerptStoreEvent,
    ) {
        if let Some(project_state) = self.projects.get(&project_entity_id) {
            if let Some(debug_tx) = project_state.debug_tx.clone() {
                match event {
                    RelatedExcerptStoreEvent::StartedRefresh => {
                        debug_tx
                            .unbounded_send(DebugEvent::ContextRetrievalStarted(
                                ContextRetrievalStartedDebugEvent {
                                    project_entity_id: project_entity_id,
                                    timestamp: Instant::now(),
                                    search_prompt: String::new(),
                                },
                            ))
                            .ok();
                    }
                    RelatedExcerptStoreEvent::FinishedRefresh {
                        cache_hit_count,
                        cache_miss_count,
                        mean_definition_latency,
                        max_definition_latency,
                    } => {
                        debug_tx
                            .unbounded_send(DebugEvent::ContextRetrievalFinished(
                                ContextRetrievalFinishedDebugEvent {
                                    project_entity_id: project_entity_id,
                                    timestamp: Instant::now(),
                                    metadata: vec![
                                        (
                                            "Cache Hits",
                                            format!(
                                                "{}/{}",
                                                cache_hit_count,
                                                cache_hit_count + cache_miss_count
                                            )
                                            .into(),
                                        ),
                                        (
                                            "Max LSP Time",
                                            format!("{} ms", max_definition_latency.as_millis())
                                                .into(),
                                        ),
                                        (
                                            "Mean LSP Time",
                                            format!("{} ms", mean_definition_latency.as_millis())
                                                .into(),
                                        ),
                                    ],
                                },
                            ))
                            .ok();
                    }
                }
            }
        }
    }

    pub fn debug_info(
        &mut self,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) -> mpsc::UnboundedReceiver<DebugEvent> {
        let project_state = self.get_or_init_project(project, cx);
        let (debug_watch_tx, debug_watch_rx) = mpsc::unbounded();
        project_state.debug_tx = Some(debug_watch_tx);
        debug_watch_rx
    }

    pub(crate) fn handle_project_event(
        &mut self,
        project: Entity<Project>,
        event: &project::Event,
        cx: &mut Context<Self>,
    ) {
        if !is_ep_store_provider(all_language_settings(None, cx).edit_predictions.provider) {
            return;
        }
        // TODO [zeta2] init with recent paths
        match event {
            project::Event::BufferEdited { source } => {
                self.get_or_init_project(&project, cx).last_edit_source = Some(*source);
            }
            project::Event::ActiveEntryChanged(Some(active_entry_id)) => {
                let Some(project_state) = self.projects.get_mut(&project.entity_id()) else {
                    return;
                };
                let path = project.read(cx).path_for_entry(*active_entry_id, cx);
                if let Some(path) = path {
                    let cursor_position = project
                        .read(cx)
                        .buffer_store()
                        .read(cx)
                        .get_by_path(&path)
                        .and_then(|buffer| {
                            let position = project_state
                                .registered_buffers
                                .get(&buffer.entity_id())?
                                .last_position?;
                            Some(position.to_offset(&buffer.read(cx).snapshot()))
                        });

                    let recent_file = RecentFile {
                        path: path.path.as_std_path().into(),
                        cursor_position,
                    };
                    let can_collect_navigation = project_state
                        .license_detection_watchers
                        .get(&path.worktree_id)
                        .is_some_and(|watcher| watcher.is_project_open_source());
                    for capture in &mut project_state.pending_prediction_captures {
                        if let Some(sample_data) = capture.sample_data.as_mut() {
                            if can_collect_navigation {
                                push_recent_file(
                                    &mut sample_data.navigation_history,
                                    recent_file.clone(),
                                );
                            } else {
                                capture.sample_data = None;
                            }
                        }
                    }
                    push_recent_file(&mut project_state.recently_viewed_files, recent_file);
                }
            }
            _ => (),
        }
    }

    pub(crate) fn register_buffer_impl<'a>(
        project_state: &'a mut ProjectState,
        buffer: &Entity<Buffer>,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) -> &'a mut RegisteredBuffer {
        let buffer_id = buffer.entity_id();

        if let Some(file) = buffer.read(cx).file() {
            let worktree_id = file.worktree_id(cx);
            if let Some(worktree) = project.read(cx).worktree_for_id(worktree_id, cx) {
                project_state
                    .license_detection_watchers
                    .entry(worktree_id)
                    .or_insert_with(|| {
                        let project_entity_id = project.entity_id();
                        cx.observe_release(&worktree, move |this, _worktree, _cx| {
                            let Some(project_state) = this.projects.get_mut(&project_entity_id)
                            else {
                                return;
                            };
                            project_state
                                .license_detection_watchers
                                .remove(&worktree_id);
                        })
                        .detach();
                        Rc::new(LicenseDetectionWatcher::new(&worktree, cx))
                    });
            }
        }

        match project_state.registered_buffers.entry(buffer_id) {
            hash_map::Entry::Occupied(entry) => entry.into_mut(),
            hash_map::Entry::Vacant(entry) => {
                let buf = buffer.read(cx);
                let snapshot = buf.text_snapshot();
                let file = buf.file().cloned();
                let project_entity_id = project.entity_id();
                entry.insert(RegisteredBuffer {
                    snapshot,
                    file,
                    last_position: None,
                    _subscriptions: [
                        cx.subscribe(buffer, {
                            let project = project.downgrade();
                            move |this, buffer, event, cx| {
                                if let language::BufferEvent::Edited { source } = event
                                    && let Some(project) = project.upgrade()
                                {
                                    let project_state = this.get_or_init_project(&project, cx);
                                    project_state.last_edit_source = Some(*source);
                                    this.report_changes_for_buffer(
                                        &buffer,
                                        &project,
                                        false,
                                        source.is_local(),
                                        cx,
                                    );
                                }
                            }
                        }),
                        cx.observe_release(buffer, move |this, _buffer, _cx| {
                            let Some(project_state) = this.projects.get_mut(&project_entity_id)
                            else {
                                return;
                            };
                            project_state.registered_buffers.remove(&buffer_id);
                        }),
                    ],
                })
            }
        }
    }
}
