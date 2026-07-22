use super::*;

#[async_trait(?Send)]
impl LspAdapter for RustLspAdapter {
    fn name(&self) -> LanguageServerName {
        SERVER_NAME
    }

    fn disk_based_diagnostic_sources(&self) -> Vec<String> {
        vec![CARGO_DIAGNOSTICS_SOURCE_NAME.to_owned()]
    }

    fn disk_based_diagnostics_progress_token(&self) -> Option<String> {
        Some("rust-analyzer/flycheck".into())
    }

    fn process_diagnostics(&self, params: &mut lsp::PublishDiagnosticsParams, _: LanguageServerId) {
        static REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)`([^`]+)\n`$").expect("Failed to create REGEX"));

        for diagnostic in &mut params.diagnostics {
            for message in diagnostic
                .related_information
                .iter_mut()
                .flatten()
                .map(|info| &mut info.message)
                .chain([&mut diagnostic.message])
            {
                if let Cow::Owned(sanitized) = REGEX.replace_all(message, "`$1`") {
                    *message = sanitized;
                }
            }
        }
    }

    fn diagnostic_message_to_markdown(&self, message: &str) -> Option<String> {
        static REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)\n *").expect("Failed to create REGEX"));
        Some(REGEX.replace_all(message, "\n\n").to_string())
    }

    async fn label_for_completion(
        &self,
        completion: &lsp::CompletionItem,
        language: &Arc<Language>,
    ) -> Option<CodeLabel> {
        // rust-analyzer calls these detail left and detail right in terms of where it expects things to be rendered
        // this usually contains signatures of the thing to be completed
        let detail_right = completion
            .label_details
            .as_ref()
            .and_then(|detail| detail.description.as_ref())
            .or(completion.detail.as_ref())
            .map(|detail| detail.trim());
        // this tends to contain alias and import information
        let mut detail_left = completion
            .label_details
            .as_ref()
            .and_then(|detail| detail.detail.as_deref());
        let mk_label = |text: String, filter_range: &dyn Fn() -> Range<usize>, runs| {
            let filter_range = completion
                .filter_text
                .as_deref()
                .and_then(|filter| text.find(filter).map(|ix| ix..ix + filter.len()))
                .or_else(|| {
                    text.find(&completion.label)
                        .map(|ix| ix..ix + completion.label.len())
                })
                .unwrap_or_else(filter_range);

            CodeLabel::new(text, filter_range, runs)
        };
        let mut label = match (detail_right, completion.kind) {
            (Some(signature), Some(lsp::CompletionItemKind::FIELD)) => {
                let name = &completion.label;
                let text = format!("{name}: {signature}");
                let prefix = "struct S { ";
                let source = Rope::from_iter([prefix, &text, " }"]);
                let runs =
                    language.highlight_text(&source, prefix.len()..prefix.len() + text.len());
                mk_label(text, &|| 0..completion.label.len(), runs)
            }
            (
                Some(signature),
                Some(lsp::CompletionItemKind::CONSTANT | lsp::CompletionItemKind::VARIABLE),
            ) if completion.insert_text_format != Some(lsp::InsertTextFormat::SNIPPET) => {
                let name = &completion.label;
                let text = format!("{name}: {signature}",);
                let prefix = "let ";
                let source = Rope::from_iter([prefix, &text, " = ();"]);
                let runs =
                    language.highlight_text(&source, prefix.len()..prefix.len() + text.len());
                mk_label(text, &|| 0..completion.label.len(), runs)
            }
            (
                function_signature,
                Some(lsp::CompletionItemKind::FUNCTION | lsp::CompletionItemKind::METHOD),
            ) => {
                const FUNCTION_PREFIXES: [&str; 6] = [
                    "async fn",
                    "async unsafe fn",
                    "const fn",
                    "const unsafe fn",
                    "unsafe fn",
                    "fn",
                ];
                let fn_prefixed = FUNCTION_PREFIXES.iter().find_map(|&prefix| {
                    function_signature?
                        .strip_prefix(prefix)
                        .map(|suffix| (prefix, suffix))
                });
                let label = if let Some(label) = completion
                    .label
                    .strip_suffix("(…)")
                    .or_else(|| completion.label.strip_suffix("()"))
                {
                    label
                } else {
                    &completion.label
                };

                static FULL_SIGNATURE_REGEX: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"fn (.+?)\(").expect("Failed to create REGEX"));
                if let Some((function_signature, match_)) = function_signature
                    .filter(|it| it.contains(&label))
                    .and_then(|it| Some((it, FULL_SIGNATURE_REGEX.find(it)?)))
                {
                    let source = Rope::from(function_signature);
                    let runs = language.highlight_text(&source, 0..function_signature.len());
                    mk_label(
                        function_signature.to_owned(),
                        &|| match_.range().start + 3..match_.range().end - 1,
                        runs,
                    )
                } else if let Some((prefix, suffix)) = fn_prefixed {
                    let text = format!("{label}{suffix}");
                    let source = Rope::from_iter([prefix, " ", &text, " {}"]);
                    let run_start = prefix.len() + 1;
                    let runs = language.highlight_text(&source, run_start..run_start + text.len());
                    mk_label(text, &|| 0..label.len(), runs)
                } else if completion
                    .detail
                    .as_ref()
                    .is_some_and(|detail| detail.starts_with("macro_rules! "))
                {
                    let text = completion.label.clone();
                    let len = text.len();
                    let source = Rope::from(text.as_str());
                    let runs = language.highlight_text(&source, 0..len);
                    mk_label(text, &|| 0..completion.label.len(), runs)
                } else if detail_left.is_none() {
                    return None;
                } else {
                    mk_label(
                        completion.label.clone(),
                        &|| 0..completion.label.len(),
                        vec![],
                    )
                }
            }
            (_, kind) => {
                let mut label;
                let mut runs = vec![];

                if completion.insert_text_format == Some(lsp::InsertTextFormat::SNIPPET)
                    && let Some(
                        lsp::CompletionTextEdit::InsertAndReplace(lsp::InsertReplaceEdit {
                            new_text,
                            ..
                        })
                        | lsp::CompletionTextEdit::Edit(lsp::TextEdit { new_text, .. }),
                    ) = completion.text_edit.as_ref()
                    && let Ok(mut snippet) = snippet::Snippet::parse(new_text)
                    && snippet.tabstops.len() > 1
                {
                    label = String::new();

                    // we never display the final tabstop
                    snippet.tabstops.remove(snippet.tabstops.len() - 1);

                    let mut text_pos = 0;

                    let mut all_stop_ranges = snippet
                        .tabstops
                        .into_iter()
                        .flat_map(|stop| stop.ranges)
                        .collect::<SmallVec<[_; 8]>>();
                    all_stop_ranges.sort_unstable_by_key(|a| (a.start, Reverse(a.end)));

                    for range in &all_stop_ranges {
                        let start_pos = range.start as usize;
                        let end_pos = range.end as usize;

                        label.push_str(&snippet.text[text_pos..start_pos]);

                        if start_pos == end_pos {
                            let caret_start = label.len();
                            label.push('…');
                            runs.push((caret_start..label.len(), HighlightId::TABSTOP_INSERT_ID));
                        } else {
                            let label_start = label.len();
                            label.push_str(&snippet.text[start_pos..end_pos]);
                            let label_end = label.len();
                            runs.push((label_start..label_end, HighlightId::TABSTOP_REPLACE_ID));
                        }

                        text_pos = end_pos;
                    }

                    label.push_str(&snippet.text[text_pos..]);

                    if detail_left.is_some_and(|detail_left| detail_left == new_text) {
                        // We only include the left detail if it isn't the snippet again
                        detail_left.take();
                    }

                    runs.extend(language.highlight_text(&Rope::from(&label), 0..label.len()));
                } else {
                    let highlight_name = kind.and_then(|kind| match kind {
                        lsp::CompletionItemKind::STRUCT
                        | lsp::CompletionItemKind::INTERFACE
                        | lsp::CompletionItemKind::ENUM => Some("type"),
                        lsp::CompletionItemKind::ENUM_MEMBER => Some("variant"),
                        lsp::CompletionItemKind::KEYWORD => Some("keyword"),
                        lsp::CompletionItemKind::VALUE | lsp::CompletionItemKind::CONSTANT => {
                            Some("constant")
                        }
                        _ => None,
                    });

                    label = completion.label.clone();

                    if let Some(highlight_name) = highlight_name {
                        let highlight_id =
                            language.grammar()?.highlight_id_for_name(highlight_name)?;
                        runs.push((
                            0..label.rfind('(').unwrap_or(completion.label.len()),
                            highlight_id,
                        ));
                    } else if detail_left.is_none()
                        && kind != Some(lsp::CompletionItemKind::SNIPPET)
                    {
                        return None;
                    }
                }

                let label_len = label.len();

                mk_label(label, &|| 0..label_len, runs)
            }
        };

        if let Some(detail_left) = detail_left {
            label.text.push(' ');
            if !detail_left.starts_with('(') {
                label.text.push('(');
            }
            label.text.push_str(detail_left);
            if !detail_left.ends_with(')') {
                label.text.push(')');
            }
        }

        Some(label)
    }

    async fn initialization_options_schema(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        let binary = self
            .get_language_server_command(
                delegate.clone(),
                None,
                LanguageServerBinaryOptions {
                    allow_path_lookup: true,
                    allow_binary_download: false,
                    pre_release: false,
                },
                cached_binary,
                cx.clone(),
            )
            .await
            .0
            .ok()?;

        let mut command = util::command::new_command(&binary.path);
        command
            .arg("--print-config-schema")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let cmd = command
            .spawn()
            .map_err(|e| log::debug!("failed to spawn command {command:?}: {e}"))
            .ok()?;
        let output = cmd
            .output()
            .await
            .map_err(|e| log::debug!("failed to execute command {command:?}: {e}"))
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let raw_schema: serde_json::Value = serde_json::from_slice(output.stdout.as_slice())
            .map_err(|e| log::debug!("failed to parse rust-analyzer's JSON schema output: {e}"))
            .ok()?;

        // Convert rust-analyzer's array-based schema format to nested JSON Schema
        let converted_schema = Self::convert_rust_analyzer_schema(&raw_schema);
        Some(converted_schema)
    }

    async fn label_for_symbol(
        &self,
        symbol: &language::Symbol,
        language: &Arc<Language>,
    ) -> Option<CodeLabel> {
        let name = &symbol.name;
        let (prefix, suffix) = match symbol.kind {
            lsp::SymbolKind::METHOD | lsp::SymbolKind::FUNCTION => ("fn ", "();"),
            lsp::SymbolKind::STRUCT => ("struct ", ";"),
            lsp::SymbolKind::ENUM => ("enum ", "{}"),
            lsp::SymbolKind::INTERFACE => ("trait ", "{}"),
            lsp::SymbolKind::CONSTANT => ("const ", ":()=();"),
            lsp::SymbolKind::MODULE => ("mod ", ";"),
            lsp::SymbolKind::PACKAGE => ("extern crate ", ";"),
            lsp::SymbolKind::TYPE_PARAMETER => ("type ", "=();"),
            lsp::SymbolKind::ENUM_MEMBER => {
                let prefix = "enum E {";
                return Some(CodeLabel::new(
                    name.to_string(),
                    0..name.len(),
                    language.highlight_text(
                        &Rope::from_iter([prefix, name, "}"]),
                        prefix.len()..prefix.len() + name.len(),
                    ),
                ));
            }
            _ => return None,
        };

        let filter_range = prefix.len()..prefix.len() + name.len();
        let display_range = 0..filter_range.end;
        Some(CodeLabel::new(
            format!("{prefix}{name}"),
            filter_range,
            language.highlight_text(&Rope::from_iter([prefix, name, suffix]), display_range),
        ))
    }

    fn prepare_initialize_params(
        &self,
        mut original: InitializeParams,
        cx: &App,
    ) -> Result<InitializeParams> {
        let enable_lsp_tasks = ProjectSettings::get_global(cx)
            .lsp
            .get(&SERVER_NAME)
            .is_some_and(|s| s.enable_lsp_tasks);

        let mut experimental = json!({
            "commands": {
                "commands": [
                    "rust-analyzer.showReferences",
                    "rust-analyzer.gotoLocation",
                    "rust-analyzer.triggerParameterHints",
                    "rust-analyzer.rename",
                ]
            }
        });

        if enable_lsp_tasks {
            merge_json_value_into(
                json!({
                    "runnables": {
                        "kinds": [ "cargo", "shell" ],
                    },
                    "commands": {
                        "commands": [
                            "rust-analyzer.runSingle",
                        ]
                    }
                }),
                &mut experimental,
            );
        }

        if let Some(original_experimental) = &mut original.capabilities.experimental {
            merge_json_value_into(experimental, original_experimental);
        } else {
            original.capabilities.experimental = Some(experimental);
        }

        Ok(original)
    }

    fn client_command(
        &self,
        command_name: &str,
        arguments: &[serde_json::Value],
    ) -> Option<ClientCommand> {
        match command_name {
            "rust-analyzer.showReferences" => Some(ClientCommand::ShowLocations),
            "rust-analyzer.runSingle" => {
                let first_arg = arguments.first()?;
                let runnable =
                    serde_json::from_value::<lsp_ext_command::Runnable>(first_arg.clone()).ok()?;
                let template =
                    lsp_ext_command::runnable_to_task_template(runnable.label, runnable.args);
                Some(ClientCommand::ScheduleTask(template))
            }
            _ => None,
        }
    }
}
