use super::*;

impl EditPredictionStore {
    pub fn refresh_context(
        &mut self,
        project: &Entity<Project>,
        buffer: &Entity<language::Buffer>,
        cursor_position: language::Anchor,
        cx: &mut Context<Self>,
    ) {
        self.get_or_init_project(project, cx)
            .context
            .update(cx, |store, cx| {
                store.refresh(buffer.clone(), cursor_position, cx);
            });
    }

    pub fn collect_editable_context(
        &mut self,
        project: Entity<Project>,
        buffer: Entity<language::Buffer>,
        cursor_position: language::Anchor,
        oracle_targets: Vec<edit_prediction_context::OracleTarget>,
        context_sources: Vec<ContextSource>,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Vec<RelatedFile>>> {
        use edit_prediction_context::{EditHistoryContextEntry, collect_editable_context};

        let buffers_by_id = project.read(cx).opened_buffers(cx).into_iter().fold(
            HashMap::default(),
            |mut buffers_by_id, buffer| {
                buffers_by_id.insert(buffer.read(cx).remote_id(), buffer.clone());
                buffers_by_id
            },
        );
        let edit_history = self
            .edit_history_for_project(&project, cx)
            .into_iter()
            .filter_map(|event| {
                let buffer = buffers_by_id.get(&event.old_snapshot.remote_id())?.clone();
                Some(EditHistoryContextEntry {
                    buffer,
                    edited_range: event.total_edit_range,
                })
            })
            .collect();

        cx.spawn(async move |_, cx| {
            collect_editable_context(
                project,
                buffer,
                cursor_position,
                edit_history,
                oracle_targets,
                context_sources,
                cx,
            )
            .await
        })
    }

    #[cfg(feature = "cli-support")]
    pub fn set_context_for_buffer(
        &mut self,
        project: &Entity<Project>,
        related_files: Vec<RelatedFile>,
        cx: &mut Context<Self>,
    ) {
        self.get_or_init_project(project, cx)
            .context
            .update(cx, |store, cx| {
                store.set_related_files(related_files, cx);
            });
    }

    #[cfg(feature = "cli-support")]
    pub fn set_recent_paths_for_project(
        &mut self,
        project: &Entity<Project>,
        paths: impl IntoIterator<Item = project::ProjectPath>,
        cx: &mut Context<Self>,
    ) {
        let project_state = self.get_or_init_project(project, cx);
        project_state.recently_viewed_files = paths
            .into_iter()
            .map(|path| RecentFile {
                path: path.path.as_std_path().into(),
                cursor_position: None,
            })
            .collect();
    }

    pub fn recently_opened_files_for_project(&self, project: &Entity<Project>) -> Vec<RecentFile> {
        self.projects
            .get(&project.entity_id())
            .map(|project_state| {
                project_state
                    .recently_opened_files
                    .iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn recently_viewed_files_for_project(&self, project: &Entity<Project>) -> Vec<RecentFile> {
        self.projects
            .get(&project.entity_id())
            .map(|project_state| {
                project_state
                    .recently_viewed_files
                    .iter()
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn is_file_open_source(
        &self,
        project: &Entity<Project>,
        file: &Arc<dyn File>,
        cx: &App,
    ) -> bool {
        if !file.is_local() || file.is_private() {
            return false;
        }
        let Some(project_state) = self.projects.get(&project.entity_id()) else {
            return false;
        };
        project_state
            .license_detection_watchers
            .get(&file.worktree_id(cx))
            .as_ref()
            .is_some_and(|watcher| watcher.is_project_open_source())
    }

    pub(crate) fn is_data_collection_enabled(&self, cx: &App) -> bool {
        if !self.is_data_collection_allowed_by_organization(cx) {
            return false;
        }

        if cx.is_staff() {
            return true;
        }

        match all_language_settings(None, cx)
            .edit_predictions
            .allow_data_collection
        {
            EditPredictionDataCollectionChoice::Yes => true,
            EditPredictionDataCollectionChoice::No => false,
            // Fall back to the legacy KV entry captured when the store was
            // created, preserving existing users' choices without per-request
            // database reads.
            EditPredictionDataCollectionChoice::Default => self.legacy_data_collection_enabled,
        }
    }

    pub(crate) fn load_legacy_data_collection_enabled(cx: &App) -> bool {
        KeyValueStore::global(cx)
            .read_kvp(MAV_PREDICT_DATA_COLLECTION_CHOICE)
            .log_err()
            .flatten()
            .as_deref()
            == Some("true")
    }

    pub(crate) fn is_data_collection_allowed_by_organization(&self, cx: &App) -> bool {
        self.user_store
            .read(cx)
            .current_organization_configuration()
            .is_none_or(|organization_configuration| {
                organization_configuration
                    .edit_prediction
                    .is_feedback_enabled
            })
    }

    pub fn rateable_predictions(&self) -> impl DoubleEndedIterator<Item = &EditPrediction> {
        self.rateable_predictions.iter()
    }

    pub fn rateable_predictions_count(&self) -> usize {
        self.rateable_predictions.len()
    }

    pub fn is_prediction_rated(&self, id: &EditPredictionId) -> bool {
        self.rated_predictions.contains(id)
    }

    pub fn rate_prediction(
        &mut self,
        prediction: &EditPrediction,
        rating: EditPredictionRating,
        feedback: String,
        expected_output: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let organization = self.user_store.read(cx).current_organization();

        self.rated_predictions.insert(prediction.id.clone());

        cx.background_spawn({
            let client = self.client.clone();
            let prediction_id = prediction.id.to_string();
            let inputs = serde_json::to_value(&prediction.inputs);
            let output = prediction
                .edit_preview
                .as_unified_diff(prediction.snapshot.file(), &prediction.edits);
            async move {
                client
                    .cloud_client()
                    .submit_edit_prediction_feedback(SubmitEditPredictionFeedbackBody {
                        organization_id: organization.map(|organization| organization.id.clone()),
                        request_id: prediction_id,
                        rating: match rating {
                            EditPredictionRating::Positive => "positive".to_string(),
                            EditPredictionRating::Negative => "negative".to_string(),
                        },
                        inputs: inputs?,
                        output,
                        expected_output,
                        feedback,
                    })
                    .await?;

                anyhow::Ok(())
            }
        })
        .detach_and_log_err(cx);

        cx.notify();
    }
}
