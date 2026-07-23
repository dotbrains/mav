use std::sync::{Arc, LazyLock};

use anyhow::{Context as _, Result};
use collections::HashMap;
use gpui::{App, AsyncApp, BorrowAppContext as _, Entity, Task, WeakEntity};
use language::{
    LanguageRegistry, LanguageServerName, LspAdapterDelegate,
    language_settings::AllLanguageSettings,
};
use parking_lot::RwLock;
use project::{LspStore, lsp_store::LocalLspAdapterDelegate};
use settings::{LSP_SETTINGS_SCHEMA_URL_PREFIX, Settings as _, SettingsLocation};
use util::schemars::{AllowTrailingCommas, DefaultDenyUnknownFields};

const SCHEMA_URI_PREFIX: &str = "mav://schemas/";

const TSCONFIG_SCHEMA: &str = include_str!("schemas/tsconfig.json");
const PACKAGE_JSON_SCHEMA: &str = include_str!("schemas/package.json");

static TASKS_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&task::TaskTemplates::generate_json_schema())
        .expect("TaskTemplates schema should serialize")
});

static SNIPPETS_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&snippet_provider::format::VsSnippetsFile::generate_json_schema())
        .expect("VsSnippetsFile schema should serialize")
});

static JSONC_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&generate_jsonc_schema()).expect("JSONC schema should serialize")
});

#[cfg(debug_assertions)]
static INSPECTOR_STYLE_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&generate_inspector_style_schema())
        .expect("Inspector style schema should serialize")
});

static KEYMAP_SCHEMA: LazyLock<String> = LazyLock::new(|| {
    serde_json::to_string(&settings::KeymapFile::generate_json_schema_from_inventory())
        .expect("Keymap schema should serialize")
});

static ACTION_SCHEMA_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::default()));

// Runtime cache for dynamic schemas that depend on runtime state:
// - "settings": depends on installed fonts, themes, languages, LSP adapters (extensions can add these)
// - "settings/lsp/*": depends on LSP adapter initialization options
// - "debug_tasks": depends on DAP adapters (extensions can add these)
// Cache is invalidated via notify_schema_changed() when extensions or DAP registry change.
static DYNAMIC_SCHEMA_CACHE: LazyLock<RwLock<HashMap<String, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::default()));

pub fn init(cx: &mut App) {
    cx.set_global(SchemaStore::default());
    project::lsp_store::json_language_server_ext::register_schema_handler(
        handle_schema_request,
        cx,
    );

    cx.observe_new(|_, _, cx| {
        let lsp_store = cx.weak_entity();
        cx.global_mut::<SchemaStore>().lsp_stores.push(lsp_store);
    })
    .detach();

    if let Some(extension_events) = extension::ExtensionEvents::try_global(cx) {
        cx.subscribe(&extension_events, move |_, evt, cx| match evt {
            extension::Event::ExtensionsInstalledChanged => {
                cx.update_global::<SchemaStore, _>(|schema_store, cx| {
                    schema_store.notify_schema_changed(ChangedSchemas::Settings, cx);
                });
            }
            extension::Event::ExtensionUninstalled(_)
            | extension::Event::ExtensionInstalled(_)
            | extension::Event::ConfigureExtensionRequested(_) => {}
        })
        .detach();
    }

    cx.observe_global::<dap::DapRegistry>(move |cx| {
        cx.update_global::<SchemaStore, _>(|schema_store, cx| {
            schema_store.notify_schema_changed(ChangedSchemas::DebugTasks, cx);
        });
    })
    .detach();
}

#[derive(Default)]
pub struct SchemaStore {
    lsp_stores: Vec<WeakEntity<LspStore>>,
}

impl gpui::Global for SchemaStore {}

enum ChangedSchemas {
    Settings,
    DebugTasks,
}

impl SchemaStore {
    fn notify_schema_changed(&mut self, changed_schemas: ChangedSchemas, cx: &mut App) {
        let uris_to_invalidate = match changed_schemas {
            ChangedSchemas::Settings => {
                let settings_uri_prefix = &format!("{SCHEMA_URI_PREFIX}settings");
                let project_settings_uri = &format!("{SCHEMA_URI_PREFIX}project_settings");
                DYNAMIC_SCHEMA_CACHE
                    .write()
                    .extract_if(|uri, _| {
                        uri == project_settings_uri || uri.starts_with(settings_uri_prefix)
                    })
                    .map(|(url, _)| url)
                    .collect()
            }
            ChangedSchemas::DebugTasks => DYNAMIC_SCHEMA_CACHE
                .write()
                .remove_entry(&format!("{SCHEMA_URI_PREFIX}debug_tasks"))
                .map_or_else(Vec::new, |(uri, _)| vec![uri]),
        };

        if uris_to_invalidate.is_empty() {
            return;
        }

        self.lsp_stores.retain(|lsp_store| {
            let Some(lsp_store) = lsp_store.upgrade() else {
                return false;
            };
            project::lsp_store::json_language_server_ext::notify_schemas_changed(
                lsp_store,
                &uris_to_invalidate,
                cx,
            );
            true
        })
    }
}

