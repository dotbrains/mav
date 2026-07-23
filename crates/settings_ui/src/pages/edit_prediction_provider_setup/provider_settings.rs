use super::*;

pub(super) fn ollama_settings() -> Box<[SettingsPageItem]> {
    Box::new([
        SettingsPageItem::SettingItem(SettingItem {
            title: "API URL",
            description: "The base URL of your Ollama server.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .ollama
                        .as_ref()?
                        .api_url
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .ollama
                        .get_or_insert_default()
                        .api_url = value;
                },
                json_path: Some("edit_predictions.ollama.api_url"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some(OLLAMA_API_URL_PLACEHOLDER),
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Model",
            description: "The Ollama model to use for edit predictions.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .ollama
                        .as_ref()?
                        .model
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .ollama
                        .get_or_insert_default()
                        .model = value;
                },
                json_path: Some("edit_predictions.ollama.model"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some(OLLAMA_MODEL_PLACEHOLDER),
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Prompt Format",
            description: "The prompt format to use when requesting predictions. Set to Infer to have the format inferred based on the model name.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .ollama
                        .as_ref()?
                        .prompt_format
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .ollama
                        .get_or_insert_default()
                        .prompt_format = value;
                },
                json_path: Some("edit_predictions.ollama.prompt_format"),
            }),
            files: USER,
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Max Output Tokens",
            description: "The maximum number of tokens to generate.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .ollama
                        .as_ref()?
                        .max_output_tokens
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .ollama
                        .get_or_insert_default()
                        .max_output_tokens = value;
                },
                json_path: Some("edit_predictions.ollama.max_output_tokens"),
            }),
            metadata: None,
            files: USER,
        }),
    ])
}

pub(super) fn open_ai_compatible_settings() -> Box<[SettingsPageItem]> {
    Box::new([
        SettingsPageItem::SettingItem(SettingItem {
            title: "API URL",
            description: "The URL of your OpenAI-compatible server's completions API.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .open_ai_compatible_api
                        .as_ref()?
                        .api_url
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .open_ai_compatible_api
                        .get_or_insert_default()
                        .api_url = value;
                },
                json_path: Some("edit_predictions.open_ai_compatible_api.api_url"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some(OLLAMA_API_URL_PLACEHOLDER),
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Model",
            description: "The model string to pass to the OpenAI-compatible server.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .open_ai_compatible_api
                        .as_ref()?
                        .model
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .open_ai_compatible_api
                        .get_or_insert_default()
                        .model = value;
                },
                json_path: Some("edit_predictions.open_ai_compatible_api.model"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some(OLLAMA_MODEL_PLACEHOLDER),
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Prompt Format",
            description: "The prompt format to use when requesting predictions. Set to Infer to have the format inferred based on the model name.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .open_ai_compatible_api
                        .as_ref()?
                        .prompt_format
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .open_ai_compatible_api
                        .get_or_insert_default()
                        .prompt_format = value;
                },
                json_path: Some("edit_predictions.open_ai_compatible_api.prompt_format"),
            }),
            files: USER,
            metadata: None,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Max Output Tokens",
            description: "The maximum number of tokens to generate.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .open_ai_compatible_api
                        .as_ref()?
                        .max_output_tokens
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .open_ai_compatible_api
                        .get_or_insert_default()
                        .max_output_tokens = value;
                },
                json_path: Some("edit_predictions.open_ai_compatible_api.max_output_tokens"),
            }),
            metadata: None,
            files: USER,
        }),
    ])
}

pub(super) fn codestral_settings() -> Box<[SettingsPageItem]> {
    Box::new([
        SettingsPageItem::SettingItem(SettingItem {
            title: "API URL",
            description: "The API URL to use for Codestral.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .codestral
                        .as_ref()?
                        .api_url
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .codestral
                        .get_or_insert_default()
                        .api_url = value;
                },
                json_path: Some("edit_predictions.codestral.api_url"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some(CODESTRAL_API_URL),
                ..Default::default()
            })),
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Max Tokens",
            description: "The maximum number of tokens to generate.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .codestral
                        .as_ref()?
                        .max_tokens
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .codestral
                        .get_or_insert_default()
                        .max_tokens = value;
                },
                json_path: Some("edit_predictions.codestral.max_tokens"),
            }),
            metadata: None,
            files: USER,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: "Model",
            description: "The Codestral model id to use.",
            field: Box::new(SettingField {
                organization_override: None,
                pick: |settings| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .codestral
                        .as_ref()?
                        .model
                        .as_ref()
                },
                write: |settings, value, _app: &App| {
                    settings
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .codestral
                        .get_or_insert_default()
                        .model = value;
                },
                json_path: Some("edit_predictions.codestral.model"),
            }),
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("codestral-latest"),
                ..Default::default()
            })),
            files: USER,
        }),
    ])
}
