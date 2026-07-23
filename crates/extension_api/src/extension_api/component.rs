use super::*;

impl wit::Guest for Component {
    fn language_server_command(
        language_server_id: String,
        worktree: &wit::Worktree,
    ) -> Result<wit::Command> {
        let language_server_id = LanguageServerId(language_server_id);
        extension().language_server_command(&language_server_id, worktree)
    }

    fn language_server_initialization_options(
        language_server_id: String,
        worktree: &Worktree,
    ) -> Result<Option<String>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        Ok(extension()
            .language_server_initialization_options(&language_server_id, worktree)?
            .and_then(|value| serde_json::to_string(&value).ok()))
    }

    fn language_server_workspace_configuration(
        language_server_id: String,
        worktree: &Worktree,
    ) -> Result<Option<String>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        Ok(extension()
            .language_server_workspace_configuration(&language_server_id, worktree)?
            .and_then(|value| serde_json::to_string(&value).ok()))
    }

    fn language_server_initialization_options_schema(
        language_server_id: String,
        worktree: &Worktree,
    ) -> Option<String> {
        let language_server_id = LanguageServerId(language_server_id);
        extension()
            .language_server_initialization_options_schema(&language_server_id, worktree)
            .and_then(|value| serde_json::to_string(&value).ok())
    }

    fn language_server_workspace_configuration_schema(
        language_server_id: String,
        worktree: &Worktree,
    ) -> Option<String> {
        let language_server_id = LanguageServerId(language_server_id);
        extension()
            .language_server_workspace_configuration_schema(&language_server_id, worktree)
            .and_then(|value| serde_json::to_string(&value).ok())
    }

    fn language_server_additional_initialization_options(
        language_server_id: String,
        target_language_server_id: String,
        worktree: &Worktree,
    ) -> Result<Option<String>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        let target_language_server_id = LanguageServerId(target_language_server_id);
        Ok(extension()
            .language_server_additional_initialization_options(
                &language_server_id,
                &target_language_server_id,
                worktree,
            )?
            .and_then(|value| serde_json::to_string(&value).ok()))
    }

    fn language_server_additional_workspace_configuration(
        language_server_id: String,
        target_language_server_id: String,
        worktree: &Worktree,
    ) -> Result<Option<String>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        let target_language_server_id = LanguageServerId(target_language_server_id);
        Ok(extension()
            .language_server_additional_workspace_configuration(
                &language_server_id,
                &target_language_server_id,
                worktree,
            )?
            .and_then(|value| serde_json::to_string(&value).ok()))
    }

    fn labels_for_completions(
        language_server_id: String,
        completions: Vec<Completion>,
    ) -> Result<Vec<Option<CodeLabel>>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        let mut labels = Vec::new();
        for (ix, completion) in completions.into_iter().enumerate() {
            let label = extension().label_for_completion(&language_server_id, completion);
            if let Some(label) = label {
                labels.resize(ix + 1, None);
                *labels.last_mut().unwrap() = Some(label);
            }
        }
        Ok(labels)
    }

    fn labels_for_symbols(
        language_server_id: String,
        symbols: Vec<Symbol>,
    ) -> Result<Vec<Option<CodeLabel>>, String> {
        let language_server_id = LanguageServerId(language_server_id);
        let mut labels = Vec::new();
        for (ix, symbol) in symbols.into_iter().enumerate() {
            let label = extension().label_for_symbol(&language_server_id, symbol);
            if let Some(label) = label {
                labels.resize(ix + 1, None);
                *labels.last_mut().unwrap() = Some(label);
            }
        }
        Ok(labels)
    }

    fn complete_slash_command_argument(
        command: SlashCommand,
        args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        extension().complete_slash_command_argument(command, args)
    }

    fn run_slash_command(
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        extension().run_slash_command(command, args, worktree)
    }

    fn context_server_command(
        context_server_id: String,
        project: &Project,
    ) -> Result<wit::Command> {
        let context_server_id = ContextServerId(context_server_id);
        extension().context_server_command(&context_server_id, project)
    }

    fn context_server_configuration(
        context_server_id: String,
        project: &Project,
    ) -> Result<Option<ContextServerConfiguration>, String> {
        let context_server_id = ContextServerId(context_server_id);
        extension().context_server_configuration(&context_server_id, project)
    }

    fn suggest_docs_packages(provider: String) -> Result<Vec<String>, String> {
        extension().suggest_docs_packages(provider)
    }

    fn index_docs(
        provider: String,
        package: String,
        database: &KeyValueStore,
    ) -> Result<(), String> {
        extension().index_docs(provider, package, database)
    }

    fn get_dap_binary(
        adapter_name: String,
        config: DebugTaskDefinition,
        user_installed_path: Option<String>,
        worktree: &Worktree,
    ) -> Result<wit::DebugAdapterBinary, String> {
        extension().get_dap_binary(adapter_name, config, user_installed_path, worktree)
    }

    fn dap_request_kind(
        adapter_name: String,
        config: String,
    ) -> Result<StartDebuggingRequestArgumentsRequest, String> {
        extension().dap_request_kind(
            adapter_name,
            serde_json::from_str(&config).map_err(|e| format!("Failed to parse config: {e}"))?,
        )
    }
    fn dap_config_to_scenario(config: DebugConfig) -> Result<DebugScenario, String> {
        extension().dap_config_to_scenario(config)
    }
    fn dap_locator_create_scenario(
        locator_name: String,
        build_task: TaskTemplate,
        resolved_label: String,
        debug_adapter_name: String,
    ) -> Option<DebugScenario> {
        extension().dap_locator_create_scenario(
            locator_name,
            build_task,
            resolved_label,
            debug_adapter_name,
        )
    }
    fn run_dap_locator(
        locator_name: String,
        build_task: TaskTemplate,
    ) -> Result<DebugRequest, String> {
        extension().run_dap_locator(locator_name, build_task)
    }
}
