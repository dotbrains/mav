use super::*;

pub(super) fn server_binary_arguments() -> Vec<OsString> {
    vec!["-mode=stdio".into()]
}

#[derive(Copy, Clone)]
pub struct GoLspAdapter;

impl GoLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("gopls");
}

pub(super) static VERSION_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+\.\d+\.\d+").expect("Failed to create VERSION_REGEX"));

pub(super) static GO_ESCAPE_SUBTEST_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[.*+?^${}()|\[\]\\"']"#).expect("Failed to create GO_ESCAPE_SUBTEST_NAME_REGEX")
});

const BINARY: &str = if cfg!(target_os = "windows") {
    "gopls.exe"
} else {
    "gopls"
};

impl LspInstaller for GoLspAdapter {
    type BinaryVersion = Option<String>;

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        cx: &mut AsyncApp,
    ) -> Result<Option<String>> {
        static DID_SHOW_NOTIFICATION: AtomicBool = AtomicBool::new(false);

        const NOTIFICATION_MESSAGE: &str =
            "Could not install the Go language server `gopls`, because `go` was not found.";

        if delegate.which("go".as_ref()).await.is_none() {
            if DID_SHOW_NOTIFICATION
                .compare_exchange(false, true, SeqCst, SeqCst)
                .is_ok()
            {
                cx.update(|cx| {
                    delegate.show_notification(NOTIFICATION_MESSAGE, cx);
                });
            }
            anyhow::bail!(
                "Could not install the Go language server `gopls`, because `go` was not found."
            );
        }

        let release =
            latest_github_release("golang/tools", false, false, delegate.http_client()).await?;
        let version: Option<String> = release.tag_name.strip_prefix("gopls/v").map(str::to_string);
        if version.is_none() {
            log::warn!(
                "couldn't infer gopls version from GitHub release tag name '{}'",
                release.tag_name
            );
        }
        Ok(version)
    }

    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let path = delegate.which(Self::SERVER_NAME.as_ref()).await?;
        Some(LanguageServerBinary {
            path,
            arguments: server_binary_arguments(),
            env: None,
        })
    }

    fn fetch_server_binary(
        &self,
        version: Option<String>,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let go = delegate.which("go".as_ref()).await.unwrap_or("go".into());
            let go_version_output = util::command::new_command(&go)
                .args(["version"])
                .output()
                .await
                .context("failed to get go version via `go version` command`")?;
            let go_version = parse_version_output(&go_version_output)?;

            if let Some(version) = version {
                let binary_path = container_dir.join(format!("gopls_{version}_go_{go_version}"));
                if let Ok(metadata) = fs::metadata(&binary_path).await
                    && metadata.is_file()
                {
                    remove_matching(&container_dir, |entry| {
                        entry != binary_path && entry.file_name() != Some(OsStr::new("gobin"))
                    })
                    .await;

                    return Ok(LanguageServerBinary {
                        path: binary_path.to_path_buf(),
                        arguments: server_binary_arguments(),
                        env: None,
                    });
                }
            } else if let Some(path) = get_cached_server_binary(&container_dir).await {
                return Ok(path);
            }

            let gobin_dir = container_dir.join("gobin");
            fs::create_dir_all(&gobin_dir).await?;
            let install_output = util::command::new_command(go)
                .env("GO111MODULE", "on")
                .env("GOBIN", &gobin_dir)
                .args(["install", "golang.org/x/tools/gopls@latest"])
                .output()
                .await?;

            if !install_output.status.success() {
                log::error!(
                    "failed to install gopls via `go install`. stdout: {:?}, stderr: {:?}",
                    String::from_utf8_lossy(&install_output.stdout),
                    String::from_utf8_lossy(&install_output.stderr)
                );
                anyhow::bail!(
                    "failed to install gopls with `go install`. Is `go` installed and in the PATH? Check logs for more information."
                );
            }

            let installed_binary_path = gobin_dir.join(BINARY);
            let version_output = util::command::new_command(&installed_binary_path)
                .arg("version")
                .output()
                .await
                .context("failed to run installed gopls binary")?;
            let gopls_version = parse_version_output(&version_output)?;
            let binary_path = container_dir.join(format!("gopls_{gopls_version}_go_{go_version}"));
            fs::rename(&installed_binary_path, &binary_path).await?;

            Ok(LanguageServerBinary {
                path: binary_path.to_path_buf(),
                arguments: server_binary_arguments(),
                env: None,
            })
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        get_cached_server_binary(&container_dir).await
    }
}

