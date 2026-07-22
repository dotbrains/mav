use super::*;

pub(crate) struct PyLspAdapter {
    python_venv_base: OnceCell<Result<Arc<Path>, String>>,
}
impl PyLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("pylsp");
    pub(crate) fn new() -> Self {
        Self {
            python_venv_base: OnceCell::new(),
        }
    }
    async fn ensure_venv(delegate: &dyn LspAdapterDelegate) -> Result<Arc<Path>> {
        let python_path = Self::find_base_python(delegate)
            .await
            .with_context(|| {
                let mut message = "Could not find Python installation for PyLSP".to_owned();
                if cfg!(windows){
                    message.push_str(". Install Python from the Microsoft Store, or manually from https://www.python.org/downloads/windows.")
                }
                message
            })?;
        let work_dir = delegate
            .language_server_download_dir(&Self::SERVER_NAME)
            .await
            .context("Could not get working directory for PyLSP")?;
        let mut path = PathBuf::from(work_dir.as_ref());
        path.push("pylsp-venv");
        if !path.exists() {
            util::command::new_command(python_path)
                .arg("-m")
                .arg("venv")
                .arg("pylsp-venv")
                .current_dir(work_dir)
                .spawn()?
                .output()
                .await?;
        }

        Ok(path.into())
    }
    // Find "baseline", user python version from which we'll create our own venv.
    async fn find_base_python(delegate: &dyn LspAdapterDelegate) -> Option<PathBuf> {
        for path in ["python3", "python"] {
            let Some(path) = delegate.which(path.as_ref()).await else {
                continue;
            };
            // Try to detect situations where `python3` exists but is not a real Python interpreter.
            // Notably, on fresh Windows installs, `python3` is a shim that opens the Microsoft Store app
            // when run with no arguments, and just fails otherwise.
            let Some(output) = new_command(&path)
                .args(["-c", "print(1 + 2)"])
                .output()
                .await
                .ok()
            else {
                continue;
            };
            if output.stdout.trim_ascii() != b"3" {
                continue;
            }
            return Some(path);
        }
        None
    }

    async fn base_venv(&self, delegate: &dyn LspAdapterDelegate) -> Result<Arc<Path>, String> {
        self.python_venv_base
            .get_or_init(move || async move {
                Self::ensure_venv(delegate)
                    .await
                    .map_err(|e| format!("{e}"))
            })
            .await
            .clone()
    }
}

const BINARY_DIR: &str = if cfg!(target_os = "windows") {
    "Scripts"
} else {
    "bin"
};

#[async_trait(?Send)]
impl LspAdapter for PyLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }

    async fn process_completions(&self, items: &mut [lsp::CompletionItem]) {
        for item in items {
            let is_named_argument = item.label.ends_with('=');
            let priority = if is_named_argument { '0' } else { '1' };
            let sort_text = item.sort_text.take().unwrap_or_else(|| item.label.clone());
            item.sort_text = Some(format!("{}{}", priority, sort_text));
        }
    }

    async fn label_for_completion(
        &self,
        item: &lsp::CompletionItem,
        language: &Arc<language::Language>,
    ) -> Option<language::CodeLabel> {
        let label = &item.label;
        let label_len = label.len();
        let grammar = language.grammar()?;
        let highlight_id = highlight_id_for_completion(item.kind?, grammar)??;
        Some(language::CodeLabel::filtered(
            label.clone(),
            label_len,
            item.filter_text.as_deref(),
            vec![(0..label.len(), highlight_id)],
        ))
    }

    async fn label_for_symbol(
        &self,
        symbol: &language::Symbol,
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
                    .unwrap_or_else(|| {
                        json!({
                            "plugins": {
                                "pycodestyle": {"enabled": false},
                                "rope_autoimport": {"enabled": true, "memory": true},
                                "pylsp_mypy": {"enabled": false}
                            },
                            "rope": {
                                "ropeFolder": null
                            },
                        })
                    });

            // If user did not explicitly modify their python venv, use one from picker.
            if let Some(toolchain) = toolchain {
                if !user_settings.is_object() {
                    user_settings = Value::Object(serde_json::Map::default());
                }
                let object = user_settings.as_object_mut().unwrap();
                if let Some(python) = object
                    .entry("plugins")
                    .or_insert(Value::Object(serde_json::Map::default()))
                    .as_object_mut()
                {
                    if let Some(jedi) = python
                        .entry("jedi")
                        .or_insert(Value::Object(serde_json::Map::default()))
                        .as_object_mut()
                    {
                        jedi.entry("environment".to_string())
                            .or_insert_with(|| Value::String(toolchain.path.clone().into()));
                    }
                    if let Some(pylint) = python
                        .entry("pylsp_mypy")
                        .or_insert(Value::Object(serde_json::Map::default()))
                        .as_object_mut()
                    {
                        pylint.entry("overrides".to_string()).or_insert_with(|| {
                            Value::Array(vec![
                                Value::String("--python-executable".into()),
                                Value::String(toolchain.path.into()),
                                Value::String("--cache-dir=/dev/null".into()),
                                Value::Bool(true),
                            ])
                        });
                    }
                }
            }
            user_settings = Value::Object(serde_json::Map::from_iter([(
                "pylsp".to_string(),
                user_settings,
            )]));

            user_settings
        }))
    }
}

