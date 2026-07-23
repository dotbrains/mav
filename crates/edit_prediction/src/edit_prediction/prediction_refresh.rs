use super::*;

impl EditPredictionStore {
    pub(crate) fn queue_prediction_refresh(
        &mut self,
        project: Entity<Project>,
        throttle_entity: EntityId,
        cx: &mut Context<Self>,
        do_refresh: impl FnOnce(
            WeakEntity<Self>,
            &mut AsyncApp,
        ) -> Task<Result<Option<(EditPredictionResult, EntityId)>>>
        + 'static,
    ) {
        let (needs_acceptance_tracking, max_pending_predictions) =
            match all_language_settings(None, cx).edit_predictions.provider {
                EditPredictionProvider::Mav | EditPredictionProvider::Mercury => (true, 2),
                EditPredictionProvider::Ollama => (false, 1),
                EditPredictionProvider::OpenAiCompatibleApi => (false, 2),
                EditPredictionProvider::None
                | EditPredictionProvider::Copilot
                | EditPredictionProvider::Codestral => {
                    log::error!("queue_prediction_refresh called with non-store provider");
                    return;
                }
            };

        let drop_on_cancel = !needs_acceptance_tracking;
        let throttle_timeout = Self::THROTTLE_TIMEOUT;
        let project_state = self.get_or_init_project(&project, cx);
        let pending_prediction_id = project_state.next_pending_prediction_id;
        project_state.next_pending_prediction_id += 1;
        let throttle_at_enqueue = project_state.last_edit_prediction_refresh;

        let task = cx.spawn(async move |this, cx| {
            let throttle_wait = this
                .update(cx, |this, cx| {
                    let project_state = this.get_or_init_project(&project, cx);
                    let throttle = project_state.last_edit_prediction_refresh;

                    let now = cx.background_executor().now();
                    throttle.and_then(|(last_entity, last_timestamp)| {
                        if throttle_entity != last_entity {
                            return None;
                        }
                        (last_timestamp + throttle_timeout).checked_duration_since(now)
                    })
                })
                .ok()
                .flatten();

            if let Some(timeout) = throttle_wait {
                cx.background_executor().timer(timeout).await;
            }

            // If this task was cancelled before the throttle timeout expired,
            // do not perform a request. Also skip if another task already
            // proceeded since we were enqueued (duplicate).
            let mut is_cancelled = true;
            this.update(cx, |this, cx| {
                let project_state = this.get_or_init_project(&project, cx);
                let was_cancelled = project_state
                    .cancelled_predictions
                    .remove(&pending_prediction_id);
                if was_cancelled {
                    return;
                }

                // Another request has been already sent since this was enqueued
                if project_state.last_edit_prediction_refresh != throttle_at_enqueue {
                    return;
                }

                let new_refresh = (throttle_entity, cx.background_executor().now());
                project_state.last_edit_prediction_refresh = Some(new_refresh);
                is_cancelled = false;
            })
            .ok();
            if is_cancelled {
                return None;
            }

            let new_prediction_result = do_refresh(this.clone(), cx).await.log_err().flatten();
            let new_prediction_metadata = new_prediction_result.as_ref().map(|(result, _)| {
                (
                    result.prediction.id.clone(),
                    result.prediction.model_version.clone(),
                )
            });

            // When a prediction completes, remove it from the pending list, and cancel
            // any pending predictions that were enqueued before it.
            this.update(cx, |this, cx| {
                let project_state = this.get_or_init_project(&project, cx);

                let is_cancelled = project_state
                    .cancelled_predictions
                    .remove(&pending_prediction_id);

                let new_current_prediction = if !is_cancelled
                    && let Some((prediction_result, requested_by)) = new_prediction_result
                {
                    let EditPredictionResult {
                        prediction,
                        reject_reason,
                        e2e_latency,
                    } = prediction_result;

                    if let Some(reject_reason) = reject_reason {
                        let should_allow_rating_prediction = matches!(
                            reject_reason,
                            EditPredictionRejectReason::Empty
                                | EditPredictionRejectReason::InterpolatedEmpty
                        );
                        let prediction_id = prediction.id.clone();
                        let model_version = prediction.model_version.clone();

                        this.reject_prediction(
                            prediction_id,
                            reject_reason,
                            false,
                            model_version,
                            Some(e2e_latency),
                            cx,
                        );

                        if should_allow_rating_prediction {
                            this.rateable_predictions.push_front(prediction);
                            if this.rateable_predictions.len() > 50
                                && let Some(completion) = this.rateable_predictions.pop_back()
                            {
                                this.rated_predictions.remove(&completion.id);
                            }
                        }

                        None
                    } else {
                        let new_prediction = CurrentEditPrediction {
                            requested_by,
                            prediction,
                            was_shown: false,
                            shown_with: None,
                            e2e_latency,
                        };

                        if let Some(current_prediction) = project_state.current_prediction.as_ref()
                        {
                            if new_prediction.should_replace_prediction(&current_prediction, cx) {
                                this.reject_current_prediction(
                                    EditPredictionRejectReason::Replaced,
                                    &project,
                                    cx,
                                );

                                Some(new_prediction)
                            } else {
                                this.reject_prediction(
                                    new_prediction.prediction.id,
                                    EditPredictionRejectReason::CurrentPreferred,
                                    false,
                                    new_prediction.prediction.model_version,
                                    Some(new_prediction.e2e_latency),
                                    cx,
                                );
                                None
                            }
                        } else {
                            Some(new_prediction)
                        }
                    }
                } else {
                    None
                };

                let project_state = this.get_or_init_project(&project, cx);

                if let Some(new_prediction) = new_current_prediction {
                    project_state.current_prediction = Some(new_prediction);
                }

                let mut pending_predictions = mem::take(&mut project_state.pending_predictions);
                for (ix, pending_prediction) in pending_predictions.iter().enumerate() {
                    if pending_prediction.id == pending_prediction_id {
                        pending_predictions.remove(ix);
                        for pending_prediction in pending_predictions.drain(0..ix) {
                            project_state.cancel_pending_prediction(pending_prediction, cx)
                        }
                        break;
                    }
                }
                this.get_or_init_project(&project, cx).pending_predictions = pending_predictions;
                cx.notify();
            })
            .ok();

            new_prediction_metadata
        });

        if project_state.pending_predictions.len() < max_pending_predictions {
            project_state
                .pending_predictions
                .push(PendingPrediction {
                    id: pending_prediction_id,
                    task,
                    drop_on_cancel,
                })
                .unwrap();
        } else {
            let pending_prediction = project_state.pending_predictions.pop().unwrap();
            project_state
                .pending_predictions
                .push(PendingPrediction {
                    id: pending_prediction_id,
                    task,
                    drop_on_cancel,
                })
                .unwrap();
            project_state.cancel_pending_prediction(pending_prediction, cx);
        }
    }
}