#[async_trait(?Send)]
impl LspAdapter for GoLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }

    async fn initialization_options(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Option<serde_json::Value>> {
        let semantic_tokens_enabled = cx.update(|cx| {
            LanguageSettings::resolve(None, Some(&LanguageName::new("Go")), cx)
                .semantic_tokens
                .enabled()
        });

        let mut default_config = json!({
            "usePlaceholders": false,
            "hints": {
                "assignVariableTypes": true,
                "compositeLiteralFields": true,
                "compositeLiteralTypes": true,
                "constantValues": true,
                "functionTypeParameters": true,
                "parameterNames": true,
                "rangeVariableTypes": true
            },
            "codelenses": {
                "test": true
            },
            "semanticTokens": semantic_tokens_enabled
        });

        let project_initialization_options = cx.update(|cx| {
            language_server_settings(delegate.as_ref(), &self.name(), cx)
                .and_then(|s| s.initialization_options.clone())
        });

        if let Some(override_options) = project_initialization_options {
            merge_json_value_into(override_options, &mut default_config);
        }

        Ok(Some(default_config))
    }

    async fn workspace_configuration(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: Option<lsp::Uri>,
        cx: &mut AsyncApp,
    ) -> Result<Value> {
        Ok(cx
            .update(|cx| {
                language_server_settings(delegate.as_ref(), &self.name(), cx)
                    .and_then(|settings| settings.settings.clone())
            })
            .unwrap_or_default())
    }

    async fn label_for_completion(
        &self,
        completion: &lsp::CompletionItem,
        language: &Arc<Language>,
    ) -> Option<CodeLabel> {
        let label = &completion.label;

        // Gopls returns nested fields and methods as completions.
        // To syntax highlight these, combine their final component
        // with their detail.
        let name_offset = label.rfind('.').unwrap_or(0);

        match completion.kind.zip(completion.detail.as_ref()) {
            Some((lsp::CompletionItemKind::MODULE, detail)) => {
                let text = format!("{label} {detail}");
                let source = Rope::from(format!("import {text}").as_str());
                let runs = language.highlight_text(&source, 7..7 + text[name_offset..].len());
                let filter_range = completion
                    .filter_text
                    .as_deref()
                    .and_then(|filter_text| {
                        text.find(filter_text)
                            .map(|start| start..start + filter_text.len())
                    })
                    .unwrap_or(0..label.len());
                return Some(CodeLabel::new(text, filter_range, runs));
            }
            Some((
                lsp::CompletionItemKind::CONSTANT | lsp::CompletionItemKind::VARIABLE,
                detail,
            )) => {
                let text = format!("{label} {detail}");
                let source =
                    Rope::from(format!("var {} {}", &text[name_offset..], detail).as_str());
                let runs = adjust_runs(
                    name_offset,
                    language.highlight_text(&source, 4..4 + text[name_offset..].len()),
                );
                let filter_range = completion
                    .filter_text
                    .as_deref()
                    .and_then(|filter_text| {
                        text.find(filter_text)
                            .map(|start| start..start + filter_text.len())
                    })
                    .unwrap_or(0..label.len());
                return Some(CodeLabel::new(text, filter_range, runs));
            }
            Some((lsp::CompletionItemKind::STRUCT, _)) => {
                let text = format!("{label} struct {{}}");
                let source = Rope::from(format!("type {}", &text[name_offset..]).as_str());
                let runs = adjust_runs(
                    name_offset,
                    language.highlight_text(&source, 5..5 + text[name_offset..].len()),
                );
                let filter_range = completion
                    .filter_text
                    .as_deref()
                    .and_then(|filter_text| {
                        text.find(filter_text)
                            .map(|start| start..start + filter_text.len())
                    })
                    .unwrap_or(0..label.len());
                return Some(CodeLabel::new(text, filter_range, runs));
            }
            Some((lsp::CompletionItemKind::INTERFACE, _)) => {
                let text = format!("{label} interface {{}}");
                let source = Rope::from(format!("type {}", &text[name_offset..]).as_str());
                let runs = adjust_runs(
                    name_offset,
                    language.highlight_text(&source, 5..5 + text[name_offset..].len()),
                );
                let filter_range = completion
                    .filter_text
                    .as_deref()
                    .and_then(|filter_text| {
                        text.find(filter_text)
                            .map(|start| start..start + filter_text.len())
                    })
                    .unwrap_or(0..label.len());
                return Some(CodeLabel::new(text, filter_range, runs));
            }
            Some((lsp::CompletionItemKind::FIELD, detail)) => {
                let text = format!("{label} {detail}");
                let source =
                    Rope::from(format!("type T struct {{ {} }}", &text[name_offset..]).as_str());
                let runs = adjust_runs(
                    name_offset,
                    language.highlight_text(&source, 16..16 + text[name_offset..].len()),
                );
                let filter_range = completion
                    .filter_text
                    .as_deref()
                    .and_then(|filter_text| {
                        text.find(filter_text)
                            .map(|start| start..start + filter_text.len())
                    })
                    .unwrap_or(0..label.len());
                return Some(CodeLabel::new(text, filter_range, runs));
            }
            Some((lsp::CompletionItemKind::FUNCTION | lsp::CompletionItemKind::METHOD, detail)) => {
                if let Some(signature) = detail.strip_prefix("func") {
                    let text = format!("{label}{signature}");
                    let source = Rope::from(format!("func {} {{}}", &text[name_offset..]).as_str());
                    let runs = adjust_runs(
                        name_offset,
                        language.highlight_text(&source, 5..5 + text[name_offset..].len()),
                    );
                    let filter_range = completion
                        .filter_text
                        .as_deref()
                        .and_then(|filter_text| {
                            text.find(filter_text)
                                .map(|start| start..start + filter_text.len())
                        })
                        .unwrap_or(0..label.len());
                    return Some(CodeLabel::new(text, filter_range, runs));
                }
            }
            _ => {}
        }
        None
    }

    async fn label_for_symbol(
        &self,
        symbol: &language::Symbol,
        language: &Arc<Language>,
    ) -> Option<CodeLabel> {
        let name = &symbol.name;
        let (text, filter_range, display_range) = match symbol.kind {
            lsp::SymbolKind::METHOD | lsp::SymbolKind::FUNCTION => {
                let text = format!("func {} () {{}}", name);
                let filter_range = 5..5 + name.len();
                let display_range = 0..filter_range.end;
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::STRUCT => {
                let text = format!("type {} struct {{}}", name);
                let filter_range = 5..5 + name.len();
                let display_range = 0..text.len();
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::INTERFACE => {
                let text = format!("type {} interface {{}}", name);
                let filter_range = 5..5 + name.len();
                let display_range = 0..text.len();
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::CLASS => {
                let text = format!("type {} T", name);
                let filter_range = 5..5 + name.len();
                let display_range = 0..filter_range.end;
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::CONSTANT => {
                let text = format!("const {} = nil", name);
                let filter_range = 6..6 + name.len();
                let display_range = 0..filter_range.end;
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::VARIABLE => {
                let text = format!("var {} = nil", name);
                let filter_range = 4..4 + name.len();
                let display_range = 0..filter_range.end;
                (text, filter_range, display_range)
            }
            lsp::SymbolKind::MODULE => {
                let text = format!("package {}", name);
                let filter_range = 8..8 + name.len();
                let display_range = 0..filter_range.end;
                (text, filter_range, display_range)
            }
            _ => return None,
        };

        Some(CodeLabel::new(
            text[display_range.clone()].to_string(),
            filter_range,
            language.highlight_text(&text.as_str().into(), display_range),
        ))
    }

    fn client_command(
        &self,
        command_name: &str,
        arguments: &[serde_json::Value],
    ) -> Option<ClientCommand> {
        if let "gopls.run_tests" = command_name {
            let template = go_test_task_template(arguments.first()?)?;
            Some(ClientCommand::ScheduleTask(template))
        } else {
            None
        }
    }

    fn diagnostic_message_to_markdown(&self, message: &str) -> Option<String> {
        static REGEX: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(?m)\n\s*").expect("Failed to create REGEX"));
        Some(REGEX.replace_all(message, "\n\n").to_string())
    }
}
