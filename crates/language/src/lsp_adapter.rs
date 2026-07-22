use super::*;

/// [`LspAdapterDelegate`] allows [`LspAdapter]` implementations to interface with the application
// e.g. to display a notification or fetch data from the web.
#[async_trait]
pub trait LspAdapterDelegate: Send + Sync {
    fn show_notification(&self, message: &str, cx: &mut App);
    fn http_client(&self) -> Arc<dyn HttpClient>;
    fn worktree_id(&self) -> WorktreeId;
    fn worktree_root_path(&self) -> &Path;
    fn resolve_relative_path(&self, path: PathBuf) -> PathBuf;
    fn update_status(&self, language: LanguageServerName, status: BinaryStatus);
    fn registered_lsp_adapters(&self) -> Vec<Arc<dyn LspAdapter>>;
    async fn language_server_download_dir(&self, name: &LanguageServerName) -> Option<Arc<Path>>;

    async fn npm_package_installed_version(
        &self,
        package_name: &str,
    ) -> Result<Option<(PathBuf, Version)>>;
    async fn which(&self, command: &OsStr) -> Option<PathBuf>;
    async fn shell_env(&self) -> HashMap<String, String>;
    async fn read_text_file(&self, path: &RelPath) -> Result<String>;
    async fn try_exec(&self, binary: LanguageServerBinary) -> Result<()>;
}

#[async_trait(?Send)]
pub trait LspAdapter: 'static + Send + Sync + DynLspInstaller {
    fn name(&self) -> LanguageServerName;

    fn process_diagnostics(&self, _: &mut lsp::PublishDiagnosticsParams, _: LanguageServerId) {}

    /// When processing new `lsp::PublishDiagnosticsParams` diagnostics, whether to retain previous one(s) or not.
    fn retain_old_diagnostic(&self, _previous_diagnostic: &Diagnostic) -> bool {
        false
    }

    /// Whether to underline a given diagnostic or not, when rendering in the editor.
    ///
    /// https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#diagnosticTag
    /// states that
    /// > Clients are allowed to render diagnostics with this tag faded out instead of having an error squiggle.
    /// for the unnecessary diagnostics, so do not underline them.
    fn underline_diagnostic(&self, _diagnostic: &lsp::Diagnostic) -> bool {
        true
    }

    /// Post-processes completions provided by the language server.
    async fn process_completions(&self, _: &mut [lsp::CompletionItem]) {}

    fn diagnostic_message_to_markdown(&self, _message: &str) -> Option<String> {
        None
    }

    async fn labels_for_completions(
        self: Arc<Self>,
        completions: &[lsp::CompletionItem],
        language: &Arc<Language>,
    ) -> Result<Vec<Option<CodeLabel>>> {
        let mut labels = Vec::new();
        for (ix, completion) in completions.iter().enumerate() {
            let label = self.label_for_completion(completion, language).await;
            if let Some(label) = label {
                labels.resize(ix + 1, None);
                *labels.last_mut().unwrap() = Some(label);
            }
        }
        Ok(labels)
    }

    async fn label_for_completion(
        &self,
        _: &lsp::CompletionItem,
        _: &Arc<Language>,
    ) -> Option<CodeLabel> {
        None
    }

    async fn labels_for_symbols(
        self: Arc<Self>,
        symbols: &[Symbol],
        language: &Arc<Language>,
    ) -> Result<Vec<Option<CodeLabel>>> {
        let mut labels = Vec::new();
        for (ix, symbol) in symbols.iter().enumerate() {
            let label = self.label_for_symbol(symbol, language).await;
            if let Some(label) = label {
                labels.resize(ix + 1, None);
                *labels.last_mut().unwrap() = Some(label);
            }
        }
        Ok(labels)
    }

    async fn label_for_symbol(
        &self,
        _symbol: &Symbol,
        _language: &Arc<Language>,
    ) -> Option<CodeLabel> {
        None
    }

    /// Returns initialization options that are going to be sent to a LSP server as a part of [`lsp::InitializeParams`]
    async fn initialization_options(
        self: Arc<Self>,
        _: &Arc<dyn LspAdapterDelegate>,
        _cx: &mut AsyncApp,
    ) -> Result<Option<Value>> {
        Ok(None)
    }

    /// Returns the JSON schema of the initialization_options for the language server.
    async fn initialization_options_schema(
        self: Arc<Self>,
        _delegate: &Arc<dyn LspAdapterDelegate>,
        _cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        _cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        None
    }

    /// Returns the JSON schema of the settings for the language server.
    /// This corresponds to the `settings` field in `LspSettings`, which is used
    /// to respond to `workspace/configuration` requests from the language server.
    async fn settings_schema(
        self: Arc<Self>,
        _delegate: &Arc<dyn LspAdapterDelegate>,
        _cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        _cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        None
    }

    async fn workspace_configuration(
        self: Arc<Self>,
        _: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: Option<Uri>,
        _cx: &mut AsyncApp,
    ) -> Result<Value> {
        Ok(serde_json::json!({}))
    }

    async fn additional_initialization_options(
        self: Arc<Self>,
        _target_language_server_id: LanguageServerName,
        _: &Arc<dyn LspAdapterDelegate>,
    ) -> Result<Option<Value>> {
        Ok(None)
    }

    async fn additional_workspace_configuration(
        self: Arc<Self>,
        _target_language_server_id: LanguageServerName,
        _: &Arc<dyn LspAdapterDelegate>,
        _cx: &mut AsyncApp,
    ) -> Result<Option<Value>> {
        Ok(None)
    }

    /// Returns a list of code actions supported by a given LspAdapter
    fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        None
    }

    fn disk_based_diagnostic_sources(&self) -> Vec<String> {
        Default::default()
    }

    fn disk_based_diagnostics_progress_token(&self) -> Option<String> {
        None
    }

    fn language_ids(&self) -> HashMap<LanguageName, String> {
        HashMap::default()
    }

    /// Support custom initialize params.
    fn prepare_initialize_params(
        &self,
        original: InitializeParams,
        _: &App,
    ) -> Result<InitializeParams> {
        Ok(original)
    }

    fn client_command(
        &self,
        _command_name: &str,
        _arguments: &[serde_json::Value],
    ) -> Option<ClientCommand> {
        None
    }

    /// Method only implemented by the default JSON language server adapter.
    /// Used to provide dynamic reloading of the JSON schemas used to
    /// provide autocompletion and diagnostics in Mav setting and keybind
    /// files
    fn is_primary_mav_json_schema_adapter(&self) -> bool {
        false
    }

    /// True for the extension adapter and false otherwise.
    fn is_extension(&self) -> bool {
        false
    }

    /// Called when a user responds to a ShowMessageRequest from this language server.
    /// This allows adapters to intercept preference selections (like "Always" or "Never")
    /// for settings that should be persisted to Mav's settings file.
    fn process_prompt_response(&self, _context: &PromptResponseContext, _cx: &mut AsyncApp) {}
}

