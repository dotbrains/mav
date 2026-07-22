use super::*;

pub(super) fn merge_settings_into_models(
    models: &mut HashMap<String, llama_cpp::Model>,
    available_models: &[AvailableModel],
    context_window: Option<u64>,
) {
    for setting_model in available_models {
        if let Some(model) = models.get_mut(&setting_model.name) {
            if context_window.is_none() {
                model.max_tokens = setting_model.max_tokens;
            }
            if setting_model.display_name.is_some() {
                model.display_name = setting_model.display_name.clone();
            }
            if let Some(supports_tools) = setting_model.supports_tools {
                model.supports_tools = supports_tools;
            }
            if let Some(supports_images) = setting_model.supports_images {
                model.supports_images = supports_images;
            }
            if let Some(supports_thinking) = setting_model.supports_thinking {
                model.supports_thinking = supports_thinking;
            }
        } else {
            models.insert(
                setting_model.name.clone(),
                llama_cpp::Model {
                    name: setting_model.name.clone(),
                    display_name: setting_model.display_name.clone(),
                    max_tokens: context_window.unwrap_or(setting_model.max_tokens),
                    supports_tools: setting_model.supports_tools.unwrap_or(false),
                    supports_images: setting_model.supports_images.unwrap_or(false),
                    supports_thinking: setting_model.supports_thinking.unwrap_or(false),
                },
            );
        }
    }
}
