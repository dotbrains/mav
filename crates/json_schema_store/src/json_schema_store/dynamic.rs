use super::*;

pub(super) async fn resolve_dynamic_schema(
    lsp_store: Entity<LspStore>,
    path: &str,
    cx: &mut AsyncApp,
) -> Result<serde_json::Value> {
    let languages = lsp_store.read_with(cx, |lsp_store, _| lsp_store.languages.clone());
    let (schema_name, rest) = path.split_once('/').unzip();
    let schema_name = schema_name.unwrap_or(path);

    let schema = match schema_name {
        "settings" if rest.is_some_and(|r| r.starts_with("lsp/")) => {
            let lsp_path = rest
                .and_then(|r| {
                    r.strip_prefix(
                        LSP_SETTINGS_SCHEMA_URL_PREFIX
                            .strip_prefix(SCHEMA_URI_PREFIX)
                            .and_then(|s| s.strip_prefix("settings/"))
                            .unwrap_or("lsp/"),
                    )
                })
                .context("Invalid LSP schema path")?;

            // Parse the schema type from the path:
            // - "rust-analyzer/initialization_options" → initialization_options_schema
            // - "rust-analyzer/settings" → settings_schema
            enum LspSchemaKind {
                InitializationOptions,
                Settings,
            }
            let (lsp_name, schema_kind) = if let Some(adapter_name) =
                lsp_path.strip_suffix("/initialization_options")
            {
                (adapter_name, LspSchemaKind::InitializationOptions)
            } else if let Some(adapter_name) = lsp_path.strip_suffix("/settings") {
                (adapter_name, LspSchemaKind::Settings)
            } else {
                anyhow::bail!(
                    "Invalid LSP schema path: \
                    Expected '{{adapter}}/initialization_options' or '{{adapter}}/settings', got '{}'",
                    lsp_path
                );
            };

            let adapter = languages
                .all_lsp_adapters()
                .into_iter()
                .find(|adapter| adapter.name().as_ref() as &str == lsp_name)
                .or_else(|| {
                    languages.load_available_lsp_adapter(&LanguageServerName::from(lsp_name))
                })
                .with_context(|| format!("LSP adapter not found: {}", lsp_name))?;

            let delegate: Arc<dyn LspAdapterDelegate> = cx
                .update(|inner_cx| {
                    lsp_store.update(inner_cx, |lsp_store, cx| {
                        let Some(local) = lsp_store.as_local() else {
                            return None;
                        };
                        let Some(worktree) = local.worktree_store.read(cx).worktrees().next()
                        else {
                            return None;
                        };
                        Some(LocalLspAdapterDelegate::from_local_lsp(
                            local, &worktree, cx,
                        ))
                    })
                })
                .context(concat!(
                    "Failed to create adapter delegate - ",
                    "either LSP store is not in local mode or no worktree is available"
                ))?;

            let schema = match schema_kind {
                LspSchemaKind::InitializationOptions => {
                    adapter.initialization_options_schema(&delegate, cx).await
                }
                LspSchemaKind::Settings => adapter.settings_schema(&delegate, cx).await,
            };

            schema.unwrap_or_else(|| {
                serde_json::json!({
                    "type": "object",
                    "additionalProperties": true
                })
            })
        }
        "settings" => {
            let mut lsp_adapter_names: Vec<String> = languages
                .all_lsp_adapters()
                .into_iter()
                .map(|adapter| adapter.name())
                .chain(languages.available_lsp_adapter_names())
                .map(|name| name.to_string())
                .collect();

            let mut i = 0;
            while i < lsp_adapter_names.len() {
                let mut j = i + 1;
                while j < lsp_adapter_names.len() {
                    if lsp_adapter_names[i] == lsp_adapter_names[j] {
                        lsp_adapter_names.swap_remove(j);
                    } else {
                        j += 1;
                    }
                }
                i += 1;
            }

            cx.update(|cx| {
                let font_names = &cx.text_system().all_font_names();
                let language_names = &languages
                    .language_names()
                    .into_iter()
                    .map(|name| name.to_string())
                    .collect::<Vec<_>>();

                let mut icon_theme_names = vec![];
                let mut theme_names = vec![];
                if let Some(registry) = theme::ThemeRegistry::try_global(cx) {
                    icon_theme_names.extend(
                        registry
                            .list_icon_themes()
                            .into_iter()
                            .map(|icon_theme| icon_theme.name),
                    );
                    theme_names.extend(registry.list_names());
                }
                let icon_theme_names = icon_theme_names.as_slice();
                let theme_names = theme_names.as_slice();

                let action_names = cx.all_action_names();
                let action_documentation = cx.action_documentation();
                let deprecations = cx.deprecated_actions_to_preferred_actions();
                let deprecation_messages = cx.action_deprecation_messages();

                let mut schema =
                    settings::SettingsStore::json_schema(&settings::SettingsJsonSchemaParams {
                        language_names,
                        font_names,
                        theme_names,
                        icon_theme_names,
                        lsp_adapter_names: &lsp_adapter_names,
                        action_names,
                        action_documentation,
                        deprecations,
                        deprecation_messages,
                    });
                inject_feature_flags_schema(&mut schema);
                schema
            })
        }
        "project_settings" => {
            let lsp_adapter_names = languages
                .all_lsp_adapters()
                .into_iter()
                .map(|adapter| adapter.name().to_string())
                .collect::<Vec<_>>();

            let language_names = &languages
                .language_names()
                .into_iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>();

            let mut schema =
                settings::SettingsStore::project_json_schema(&settings::SettingsJsonSchemaParams {
                    language_names,
                    lsp_adapter_names: &lsp_adapter_names,
                    // These are not allowed in project-specific settings but
                    // they're still fields required by the
                    // `SettingsJsonSchemaParams` struct.
                    font_names: &[],
                    theme_names: &[],
                    icon_theme_names: &[],
                    action_names: &[],
                    action_documentation: &HashMap::default(),
                    deprecations: &HashMap::default(),
                    deprecation_messages: &HashMap::default(),
                });
            inject_feature_flags_schema(&mut schema);
            schema
        }
        "debug_tasks" => {
            let adapter_schemas = cx.read_global::<dap::DapRegistry, _>(|dap_registry, _| {
                dap_registry.adapters_schema()
            });
            task::DebugTaskFile::generate_json_schema(&adapter_schemas)
        }
        "keymap" => cx.update(settings::KeymapFile::generate_json_schema_for_registered_actions),
        "action" => {
            let normalized_action_name = rest.context("No Action name provided")?;
            let action_name = denormalize_action_name(normalized_action_name);
            let mut generator = settings::KeymapFile::action_schema_generator();
            let schema = cx
                .update(|cx| cx.action_schema_by_name(&action_name, &mut generator))
                .flatten();
            root_schema_from_action_schema(schema, &mut generator).to_value()
        }
        "tasks" => task::TaskTemplates::generate_json_schema(),
        _ => {
            anyhow::bail!("Unrecognized schema: {schema_name}");
        }
    };
    Ok(schema)
}
