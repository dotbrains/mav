use super::*;

async fn send_settled_batches(
    client: Arc<Client>,
    llm_token: LlmApiToken,
    app_version: Version,
    ready_predictions_by_organization_id: hash_map::HashMap<
        Option<OrganizationId>,
        Vec<(PendingPredictionCapture, String)>,
        collections::FxBuildHasher,
    >,
) {
    let Some(url) = client
        .http_client()
        .build_mav_llm_url("/predict_edits/settled", &[])
        .context("failed to build edit predictions settled url")
        .log_err()
    else {
        return;
    };

    for (organization_id, ready_predictions) in ready_predictions_by_organization_id {
        let mut ready_predictions = ready_predictions.into_iter();
        loop {
            let done_batch = ready_predictions
                .by_ref()
                .take(MAX_EDIT_PREDICTION_SETTLED_PER_REQUEST);
            let mut batch = Vec::with_capacity(MAX_EDIT_PREDICTION_SETTLED_PER_REQUEST);
            for (pending_capture, settled_editable_region) in done_batch {
                let PendingPredictionCapture {
                    request_id,
                    editable_region_before_prediction,
                    predicted_editable_region,
                    ts_error_count_before_prediction,
                    ts_error_count_after_prediction,
                    can_collect_data,
                    is_in_open_source_repo,
                    sample_data,
                    model_version,
                    e2e_latency,
                    ..
                } = pending_capture;
                let kept_rate_result = compute_kept_rate(
                    &editable_region_before_prediction,
                    &predicted_editable_region,
                    &settled_editable_region,
                );

                let sample_data = if can_collect_data
                    && let Some(sample_data) = sample_data
                    && let Ok(context) = sample_data.context_task.await
                {
                    Some(SettledEditPredictionSampleData {
                        repository_url: context.repository_url,
                        revision: context.revision,
                        uncommitted_diff: context.uncommitted_diff,
                        editable_path: sample_data.editable_path,
                        editable_offset_range: sample_data.editable_offset_range,
                        buffer_diagnostics: context.buffer_diagnostics,
                        editable_context: context.editable_context,
                        future_edit_history_events: sample_data.future_edit_history_events,
                        navigation_history: sample_data
                            .navigation_history
                            .into_iter()
                            .map(|file| EditPredictionRecentFile {
                                path: file.path,
                                cursor_position: file.cursor_position,
                            })
                            .collect(),
                        edit_events_before_quiescence: sample_data.edit_events_before_quiescence,
                        next_edit_cursor_offset: sample_data.next_edit_cursor_offset,
                    })
                } else {
                    None
                };

                batch.push(SettledEditPrediction {
                    request_id: request_id.0.to_string(),
                    settled_editable_region: can_collect_data.then_some(settled_editable_region),
                    ts_error_count_before_prediction,
                    ts_error_count_after_prediction,
                    can_collect_data,
                    is_in_open_source_repo,
                    sample_data,
                    kept_chars: EditPredictionSettledKeptChars {
                        candidate_new: kept_rate_result.candidate_new_chars,
                        reference_new: kept_rate_result.reference_new_chars,
                        candidate_deleted: kept_rate_result.candidate_deleted_chars,
                        reference_deleted: kept_rate_result.reference_deleted_chars,
                        kept: kept_rate_result.kept_chars,
                        correctly_deleted: kept_rate_result.correctly_deleted_chars,
                        discarded: kept_rate_result.discarded_chars,
                        context: kept_rate_result.context_chars,
                        kept_rate: kept_rate_result.kept_rate,
                        recall_rate: kept_rate_result.recall_rate,
                    },
                    example: None,
                    model_version,
                    e2e_latency_ms: e2e_latency.as_millis().min(u128::from(u64::MAX)) as u64,
                });
            }

            if batch.is_empty() {
                break;
            }

            let result = async {
                let body = SubmitEditPredictionSettledBatchBody { predictions: batch };
                let compressed = zstd::encode_all(&serde_json::to_vec(&body)?[..], 3)?;
                EditPredictionStore::send_api_request::<SubmitEditPredictionSettledResponse>(
                    |builder| {
                        Ok(builder
                            .uri(url.as_ref())
                            .header("Content-Encoding", "zstd")
                            .body(compressed.clone().into())?)
                    },
                    client.clone(),
                    llm_token.clone(),
                    organization_id.clone(),
                    app_version.clone(),
                )
                .await?;
                anyhow::Ok(())
            }
            .await;

            if let Err(error) = result {
                log::error!("failed to submit edit predictions settled: {error:?}");
            }
        }
    }
}

fn currently_following(project: &Entity<Project>, cx: &App) -> bool {
    let Some(app_state) = AppState::try_global(cx) else {
        return false;
    };

    app_state
        .workspace_store
        .read(cx)
        .workspaces()
        .filter_map(|workspace| workspace.upgrade())
        .any(|workspace| {
            workspace.read(cx).project().entity_id() == project.entity_id()
                && workspace
                    .read(cx)
                    .leader_for_pane(workspace.read(cx).active_pane())
                    .is_some()
        })
}

fn is_ep_store_provider(provider: EditPredictionProvider) -> bool {
    match provider {
        EditPredictionProvider::Mav
        | EditPredictionProvider::Mercury
        | EditPredictionProvider::Ollama
        | EditPredictionProvider::OpenAiCompatibleApi => true,
        EditPredictionProvider::None
        | EditPredictionProvider::Copilot
        | EditPredictionProvider::Codestral => false,
    }
}
