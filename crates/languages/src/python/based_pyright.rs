use super::*;

pub(crate) struct BasedPyrightLspAdapter {
    node: NodeRuntime,
}

impl BasedPyrightLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("basedpyright");
    const BINARY_NAME: &'static str = "basedpyright-langserver";
    const SERVER_PATH: &str = "node_modules/basedpyright/langserver.index.js";
    const NODE_MODULE_RELATIVE_SERVER_PATH: &str = "basedpyright/langserver.index.js";

    pub(crate) fn new(node: NodeRuntime) -> Self {
        BasedPyrightLspAdapter { node }
    }

    async fn get_cached_server_binary(
        container_dir: PathBuf,
        node: &NodeRuntime,
    ) -> Option<LanguageServerBinary> {
        let server_path = container_dir.join(Self::SERVER_PATH);
        if server_path.exists() {
            Some(LanguageServerBinary {
                path: node.binary_path().await.log_err()?,
                env: None,
                arguments: vec![server_path.into(), "--stdio".into()],
            })
        } else {
            log::error!("missing executable in directory {:?}", server_path);
            None
        }
    }
}

#[async_trait(?Send)]
impl LspAdapter for BasedPyrightLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }

    async fn initialization_options(
        self: Arc<Self>,
        _: &Arc<dyn LspAdapterDelegate>,
        _: &mut AsyncApp,
    ) -> Result<Option<Value>> {
        // Provide minimal initialization options
        // Virtual environment configuration will be handled through workspace configuration
        Ok(Some(json!({
            "python": {
                "analysis": {
                    "autoSearchPaths": true,
                    "useLibraryCodeForTypes": true,
                    "autoImportCompletions": true
                }
            }
        })))
    }

    async fn process_completions(&self, items: &mut [lsp::CompletionItem]) {
        process_pyright_completions(items);
    }

    async fn label_for_completion(
        &self,
        item: &lsp::CompletionItem,
        language: &Arc<language::Language>,
    ) -> Option<language::CodeLabel> {
        label_for_pyright_completion(item, language)
    }

    async fn label_for_symbol(
        &self,
        symbol: &Symbol,
        language: &Arc<language::Language>,
    ) -> Option<language::CodeLabel> {
        label_for_python_symbol(symbol, language)
    }

    async fn workspace_configuration(
        self: Arc<Self>,
        adapter: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        _: Option<Uri>,
        cx: &mut AsyncApp,
    ) -> Result<Value> {
        Ok(cx.update(move |cx| {
            let mut user_settings =
                language_server_settings(adapter.as_ref(), &Self::SERVER_NAME, cx)
                    .and_then(|s| s.settings.clone())
                    .unwrap_or_default();

            // If we have a detected toolchain, configure Pyright to use it
            let should_insert_toolchain = || {
                user_settings.as_object().is_none_or(|object| {
                    [
                        "venvPath",
                        "venv",
                        "python",
                        "pythonPath",
                        "defaultInterpreterPath",
                    ]
                    .into_iter()
                    .any(|known_key| object.contains_key(known_key))
                })
            };
            if let Some(toolchain) = toolchain
                && should_insert_toolchain()
                && let Ok(env) = serde_json::from_value::<
                    pet_core::python_environment::PythonEnvironment,
                >(toolchain.as_json.clone())
            {
                if !user_settings.is_object() {
                    user_settings = Value::Object(serde_json::Map::default());
                }
                let object = user_settings.as_object_mut().unwrap();

                let interpreter_path = toolchain.path.to_string();
                if let Some(venv_dir) = env.prefix {
                    // Set venvPath and venv at the root level
                    // This matches the format of a pyrightconfig.json file
                    if let Some(parent) = venv_dir.parent() {
                        // Use relative path if the venv is inside the workspace
                        let venv_path = if parent == adapter.worktree_root_path() {
                            ".".to_string()
                        } else {
                            parent.to_string_lossy().into_owned()
                        };
                        object.insert("venvPath".to_string(), Value::String(venv_path));
                    }

                    if let Some(venv_name) = venv_dir.file_name() {
                        object.insert(
                            "venv".to_owned(),
                            Value::String(venv_name.to_string_lossy().into_owned()),
                        );
                    }
                }

                // Set both pythonPath and defaultInterpreterPath for compatibility
                if let Some(python) = object
                    .entry("python")
                    .or_insert(Value::Object(serde_json::Map::default()))
                    .as_object_mut()
                {
                    python.insert(
                        "pythonPath".to_owned(),
                        Value::String(interpreter_path.clone()),
                    );
                    python.insert(
                        "defaultInterpreterPath".to_owned(),
                        Value::String(interpreter_path),
                    );
                }
                // Basedpyright by default uses `strict` type checking, we tone it down as to not surpris users
                maybe!({
                    let analysis = object
                        .entry("basedpyright.analysis")
                        .or_insert(Value::Object(serde_json::Map::default()));
                    if let serde_json::map::Entry::Vacant(v) =
                        analysis.as_object_mut()?.entry("typeCheckingMode")
                    {
                        v.insert(Value::String("standard".to_owned()));
                    }
                    Some(())
                });
                // Disable basedpyright's organizeImports so ruff handles it instead
                if let serde_json::map::Entry::Vacant(v) =
                    object.entry("basedpyright.disableOrganizeImports")
                {
                    v.insert(Value::Bool(true));
                }
            }

            user_settings
        }))
    }
}

