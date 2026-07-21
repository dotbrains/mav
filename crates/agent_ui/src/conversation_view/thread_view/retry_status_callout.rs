use super::*;

impl ThreadView {
    pub(super) fn callout_border_position(&self) -> CalloutBorderPosition {
        if self.list_state.item_count() > 0 {
            CalloutBorderPosition::Top
        } else {
            CalloutBorderPosition::Bottom
        }
    }

    pub fn render_thread_retry_status_callout(&self, cx: &mut Context<Self>) -> Option<Callout> {
        let state = self.thread_retry_status.as_ref()?;

        if let Some(fallback_model) = acp_thread::refusal_fallback_model_from_meta(&state.meta) {
            return Some(
                Callout::new()
                    .icon(IconName::Warning)
                    .severity(Severity::Warning)
                    .title(state.last_error.clone())
                    .description(format!("Retrying with {fallback_model}"))
                    .dismiss_action(
                        IconButton::new("dismiss-refusal-fallback", IconName::Close)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Dismiss"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.thread_retry_status = None;
                                cx.notify();
                            })),
                    ),
            );
        }

        let next_attempt_in = state
            .duration
            .saturating_sub(Instant::now().saturating_duration_since(state.started_at));
        if next_attempt_in.is_zero() {
            return None;
        }

        let next_attempt_in_secs = next_attempt_in.as_secs() + 1;

        let retry_message = if state.max_attempts == 1 {
            if next_attempt_in_secs == 1 {
                "Retrying. Next attempt in 1 second.".to_string()
            } else {
                format!("Retrying. Next attempt in {next_attempt_in_secs} seconds.")
            }
        } else if next_attempt_in_secs == 1 {
            format!(
                "Retrying. Next attempt in 1 second (Attempt {} of {}).",
                state.attempt, state.max_attempts,
            )
        } else {
            format!(
                "Retrying. Next attempt in {next_attempt_in_secs} seconds (Attempt {} of {}).",
                state.attempt, state.max_attempts,
            )
        };

        Some(
            Callout::new()
                .border_position(self.callout_border_position())
                .icon(IconName::Warning)
                .severity(Severity::Warning)
                .title(state.last_error.clone())
                .description(retry_message),
        )
    }
}
