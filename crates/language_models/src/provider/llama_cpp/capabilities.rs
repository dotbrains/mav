use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LiveCapabilities {
    pub(super) max_tokens: u64,
    pub(super) supports_tools: bool,
    pub(super) supports_thinking: bool,
}

impl LiveCapabilities {
    pub(super) fn of(model: &llama_cpp::Model) -> Self {
        Self {
            max_tokens: model.max_tokens,
            supports_tools: model.supports_tools,
            supports_thinking: model.supports_thinking,
        }
    }
}

/// Live capabilities keyed by model name, shared by the provider and its models.
pub(super) type CapabilityCells = Arc<RwLock<HashMap<String, LiveCapabilities>>>;

/// Model name → load-status label (e.g. `"Loading weights 42%"`) while a router
/// model loads, shared so the model selector can show progress. Absent once loaded.
pub(super) type LoadingProgress = Arc<RwLock<HashMap<String, SharedString>>>;

/// Locks for reading, recovering instead of panicking on a poisoned lock. The
/// critical sections are infallible map ops, so poisoning is unreachable anyway.
pub(super) fn read_recover<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Locks for writing; see [`read_recover`].
pub(super) fn write_recover<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The exact set of models `provided_models` exposes: discovery merged with the
/// `available_models` and `context_window` overrides. Shared with re-discovery.
pub(super) fn compute_effective_models(
    fetched_models: &[llama_cpp::Model],
    settings: &LlamaCppSettings,
) -> HashMap<String, llama_cpp::Model> {
    let mut models: HashMap<String, llama_cpp::Model> = HashMap::default();
    if settings.auto_discover {
        for model in fetched_models {
            let mut model = model.clone();
            if let Some(context_window) = settings.context_window {
                model.max_tokens = context_window;
            }
            models.insert(model.name.clone(), model);
        }
    }
    merge_settings_into_models(
        &mut models,
        &settings.available_models,
        settings.context_window,
    );
    models
}

/// Updates the shared capability map from the effective models, so a model held
/// by an open conversation observes the new values (it reads the map by name).
pub(super) fn sync_capability_cells(
    cells: &CapabilityCells,
    effective: &HashMap<String, llama_cpp::Model>,
) {
    let mut cells = write_recover(cells);
    for model in effective.values() {
        cells.insert(model.name.clone(), LiveCapabilities::of(model));
    }
}

/// Builds a model from a `/v1/models` entry, refined by `/props` when the model
/// is loaded. An unloaded router model can't be probed, so we assume optimistic
/// capabilities and let re-discovery refine them on load.
pub(super) fn model_from_entry(entry: &ModelEntry, props: Option<&Props>) -> llama_cpp::Model {
    let max_tokens = props
        .and_then(Props::context_length)
        .or_else(|| entry.meta.as_ref().and_then(|meta| meta.n_ctx))
        .or_else(|| entry.meta.as_ref().and_then(|meta| meta.n_ctx_train))
        .unwrap_or(ASSUMED_UNLOADED_CONTEXT);
    // Trust `/props` when present. Without it, assume tools for an unloaded model
    // (re-discovery corrects on load) but not for a loaded model whose probe failed.
    let supports_tools = match props {
        Some(props) => props.supports_tools(),
        None => !entry.is_loaded(),
    };
    let supports_images = props.is_some_and(Props::supports_images) || entry.supports_images_hint();
    let supports_thinking = props.is_some_and(Props::supports_thinking);

    llama_cpp::Model::new(
        &entry.id,
        Some(&display_name_for(&entry.id)),
        Some(max_tokens),
        supports_tools,
        supports_images,
        supports_thinking,
    )
}

/// Friendly display name from a model id, which is often a `.gguf` file path.
pub(super) fn display_name_for(id: &str) -> String {
    let base = id.rsplit(['/', '\\']).next().unwrap_or(id);
    base.strip_suffix(".gguf").unwrap_or(base).to_string()
}

pub(super) fn telemetry_id_for(id: &str) -> String {
    format!("{PROVIDER_ID}/{}", display_name_for(id))
}