impl LspInstaller for BasedPyrightLspAdapter {
    type BinaryVersion = Version;

    async fn fetch_latest_server_version(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<Self::BinaryVersion> {
        self.node
            .npm_package_latest_version(Self::SERVER_NAME.as_ref())
            .await
    }

    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        if let Some(path) = delegate.which(Self::BINARY_NAME.as_ref()).await {
            let env = delegate.shell_env().await;
            Some(LanguageServerBinary {
                path,
                env: Some(env),
                arguments: vec!["--stdio".into()],
            })
        } else {
            // TODO shouldn't this be self.node.binary_path()?
            let node = delegate.which("node".as_ref()).await?;
            let (node_modules_path, _) = delegate
                .npm_package_installed_version(Self::SERVER_NAME.as_ref())
                .await
                .log_err()??;

            let path = node_modules_path.join(Self::NODE_MODULE_RELATIVE_SERVER_PATH);

            let env = delegate.shell_env().await;
            Some(LanguageServerBinary {
                path: node,
                env: Some(env),
                arguments: vec![path.into(), "--stdio".into()],
            })
        }
    }

    fn fetch_server_binary(
        &self,
        _latest_version: Self::BinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();
        let node = self.node.clone();

        async move {
            let server_path = container_dir.join(Self::SERVER_PATH);
            node.npm_install_latest_packages(&container_dir, &[Self::SERVER_NAME.as_ref()])
                .await?;

            let env = delegate.shell_env().await;
            Ok(LanguageServerBinary {
                path: node.binary_path().await?,
                env: Some(env),
                arguments: vec![server_path.into(), "--stdio".into()],
            })
        }
    }

    fn check_if_version_installed(
        &self,
        version: &Self::BinaryVersion,
        container_dir: &PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Option<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();
        let node = self.node.clone();
        let version = version.clone();
        let container_dir = container_dir.clone();

        async move {
            let server_path = container_dir.join(Self::SERVER_PATH);

            let should_install_language_server = node
                .should_install_npm_package(
                    Self::SERVER_NAME.as_ref(),
                    &server_path,
                    &container_dir,
                    VersionStrategy::Latest(&version),
                )
                .await;

            if should_install_language_server {
                None
            } else {
                let env = delegate.shell_env().await;
                Some(LanguageServerBinary {
                    path: node.binary_path().await.ok()?,
                    env: Some(env),
                    arguments: vec![server_path.into(), "--stdio".into()],
                })
            }
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        delegate: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        let mut binary = Self::get_cached_server_binary(container_dir, &self.node).await?;
        binary.env = Some(delegate.shell_env().await);
        Some(binary)
    }
}
