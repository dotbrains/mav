use super::*;

impl EditPredictionStore {
    async fn handle_rejected_predictions(
        rx: UnboundedReceiver<EditPredictionRejectionPayload>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        app_version: Version,
        background_executor: BackgroundExecutor,
    ) {
        let mut rx = std::pin::pin!(rx.peekable());
        let mut batched = Vec::new();

        while let Some(EditPredictionRejectionPayload {
            rejection,
            organization_id,
        }) = rx.next().await
        {
            batched.push(rejection);

            if batched.len() < MAX_EDIT_PREDICTION_REJECTIONS_PER_REQUEST / 2 {
                select_biased! {
                    next = rx.as_mut().peek().fuse() => {
                        if next.is_some() {
                            continue;
                        }
                    }
                    () = background_executor.timer(REJECT_REQUEST_DEBOUNCE).fuse() => {},
                }
            }

            let url = client
                .http_client()
                .build_mav_llm_url("/predict_edits/reject", &[])
                .unwrap();

            let flush_count = batched
                .len()
                // in case items have accumulated after failure
                .min(MAX_EDIT_PREDICTION_REJECTIONS_PER_REQUEST);
            let start = batched.len() - flush_count;

            let body = RejectEditPredictionsBodyRef {
                rejections: &batched[start..],
            };

            let result = Self::send_api_request::<()>(
                |builder| {
                    let req = builder
                        .uri(url.as_ref())
                        .body(serde_json::to_string(&body)?.into());
                    anyhow::Ok(req?)
                },
                client.clone(),
                llm_token.clone(),
                organization_id,
                app_version.clone(),
            )
            .await;

            if result.log_err().is_some() {
                batched.drain(start..);
            }
        }
    }

    async fn run_settled_predictions_worker(
        this: WeakEntity<Self>,
        mut rx: UnboundedReceiver<Instant>,
        client: Arc<Client>,
        llm_token: LlmApiToken,
        app_version: Version,
        cx: &mut AsyncApp,
    ) {
        let mut next_wake_time: Option<Instant> = None;
        loop {
            let now = cx.background_executor().now();
            if let Some(wake_time) = next_wake_time.take() {
                cx.background_executor()
                    .timer(wake_time.duration_since(now))
                    .await;
            } else {
                let Some(new_enqueue_time) = rx.next().await else {
                    break;
                };
                next_wake_time = Some(new_enqueue_time + EDIT_PREDICTION_SETTLED_QUIESCENCE);
                while rx.next().now_or_never().flatten().is_some() {}
                continue;
            }

            let Some(this) = this.upgrade() else {
                break;
            };

            let now = cx.background_executor().now();
            let mut oldest_edited_at = None;
            let mut ready_predictions = Vec::new();

            this.update(cx, |this, cx| {
                for project_state in this.projects.values_mut() {
                    let ProjectState {
                        last_event,
                        registered_buffers,
                        license_detection_watchers,
                        pending_prediction_captures,
                        ..
                    } = project_state;
                    let pending_last_event = last_event.as_ref().map(|last_event| {
                        (
                            last_event,
                            last_event.finalize(license_detection_watchers, cx),
                        )
                    });
                    let mut pending_index = 0;
                    while pending_index < pending_prediction_captures.len() {
                        let pending_capture = &pending_prediction_captures[pending_index];
                        let age = now.saturating_duration_since(pending_capture.enqueued_at);
                        if age >= EDIT_PREDICTION_SETTLED_TTL {
                            pending_prediction_captures.remove(pending_index);
                            continue;
                        }

                        let quiet_for = now.saturating_duration_since(pending_capture.last_edit_at);
                        if quiet_for >= EDIT_PREDICTION_SETTLED_QUIESCENCE {
                            let Some(registered_buffer) =
                                registered_buffers.get(&pending_capture.edited_buffer_id)
                            else {
                                pending_prediction_captures.remove(pending_index);
                                continue;
                            };
                            let editable_offset_range = pending_capture
                                .editable_anchor_range
                                .to_offset(&registered_buffer.snapshot);
                            if editable_offset_range.len()
                                > EDIT_PREDICTION_SETTLED_MAX_EDITABLE_REGION_BYTES
                            {
                                // The prediction was obliterated by a huge edit;
                                // kept-rate against it would be meaningless and the
                                // region would blow the body size cap.
                                pending_prediction_captures.remove(pending_index);
                                continue;
                            }
                            let settled_editable_region = registered_buffer
                                .snapshot
                                .text_for_range(editable_offset_range)
                                .collect::<String>();
                            let mut pending_capture =
                                pending_prediction_captures.remove(pending_index);
                            if let Some((last_event, finalized_event)) = pending_last_event.as_ref()
                            {
                                pending_capture.try_record_future_event(
                                    last_event,
                                    finalized_event.as_ref(),
                                    license_detection_watchers,
                                    cx,
                                );
                            }
                            ready_predictions.push((pending_capture, settled_editable_region));
                            continue;
                        }

                        if oldest_edited_at.is_none_or(|time| pending_capture.last_edit_at < time) {
                            oldest_edited_at = Some(pending_capture.last_edit_at);
                        }
                        pending_index += 1;
                    }
                }
            });

            let mut ready_predictions_by_organization_id: HashMap<_, Vec<_>> = HashMap::default();
            for (pending_capture, settled_editable_region) in ready_predictions {
                #[cfg(test)]
                {
                    let request_id = pending_capture.request_id.clone();
                    let settled_editable_region = settled_editable_region.clone();
                    this.update(cx, |this, _| {
                        if let Some(callback) = &this.settled_event_callback {
                            callback(request_id, settled_editable_region);
                        }
                    });
                }
                ready_predictions_by_organization_id
                    .entry(pending_capture.organization_id.clone())
                    .or_default()
                    .push((pending_capture, settled_editable_region));
            }

            cx.background_spawn({
                let client = client.clone();
                let llm_token = llm_token.clone();
                let app_version = app_version.clone();
                async move {
                    send_settled_batches(
                        client,
                        llm_token,
                        app_version,
                        ready_predictions_by_organization_id,
                    )
                    .await;
                }
            })
            .detach();

            next_wake_time = oldest_edited_at.map(|time| time + EDIT_PREDICTION_SETTLED_QUIESCENCE);
        }
    }

