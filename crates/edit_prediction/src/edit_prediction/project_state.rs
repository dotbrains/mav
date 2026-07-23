use super::*;

impl ProjectState {
    pub fn events(&self, cx: &App) -> Vec<StoredEvent> {
        self.events
            .iter()
            .cloned()
            .chain(self.last_event.as_ref().iter().flat_map(|event| {
                let (one, two) = event.split_by_pause();
                let one = one.finalize(&self.license_detection_watchers, cx);
                let two = two.and_then(|two| two.finalize(&self.license_detection_watchers, cx));
                one.into_iter().chain(two)
            }))
            .collect()
    }

    pub(crate) fn cancel_pending_prediction(
        &mut self,
        pending_prediction: PendingPrediction,
        cx: &mut Context<EditPredictionStore>,
    ) {
        self.cancelled_predictions.insert(pending_prediction.id);

        if pending_prediction.drop_on_cancel {
            drop(pending_prediction.task);
        } else {
            cx.spawn(async move |this, cx| {
                let Some((prediction_id, model_version)) = pending_prediction.task.await else {
                    return;
                };

                this.update(cx, |this, cx| {
                    this.reject_prediction(
                        prediction_id,
                        EditPredictionRejectReason::Canceled,
                        false,
                        model_version,
                        None,
                        cx,
                    );
                })
                .ok();
            })
            .detach()
        }
    }

    pub(crate) fn active_buffer(
        &self,
        project: &Entity<Project>,
        cx: &App,
    ) -> Option<(Entity<Buffer>, Option<Anchor>)> {
        let project = project.read(cx);
        let active_path = project.path_for_entry(project.active_entry()?, cx)?;
        let active_buffer = project.buffer_store().read(cx).get_by_path(&active_path)?;
        let registered_buffer = self.registered_buffers.get(&active_buffer.entity_id())?;
        Some((active_buffer, registered_buffer.last_position))
    }

    pub(crate) fn file_context_for_path(
        &mut self,
        path: ProjectPath,
        cx: &mut Context<EditPredictionStore>,
    ) -> Entity<StoredFileContext> {
        if let Some(context) = self
            .file_contexts
            .get_mut(&path)
            .and_then(|entry| entry.upgrade())
        {
            context
        } else {
            let context = cx.new(|_| StoredFileContext {
                uncommitted_diff: None,
                git_changed_file_sets: None,
                git_changed_file_sets_task: None,
            });
            self.file_contexts.insert(path, context.downgrade());
            context
        }
    }

    pub(crate) fn update_recent_file_cursor(&mut self, path: &Path, cursor_position: usize) {
        for file in &mut self.recently_opened_files {
            if file.path.as_ref() == path && file.cursor_position.is_none() {
                file.cursor_position = Some(cursor_position);
            }
        }
        for file in &mut self.recently_viewed_files {
            if file.path.as_ref() == path {
                file.cursor_position = Some(cursor_position);
            }
        }
    }

    pub(crate) fn finalize_last_event(&mut self, cx: &mut Context<EditPredictionStore>) {
        let Some(last_event) = self.last_event.take() else {
            return;
        };
        let event = last_event.finalize(&self.license_detection_watchers, cx);

        for capture in &mut self.pending_prediction_captures {
            capture.try_record_future_event(
                &last_event,
                event.as_ref(),
                &self.license_detection_watchers,
                cx,
            );
        }

        let Some(event) = event else {
            return;
        };
        if self.events.len() + 1 >= EVENT_COUNT_MAX {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub(crate) fn clear_history(&mut self) {
        self.events.clear();
        self.last_event.take();
        for capture in &mut self.pending_prediction_captures {
            capture.sample_data = None;
        }
    }
}
