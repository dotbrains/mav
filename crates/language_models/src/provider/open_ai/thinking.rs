use super::*;

pub(super) fn default_thinking_reasoning_effort(
    model: &open_ai::Model,
) -> Option<open_ai::ReasoningEffort> {
    use open_ai::ReasoningEffort;

    model
        .reasoning_effort()
        .filter(|effort| open_ai_reasoning_effort_is_supported(*effort))
        .or_else(|| {
            let supported_efforts = model.supported_reasoning_efforts();
            if supported_efforts.contains(&ReasoningEffort::Medium) {
                Some(ReasoningEffort::Medium)
            } else {
                supported_efforts
                    .iter()
                    .copied()
                    .find(|effort| open_ai_reasoning_effort_is_supported(*effort))
            }
        })
}

fn open_ai_reasoning_effort_is_supported(effort: open_ai::ReasoningEffort) -> bool {
    effort != open_ai::ReasoningEffort::None
}

pub(super) fn normalize_open_ai_response_thinking_effort(
    request: &mut LanguageModelRequest,
    model: &open_ai::Model,
) {
    let selected_effort_is_supported = request
        .thinking_effort
        .as_deref()
        .and_then(|effort| effort.parse::<open_ai::ReasoningEffort>().ok())
        .is_some_and(|effort| {
            open_ai_reasoning_effort_is_supported(effort)
                && model.supported_reasoning_efforts().contains(&effort)
        });

    if !selected_effort_is_supported {
        request.thinking_effort = None;
    }
}

pub(super) fn supports_selectable_thinking_effort(model: &open_ai::Model) -> bool {
    model.uses_responses_api()
        && model
            .supported_reasoning_efforts()
            .iter()
            .any(|effort| open_ai_reasoning_effort_is_supported(*effort))
}

pub(super) fn supported_thinking_effort_levels(
    model: &open_ai::Model,
) -> Vec<LanguageModelEffortLevel> {
    if !supports_selectable_thinking_effort(model) {
        return Vec::new();
    }

    let default_effort = default_thinking_reasoning_effort(model);
    model
        .supported_reasoning_efforts()
        .iter()
        .copied()
        .filter_map(|effort| {
            if !open_ai_reasoning_effort_is_supported(effort) {
                return None;
            }

            Some(LanguageModelEffortLevel {
                name: effort.label().into(),
                value: effort.value().into(),
                is_default: Some(effort) == default_effort,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_thinking_effort_levels_hide_none() {
        let effort_levels = supported_thinking_effort_levels(&open_ai::Model::FivePointTwo);
        let values = effort_levels
            .iter()
            .map(|level| level.value.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(values, ["low", "medium", "high", "xhigh"]);
        assert_eq!(
            effort_levels
                .iter()
                .find(|level| level.is_default)
                .map(|level| level.value.as_ref()),
            Some("medium")
        );
    }

    #[test]
    fn models_supporting_only_none_have_no_selectable_thinking_effort() {
        let model = open_ai::Model::Custom {
            name: "custom-model".to_string(),
            display_name: None,
            max_tokens: 128_000,
            max_output_tokens: None,
            max_completion_tokens: None,
            reasoning_effort: Some(open_ai::ReasoningEffort::None),
            supports_chat_completions: false,
            supports_images: true,
        };

        assert!(!supports_selectable_thinking_effort(&model));
        assert!(supported_thinking_effort_levels(&model).is_empty());
        assert!(
            model
                .supported_reasoning_efforts()
                .contains(&open_ai::ReasoningEffort::None)
        );
    }
}