    pub(crate) fn enqueue_settled_prediction(
        &mut self,
        request_id: EditPredictionId,
        project: &Entity<Project>,
        edited_buffer: &Entity<Buffer>,
        edited_buffer_snapshot: &BufferSnapshot,
        editable_offset_range: Range<usize>,
        edit_preview: &EditPreview,
        context_task: Option<Task<Result<CapturedPredictionContext>>>,
        prompt_history_boundary: Option<PromptHistoryBoundary>,
        model_version: Option<String>,
        e2e_latency: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        let this = &mut *self;
        let is_in_open_source_repo = edited_buffer_snapshot
            .file()
            .map_or(false, |file| this.is_file_open_source(project, file, cx));
        let can_collect_data = !cfg!(test)
            && is_in_open_source_repo
            && this.is_data_collection_enabled(cx)
            && matches!(this.edit_prediction_model, EditPredictionModel::Zeta);

        let organization_id = this
            .user_store
            .read(cx)
            .current_organization()
            .map(|organization| organization.id.clone());
        let project_state = this.get_or_init_project(project, cx);
        if !project_state
            .registered_buffers
            .contains_key(&edited_buffer.entity_id())
        {
            return;
        }

        let editable_region_before_prediction = edited_buffer_snapshot
            .text_for_range(editable_offset_range.clone())
            .collect::<String>();
        let editable_anchor_range_for_result =
            edited_buffer_snapshot.anchor_range_inside(editable_offset_range.clone());
        let predicted_editable_region = edit_preview
            .result_text_snapshot()
            .text_for_range(editable_anchor_range_for_result.clone())
            .collect();
        let ts_error_count_before_prediction = crate::metrics::count_tree_sitter_errors(
            edited_buffer_snapshot
                .syntax_layers_for_range(editable_anchor_range_for_result.clone(), true),
        );
        let ts_error_count_after_prediction = crate::metrics::count_tree_sitter_errors(
            edit_preview.result_syntax_snapshot().layers_for_range(
                editable_anchor_range_for_result,
                edit_preview.result_text_snapshot(),
                true,
            ),
        );
        let editable_anchor_range =
            edited_buffer_snapshot.anchor_range_inside(editable_offset_range.clone());
        let now = cx.background_executor().now();
        let sample_data = if can_collect_data
            && let Some(context_task) = context_task
            && let Some(file) = edited_buffer_snapshot.file()
        {
            Some(PendingPredictionCaptureSampleData {
                context_task,
                editable_path: file.path().as_std_path().into(),
                editable_offset_range,
                next_edit_cursor_offset: None,
                future_edit_history_events: Vec::new(),
                navigation_history: VecDeque::new(),
                edit_events_before_quiescence: 0,
                prompt_history_boundary,
            })
        } else {
            None
        };
        project_state
            .pending_prediction_captures
            .push(PendingPredictionCapture {
                request_id,
                edited_buffer_id: edited_buffer.entity_id(),
                editable_anchor_range,
                editable_region_before_prediction,
                predicted_editable_region,
                ts_error_count_before_prediction,
                ts_error_count_after_prediction,
                organization_id,
                can_collect_data,
                is_in_open_source_repo,
                sample_data,
                model_version,
                e2e_latency,
                enqueued_at: now,
                last_edit_at: now,
            });
        this.settled_predictions_tx.unbounded_send(now).ok();
    }

