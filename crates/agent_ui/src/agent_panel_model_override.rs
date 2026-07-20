use agent::Thread;
use gpui::{App, Entity};
use language_model::LanguageModelRegistry;

/// Apply a `provider/model-id` model override to a freshly-created native thread.
/// Best-effort: logs an error and leaves the default model in place if the
/// string can't be parsed or the model isn't registered.
pub(crate) fn apply_native_model_override(thread: &Entity<Thread>, model_id: &str, cx: &mut App) {
    let Some(selected) = parse_provider_slash_model(model_id) else {
        log::warn!(
            "create_thread: could not parse model override {model_id:?}; expected `provider/model-id`"
        );
        return;
    };
    let configured = LanguageModelRegistry::global(cx)
        .update(cx, |registry, cx| registry.select_model(&selected, cx));
    let Some(configured) = configured else {
        log::warn!(
            "create_thread: no model registered for {model_id:?}; using thread's default model"
        );
        return;
    };
    thread.update(cx, |thread, cx| {
        thread.set_model(configured.model, cx);
    });
}

fn parse_provider_slash_model(input: &str) -> Option<language_model::SelectedModel> {
    let (provider, model) = input.split_once('/')?;
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some(language_model::SelectedModel {
        provider: language_model::LanguageModelProviderId::from(provider.to_string()),
        model: language_model::LanguageModelId::from(model.to_string()),
    })
}
