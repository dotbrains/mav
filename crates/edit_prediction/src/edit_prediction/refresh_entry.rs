use super::*;

impl EditPredictionStore {
    pub fn refresh_prediction_from_buffer(
        &mut self,
        project: Entity<Project>,
        buffer: Entity<Buffer>,
        position: language::Anchor,
        trigger: EditPredictionRequestTrigger,
        cx: &mut Context<Self>,
    ) {
        if currently_following(&project, cx) {
            return;
        }

        let trigger = predict_edits_request_trigger_from_editor_trigger(trigger);

        self.queue_prediction_refresh(project.clone(), buffer.entity_id(), cx, move |this, cx| {
            let Some(request_task) = this
                .update(cx, |this, cx| {
                    this.request_prediction_internal(
                        project.clone(),
                        buffer.clone(),
                        position,
                        trigger,
                        cx,
                    )
                })
                .log_err()
            else {
                return Task::ready(anyhow::Ok(None));
            };

            cx.spawn(async move |_cx| {
                request_task.await.map(|prediction_result| {
                    prediction_result
                        .map(|prediction_result| (prediction_result, buffer.entity_id()))
                })
            })
        })
    }

    pub const THROTTLE_TIMEOUT: Duration = Duration::from_millis(300);
}
