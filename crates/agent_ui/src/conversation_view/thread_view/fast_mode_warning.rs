use super::*;
use db::kvp::KeyValueStore;

const FAST_MODE_WARNING_NAMESPACE: &str = "fast-mode-warning-dismissed";

fn fast_mode_warning_id(
    provider_id: &LanguageModelProviderId,
    model_id: &LanguageModelId,
) -> String {
    format!("{}:{}", provider_id.0, model_id.0)
}

pub(super) fn fast_mode_warning_dismissed(
    provider_id: &LanguageModelProviderId,
    model_id: &LanguageModelId,
    cx: &App,
) -> bool {
    KeyValueStore::global(cx)
        .scoped(FAST_MODE_WARNING_NAMESPACE)
        .read(&fast_mode_warning_id(provider_id, model_id))
        .log_err()
        .flatten()
        .is_some()
}

pub(super) fn set_fast_mode_warning_dismissed(
    provider_id: &LanguageModelProviderId,
    model_id: &LanguageModelId,
    cx: &mut App,
) {
    let key = fast_mode_warning_id(provider_id, model_id);
    let kvp = KeyValueStore::global(cx);
    cx.background_spawn(async move {
        kvp.scoped(FAST_MODE_WARNING_NAMESPACE)
            .write(key, "1".to_string())
            .await
            .log_err();
    })
    .detach();
}

pub(crate) fn reset_fast_mode_warnings(cx: &mut App) {
    let kvp = KeyValueStore::global(cx);
    cx.background_spawn(async move {
        kvp.scoped(FAST_MODE_WARNING_NAMESPACE)
            .delete_all()
            .await
            .log_err();
    })
    .detach();
}