pub fn handle_schema_request(
    lsp_store: Entity<LspStore>,
    uri: String,
    cx: &mut AsyncApp,
) -> Task<Result<String>> {
    let path = match uri.strip_prefix(SCHEMA_URI_PREFIX) {
        Some(path) => path,
        None => return Task::ready(Err(anyhow::anyhow!("Invalid schema URI: {}", uri))),
    };

    if let Some(json) = resolve_static_schema(path) {
        return Task::ready(Ok(json));
    }

    if let Some(cached) = DYNAMIC_SCHEMA_CACHE.read().get(&uri).cloned() {
        return Task::ready(Ok(cached));
    }

    let path = path.to_string();
    let uri_clone = uri.clone();
    cx.spawn(async move |cx| {
        let schema = resolve_dynamic_schema(lsp_store, &path, cx).await?;
        let json = serde_json::to_string(&schema).context("Failed to serialize schema")?;

        DYNAMIC_SCHEMA_CACHE.write().insert(uri_clone, json.clone());

        Ok(json)
    })
}

fn resolve_static_schema(path: &str) -> Option<String> {
    let (schema_name, rest) = path.split_once('/').unzip();
    let schema_name = schema_name.unwrap_or(path);

    match schema_name {
        "tsconfig" => Some(TSCONFIG_SCHEMA.to_string()),
        "package_json" => Some(PACKAGE_JSON_SCHEMA.to_string()),
        "tasks" => Some(TASKS_SCHEMA.clone()),
        "snippets" => Some(SNIPPETS_SCHEMA.clone()),
        "jsonc" => Some(JSONC_SCHEMA.clone()),
        "keymap" => Some(KEYMAP_SCHEMA.clone()),
        "mav_inspector_style" => {
            #[cfg(debug_assertions)]
            {
                Some(INSPECTOR_STYLE_SCHEMA.clone())
            }
            #[cfg(not(debug_assertions))]
            {
                Some(
                    serde_json::to_string(&schemars::json_schema!(true).to_value())
                        .expect("true schema should serialize"),
                )
            }
        }

        "action" => {
            let normalized_action_name = match rest {
                Some(name) => name,
                None => return None,
            };
            let action_name = denormalize_action_name(normalized_action_name);

            if let Some(cached) = ACTION_SCHEMA_CACHE.read().get(&action_name).cloned() {
                return Some(cached);
            }

            let mut generator = settings::KeymapFile::action_schema_generator();
            let schema =
                settings::KeymapFile::get_action_schema_by_name(&action_name, &mut generator);
            let json = serde_json::to_string(
                &root_schema_from_action_schema(schema, &mut generator).to_value(),
            )
            .expect("Action schema should serialize");

            ACTION_SCHEMA_CACHE
                .write()
                .insert(action_name, json.clone());
            Some(json)
        }

        _ => None,
    }
}

#[path = "json_schema_store/dynamic.rs"]
mod dynamic;

use dynamic::resolve_dynamic_schema;

const JSONC_LANGUAGE_NAME: &str = "JSONC";