    fn reject_current_prediction(
        &mut self,
        reason: EditPredictionRejectReason,
        project: &Entity<Project>,
        cx: &App,
    ) {
        if let Some(project_state) = self.projects.get_mut(&project.entity_id()) {
            project_state.pending_predictions.clear();
            if let Some(prediction) = project_state.current_prediction.take() {
                let model_version = prediction.prediction.model_version.clone();
                self.reject_prediction(
                    prediction.prediction.id,
                    reason,
                    prediction.was_shown,
                    model_version,
                    Some(prediction.e2e_latency),
                    cx,
                );
            }
        };
    }

    fn did_show_current_prediction(
        &mut self,
        project: &Entity<Project>,
        display_type: edit_prediction_types::SuggestionDisplayType,
        _cx: &mut Context<Self>,
    ) {
        let Some(project_state) = self.projects.get_mut(&project.entity_id()) else {
            return;
        };

        let Some(current_prediction) = project_state.current_prediction.as_mut() else {
            return;
        };

        let is_jump = display_type == edit_prediction_types::SuggestionDisplayType::Jump;
        let previous_shown_with = current_prediction.shown_with;

        if previous_shown_with.is_none() || !is_jump {
            current_prediction.shown_with = Some(display_type);
        }

        let is_first_non_jump_show = !current_prediction.was_shown && !is_jump;

        if is_first_non_jump_show {
            current_prediction.was_shown = true;
        }

        if is_first_non_jump_show {
            self.rateable_predictions
                .push_front(current_prediction.prediction.clone());
            if self.rateable_predictions.len() > 50 {
                let completion = self.rateable_predictions.pop_back().unwrap();
                self.rated_predictions.remove(&completion.id);
            }
        }
    }

    fn reject_prediction(
        &mut self,
        prediction_id: EditPredictionId,
        reason: EditPredictionRejectReason,
        was_shown: bool,
        model_version: Option<String>,
        e2e_latency: Option<std::time::Duration>,
        cx: &App,
    ) {
        match self.edit_prediction_model {
            EditPredictionModel::Zeta => {
                let is_cloud = !matches!(
                    all_language_settings(None, cx).edit_predictions.provider,
                    EditPredictionProvider::Ollama | EditPredictionProvider::OpenAiCompatibleApi
                );

                if is_cloud {
                    let organization_id = self
                        .user_store
                        .read(cx)
                        .current_organization()
                        .map(|organization| organization.id.clone());

                    self.reject_predictions_tx
                        .unbounded_send(EditPredictionRejectionPayload {
                            rejection: EditPredictionRejection {
                                request_id: prediction_id.to_string(),
                                reason,
                                was_shown,
                                model_version,
                                e2e_latency_ms: e2e_latency.map(|latency| latency.as_millis()),
                            },
                            organization_id,
                        })
                        .log_err();
                }
            }
            EditPredictionModel::Mercury => {
                mercury::edit_prediction_rejected(
                    prediction_id,
                    was_shown,
                    reason,
                    self.client.http_client(),
                    cx,
                );
            }
            EditPredictionModel::Fim { .. } => {}
        }
    }

    fn is_refreshing(&self, project: &Entity<Project>) -> bool {
        self.projects
            .get(&project.entity_id())
            .is_some_and(|project_state| !project_state.pending_predictions.is_empty())
    }
}