pub trait LspInstaller {
    type BinaryVersion;
    fn check_if_user_installed(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> impl Future<Output = Option<LanguageServerBinary>> {
        async { None }
    }

    fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        pre_release: bool,
        cx: &mut AsyncApp,
    ) -> impl Future<Output = Result<Self::BinaryVersion>>;

    fn check_if_version_installed(
        &self,
        _version: &Self::BinaryVersion,
        _container_dir: &PathBuf,
        _delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Option<LanguageServerBinary>> + use<Self> {
        async { None }
    }

    fn fetch_server_binary(
        &self,
        latest_version: Self::BinaryVersion,
        container_dir: PathBuf,
        _delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<Self>;

    fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        delegate: &dyn LspAdapterDelegate,
    ) -> impl Future<Output = Option<LanguageServerBinary>>;
}

#[async_trait(?Send)]
pub trait DynLspInstaller {
    async fn try_fetch_server_binary(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        container_dir: PathBuf,
        pre_release: bool,
        cx: &mut AsyncApp,
    ) -> Result<LanguageServerBinary>;

    fn get_language_server_command(
        self: Arc<Self>,
        delegate: Arc<dyn LspAdapterDelegate>,
        toolchains: Option<Toolchain>,
        binary_options: LanguageServerBinaryOptions,
        cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        cx: AsyncApp,
    ) -> LanguageServerBinaryLocations;
}

#[async_trait(?Send)]
impl<LI, BinaryVersion> DynLspInstaller for LI
where
    BinaryVersion: Send + Sync,
    LI: LspInstaller<BinaryVersion = BinaryVersion> + LspAdapter,
{
    async fn try_fetch_server_binary(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        container_dir: PathBuf,
        pre_release: bool,
        cx: &mut AsyncApp,
    ) -> Result<LanguageServerBinary> {
        let name = self.name();

        log::debug!("fetching latest version of language server {:?}", name.0);
        delegate.update_status(name.clone(), BinaryStatus::CheckingForUpdate);

        let latest_version = self
            .fetch_latest_server_version(delegate, pre_release, cx)
            .await?;

        if let Some(binary) = cx
            .background_executor()
            .spawn(self.check_if_version_installed(&latest_version, &container_dir, &delegate))
            .await
        {
            log::debug!("language server {:?} is already installed", name.0);
            delegate.update_status(name.clone(), BinaryStatus::None);
            Ok(binary)
        } else {
            log::debug!("downloading language server {:?}", name.0);
            delegate.update_status(name.clone(), BinaryStatus::Downloading);
            let binary = cx
                .background_executor()
                .spawn(self.fetch_server_binary(latest_version, container_dir, delegate))
                .await;

            delegate.update_status(name.clone(), BinaryStatus::None);
            binary
        }
    }
    fn get_language_server_command(
        self: Arc<Self>,
        delegate: Arc<dyn LspAdapterDelegate>,
        toolchain: Option<Toolchain>,
        binary_options: LanguageServerBinaryOptions,
        mut cached_binary: OwnedMutexGuard<Option<(bool, LanguageServerBinary)>>,
        mut cx: AsyncApp,
    ) -> LanguageServerBinaryLocations {
        async move {
            let cached_binary_deref = cached_binary.deref_mut();
            // First we check whether the adapter can give us a user-installed binary.
            // If so, we do *not* want to cache that, because each worktree might give us a different
            // binary:
            //
            //      worktree 1: user-installed at `.bin/gopls`
            //      worktree 2: user-installed at `~/bin/gopls`
            //      worktree 3: no gopls found in PATH -> fallback to Mav installation
            //
            // We only want to cache when we fall back to the global one,
            // because we don't want to download and overwrite our global one
            // for each worktree we might have open.
            if binary_options.allow_path_lookup
                && let Some(binary) = self
                    .check_if_user_installed(&delegate, toolchain, &mut cx)
                    .await
            {
                log::info!(
                    "found user-installed language server for {}. path: {:?}, arguments: {:?}",
                    self.name().0,
                    binary.path,
                    binary.arguments
                );
                return (Ok(binary), None);
            }

            if let Some((pre_release, cached_binary)) = cached_binary_deref
                && *pre_release == binary_options.pre_release
            {
                return (Ok(cached_binary.clone()), None);
            }

            if !binary_options.allow_binary_download {
                return (
                    Err(anyhow::anyhow!("downloading language servers disabled")),
                    None,
                );
            }

            let Some(container_dir) = delegate.language_server_download_dir(&self.name()).await
            else {
                return (
                    Err(anyhow::anyhow!("no language server download dir defined")),
                    None,
                );
            };

            let last_downloaded_binary = self
                .cached_server_binary(container_dir.to_path_buf(), delegate.as_ref())
                .await
                .context(
                    "did not find existing language server binary, falling back to downloading",
                );
            let download_binary = async move {
                let mut binary = self
                    .try_fetch_server_binary(
                        &delegate,
                        container_dir.to_path_buf(),
                        binary_options.pre_release,
                        &mut cx,
                    )
                    .await;

                if let Err(error) = binary.as_ref() {
                    if let Some(prev_downloaded_binary) = self
                        .cached_server_binary(container_dir.to_path_buf(), delegate.as_ref())
                        .await
                    {
                        log::info!(
                            "failed to fetch newest version of language server {:?}. \
                            error: {:?}, falling back to using {:?}",
                            self.name(),
                            error,
                            prev_downloaded_binary.path
                        );
                        binary = Ok(prev_downloaded_binary);
                    } else {
                        delegate.update_status(
                            self.name(),
                            BinaryStatus::Failed {
                                error: format!("{error:?}"),
                            },
                        );
                    }
                }

                if let Ok(binary) = &binary {
                    *cached_binary = Some((binary_options.pre_release, binary.clone()));
                }

                binary
            }
            .boxed_local();
            (last_downloaded_binary, Some(download_binary))
        }
        .boxed_local()
    }
}