pub fn all_schema_file_associations(
    languages: &Arc<LanguageRegistry>,
    path: Option<SettingsLocation<'_>>,
    cx: &mut App,
) -> serde_json::Value {
    let extension_globs = languages
        .available_language_for_name(JSONC_LANGUAGE_NAME)
        .map(|language| language.matcher().path_suffixes.clone())
        .into_iter()
        .flatten()
        // Path suffixes can be entire file names or just their extensions.
        .flat_map(|path_suffix| [format!("*.{path_suffix}"), path_suffix]);
    let override_globs = AllLanguageSettings::get(path, cx)
        .file_types
        .get(JSONC_LANGUAGE_NAME)
        .into_iter()
        .flat_map(|(_, glob_strings)| glob_strings)
        .cloned();
    let jsonc_globs = extension_globs.chain(override_globs).collect::<Vec<_>>();

    let mut file_associations = serde_json::json!([
        {
            "fileMatch": [
                schema_file_match(paths::settings_file()),
            ],
            "url": format!("{SCHEMA_URI_PREFIX}settings"),
        },
        {
            "fileMatch": [
            paths::local_settings_file_relative_path()],
            "url": format!("{SCHEMA_URI_PREFIX}project_settings"),
        },
        {
            "fileMatch": [schema_file_match(paths::keymap_file())],
            "url": format!("{SCHEMA_URI_PREFIX}keymap"),
        },
        {
            "fileMatch": [
                schema_file_match(paths::tasks_file()),
                paths::local_tasks_file_relative_path()
            ],
            "url": format!("{SCHEMA_URI_PREFIX}tasks"),
        },
        {
            "fileMatch": [
                schema_file_match(paths::debug_scenarios_file()),
                paths::local_debug_file_relative_path()
            ],
            "url": format!("{SCHEMA_URI_PREFIX}debug_tasks"),
        },
        {
            "fileMatch": [
                schema_file_match(
                    paths::snippets_dir()
                        .join("*.json")
                        .as_path()
                )
            ],
            "url": format!("{SCHEMA_URI_PREFIX}snippets"),
        },
        {
            "fileMatch": ["tsconfig.json"],
            "url": format!("{SCHEMA_URI_PREFIX}tsconfig")
        },
        {
            "fileMatch": ["package.json"],
            "url": format!("{SCHEMA_URI_PREFIX}package_json")
        },
        {
            "fileMatch": &jsonc_globs,
            "url": format!("{SCHEMA_URI_PREFIX}jsonc")
        },
    ]);

    #[cfg(debug_assertions)]
    {
        file_associations
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "fileMatch": [
                    "mav-inspector-style.json"
                ],
                "url": format!("{SCHEMA_URI_PREFIX}mav_inspector_style")
            }));
    }

    file_associations
        .as_array_mut()
        .unwrap()
        .extend(cx.all_action_names().into_iter().map(|&name| {
            let normalized_name = normalize_action_name(name);
            let file_name = normalized_action_name_to_file_name(normalized_name.clone());
            serde_json::json!({
                "fileMatch": [file_name],
                "url": format!("{SCHEMA_URI_PREFIX}action/{normalized_name}")
            })
        }));

    file_associations
}

/// Swaps the placeholder [`settings::FeatureFlagsMap`] subschema produced by
/// schemars for an enriched one that lists each known flag's variants. The
/// placeholder is registered in the `settings_content` crate so the
/// `settings` crate doesn't need a reverse dependency on `feature_flags`.
fn inject_feature_flags_schema(schema: &mut serde_json::Value) {
    use schemars::JsonSchema;

    let Some(defs) = schema.get_mut("$defs").and_then(|d| d.as_object_mut()) else {
        return;
    };
    let schema_name = settings::FeatureFlagsMap::schema_name();
    let enriched = feature_flags::generate_feature_flags_schema().to_value();
    defs.insert(schema_name.into_owned(), enriched);
}

fn generate_jsonc_schema() -> serde_json::Value {
    let generator = schemars::generate::SchemaSettings::draft2019_09()
        .with_transform(DefaultDenyUnknownFields)
        .with_transform(AllowTrailingCommas)
        .into_generator();
    let meta_schema = generator
        .settings()
        .meta_schema
        .as_ref()
        .expect("meta_schema should be present in schemars settings")
        .to_string();
    let defs = generator.definitions();
    let schema = schemars::json_schema!({
        "$schema": meta_schema,
        "allowTrailingCommas": true,
        "$defs": defs,
    });
    serde_json::to_value(schema).unwrap()
}

#[cfg(debug_assertions)]
fn generate_inspector_style_schema() -> serde_json::Value {
    let schema = schemars::generate::SchemaSettings::draft2019_09()
        .with_transform(util::schemars::DefaultDenyUnknownFields)
        .into_generator()
        .root_schema_for::<gpui::StyleRefinement>();

    serde_json::to_value(schema).unwrap()
}

pub fn normalize_action_name(action_name: &str) -> String {
    action_name.replace("::", "__")
}

pub fn denormalize_action_name(action_name: &str) -> String {
    action_name.replace("__", "::")
}

pub fn normalized_action_file_name(action_name: &str) -> String {
    normalized_action_name_to_file_name(normalize_action_name(action_name))
}

pub fn normalized_action_name_to_file_name(mut normalized_action_name: String) -> String {
    normalized_action_name.push_str(".json");
    normalized_action_name
}

fn root_schema_from_action_schema(
    action_schema: Option<schemars::Schema>,
    generator: &mut schemars::SchemaGenerator,
) -> schemars::Schema {
    let Some(mut action_schema) = action_schema else {
        return schemars::json_schema!(false);
    };
    let meta_schema = generator
        .settings()
        .meta_schema
        .as_ref()
        .expect("meta_schema should be present in schemars settings")
        .to_string();
    let defs = generator.definitions();
    let mut schema = schemars::json_schema!({
        "$schema": meta_schema,
        "allowTrailingCommas": true,
        "$defs": defs,
    });
    schema
        .ensure_object()
        .extend(std::mem::take(action_schema.ensure_object()));
    schema
}

#[inline]
fn schema_file_match(path: &std::path::Path) -> String {
    path.strip_prefix(path.parent().unwrap().parent().unwrap())
        .unwrap()
        .display()
        .to_string()
        .replace('\\', "/")
}
