use super::*;

impl EditPredictionStore {
    pub(crate) fn report_changes_for_buffer(
        &mut self,
        buffer: &Entity<Buffer>,
        project: &Entity<Project>,
        is_predicted: bool,
        is_local: bool,
        cx: &mut Context<Self>,
    ) {
        let project_state = self.get_or_init_project(project, cx);
        let registered_buffer = Self::register_buffer_impl(project_state, buffer, project, cx);

        let buf = buffer.read(cx);
        let new_file = buf.file().cloned();
        let new_snapshot = buf.text_snapshot();
        if new_snapshot.version == registered_buffer.snapshot.version {
            return;
        }
        let old_file = mem::replace(&mut registered_buffer.file, new_file.clone());
        let old_snapshot = mem::replace(&mut registered_buffer.snapshot, new_snapshot.clone());
        let mut edit_range: Option<Range<Anchor>> = None;
        let now = cx.background_executor().now();

        for (_edit, anchor_range) in
            new_snapshot.anchored_edits_since::<usize>(&old_snapshot.version)
        {
            edit_range = Some(match edit_range {
                None => anchor_range,
                Some(acc) => acc.start..anchor_range.end,
            });
        }

        let Some(edit_range) = edit_range else {
            return;
        };

        for pending_capture in &mut project_state.pending_prediction_captures {
            if pending_capture.edited_buffer_id == buffer.entity_id()
                && edit_range.overlaps(&pending_capture.editable_anchor_range, &new_snapshot)
            {
                pending_capture.last_edit_at = now;
                if is_local
                    && !is_predicted
                    && let Some(sample_data) = pending_capture.sample_data.as_mut()
                    && sample_data.next_edit_cursor_offset.is_none()
                {
                    sample_data.next_edit_cursor_offset =
                        Some(edit_range.start.to_offset(&new_snapshot));
                }
            }
        }

        let include_in_history = is_local
            || collaborator_edit_overlaps_locality_region(
                project_state,
                project,
                buffer,
                &buf.snapshot(),
                &edit_range,
                cx,
            );

        if !include_in_history {
            return;
        }

        let is_recordable_history_edit =
            compute_diff_between_snapshots_in_range(&old_snapshot, &new_snapshot, &edit_range)
                .is_some();

        if !is_recordable_history_edit {
            project_state.finalize_last_event(cx);
            return;
        }

        if let Some(last_event) = project_state.last_event.as_mut() {
            let is_next_snapshot_of_same_buffer = old_snapshot.remote_id()
                == last_event.new_snapshot.remote_id()
                && old_snapshot.version == last_event.new_snapshot.version;

            let prediction_source_changed = is_predicted != last_event.predicted;

            let should_coalesce = is_next_snapshot_of_same_buffer
                && !prediction_source_changed
                && lines_between_ranges(
                    &edit_range.to_point(&new_snapshot),
                    &last_event.latest_edit_range.to_point(&new_snapshot),
                ) <= CHANGE_GROUPING_LINE_SPAN;

            if should_coalesce {
                let pause_elapsed = last_event
                    .last_edit_time
                    .map(|t| now.duration_since(t) >= LAST_CHANGE_GROUPING_TIME)
                    .unwrap_or(false);
                if pause_elapsed {
                    last_event.snapshot_after_last_editing_pause =
                        Some(last_event.new_snapshot.clone());
                    last_event.total_edit_range_at_last_pause_boundary =
                        Some(last_event.total_edit_range.clone());
                }

                last_event.latest_edit_range = edit_range.clone();
                last_event.total_edit_range =
                    merge_anchor_ranges(&last_event.total_edit_range, &edit_range, &new_snapshot);
                last_event.new_snapshot = new_snapshot;
                last_event.last_edit_time = Some(now);
                return;
            }
        }

        project_state.finalize_last_event(cx);

        merge_trailing_events_if_needed(
            &mut project_state.events,
            &old_snapshot,
            &new_snapshot,
            &edit_range,
        );

        let file_context = new_file.as_ref().map(|file| {
            let project_path = ProjectPath::from_file(file.as_ref(), cx);
            let file_context = project_state.file_context_for_path(project_path.clone(), cx);
            Self::ensure_git_changed_file_sets_loading(&file_context, project, &project_path, cx);
            file_context
        });

        let seq = project_state.next_last_event_seq;
        project_state.next_last_event_seq += 1;
        project_state.last_event = Some(LastEvent {
            seq,
            old_file,
            new_file,
            old_snapshot,
            new_snapshot,
            latest_edit_range: edit_range.clone(),
            total_edit_range: edit_range,
            total_edit_range_at_last_pause_boundary: None,
            predicted: is_predicted,
            snapshot_after_last_editing_pause: None,
            last_edit_time: Some(now),
            file_context,
        });
    }

    pub(crate) fn prediction_at(
        &mut self,
        buffer: &Entity<Buffer>,
        position: Option<language::Anchor>,
        project: &Entity<Project>,
        cx: &App,
    ) -> Option<BufferEditPrediction<'_>> {
        let project_state = self.projects.get_mut(&project.entity_id())?;
        if let Some(position) = position {
            let snapshot = buffer.read(cx).snapshot();
            let cursor_position = position.to_offset(&snapshot);
            if let Some(file) = snapshot.file() {
                project_state.update_recent_file_cursor(file.path().as_std_path(), cursor_position);
            }
            if let Some(buffer) = project_state
                .registered_buffers
                .get_mut(&buffer.entity_id())
            {
                buffer.last_position = Some(position);
            }
        }

        let CurrentEditPrediction {
            requested_by,
            prediction,
            ..
        } = project_state.current_prediction.as_ref()?;

        if prediction.targets_buffer(buffer.read(cx)) {
            Some(BufferEditPrediction::Local { prediction })
        } else if requested_by == &buffer.entity_id() {
            Some(BufferEditPrediction::Jump { prediction })
        } else {
            None
        }
    }

    pub(crate) fn accept_current_prediction(
        &mut self,
        project: &Entity<Project>,
        cx: &mut Context<Self>,
    ) {
        let Some(current_prediction) = self
            .projects
            .get_mut(&project.entity_id())
            .and_then(|project_state| project_state.current_prediction.take())
        else {
            return;
        };

        self.report_changes_for_buffer(
            &current_prediction.prediction.buffer,
            project,
            true,
            true,
            cx,
        );

        // can't hold &mut project_state ref across report_changes_for_buffer_call
        let Some(project_state) = self.projects.get_mut(&project.entity_id()) else {
            return;
        };

        for pending_prediction in mem::take(&mut project_state.pending_predictions) {
            project_state.cancel_pending_prediction(pending_prediction, cx);
        }

        match self.edit_prediction_model {
            EditPredictionModel::Mercury => {
                mercury::edit_prediction_accepted(
                    current_prediction.prediction.id,
                    self.client.http_client(),
                    cx,
                );
            }
            EditPredictionModel::Zeta => {
                let is_cloud = !matches!(
                    all_language_settings(None, cx).edit_predictions.provider,
                    EditPredictionProvider::Ollama | EditPredictionProvider::OpenAiCompatibleApi
                );
                if is_cloud {
                    zeta::edit_prediction_accepted(self, current_prediction, cx)
                }
            }
            EditPredictionModel::Fim { .. } => {}
        }
    }
}