impl LspInstaller for PyLspAdapter {
    type BinaryVersion = ();
    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        if let Some(pylsp_bin) = delegate.which(Self::SERVER_NAME.as_ref()).await {
            let env = delegate.shell_env().await;
            delegate
                .try_exec(LanguageServerBinary {
                    path: pylsp_bin.clone(),
                    arguments: vec!["--version".into()],
                    env: Some(env.clone()),
                })
                .await
                .inspect_err(|err| {
                    log::warn!("failed to validate user-installed pylsp at {pylsp_bin:?}: {err:#}")
                })
                .ok()?;
            Some(LanguageServerBinary {
                path: pylsp_bin,
                env: Some(env),
                arguments: vec![],
            })
        } else {
            let toolchain = toolchain?;
            let pylsp_path = Path::new(toolchain.path.as_ref()).parent()?.join("pylsp");
            if !pylsp_path.exists() {
                return None;
            }
            delegate
                .try_exec(LanguageServerBinary {
                    path: toolchain.path.to_string().into(),
                    arguments: vec![pylsp_path.clone().into(), "--version".into()],
                    env: None,
                })
                .await
                .inspect_err(|err| {
                    log::warn!("failed to validate toolchain pylsp at {pylsp_path:?}: {err:#}")
                })
                .ok()?;
            Some(LanguageServerBinary {
                path: toolchain.path.to_string().into(),
                arguments: vec![pylsp_path.into()],
                env: None,
            })
        }
    }

    async fn fetch_latest_server_version(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<()> {
        Ok(())
    }

    fn fetch_server_binary(
        &self,
        _: (),
        _: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let venv = Self::ensure_venv(delegate.as_ref()).await?;
            let pip_path = venv.join(BINARY_DIR).join("pip3");
            ensure!(
                util::command::new_command(pip_path.as_path())
                    .arg("install")
                    .arg("python-lsp-server[all]")
                    .arg("--upgrade")
                    .output()
                    .await?
                    .status
                    .success(),
                "python-lsp-server[all] installation failed"
            );
            ensure!(
                util::command::new_command(pip_path)
                    .arg("install")
                    .arg("pylsp-mypy")
                    .arg("--upgrade")
                    .output()
                    .await?
                    .status
                    .success(),
                "pylsp-mypy installation failed"
            );
            let pylsp = venv.join(BINARY_DIR).join("pylsp");
            ensure!(
                delegate.which(pylsp.as_os_str()).await.is_some(),
                "pylsp installation was incomplete"
            );
            Ok(LanguageServerBinary {
                path: pylsp,
                env: None,
                arguments: vec![],
            })
        }
    }

    async fn cached_server_binary(
        &self,
        _: PathBuf,
        delegate: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        let venv = self.base_venv(delegate).await.ok()?;
        let pylsp = venv.join(BINARY_DIR).join("pylsp");
        delegate.which(pylsp.as_os_str()).await?;
        Some(LanguageServerBinary {
            path: pylsp,
            env: None,
            arguments: vec![],
        })
    }
}
