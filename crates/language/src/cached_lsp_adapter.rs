use super::*;

type ServerBinaryCache = futures::lock::Mutex<Option<(bool, LanguageServerBinary)>>;
type DownloadableLanguageServerBinary = LocalBoxFuture<'static, Result<LanguageServerBinary>>;
pub type LanguageServerBinaryLocations = LocalBoxFuture<
    'static,
    (
        Result<LanguageServerBinary>,
        Option<DownloadableLanguageServerBinary>,
    ),
>;
/// Represents a Language Server, with certain cached sync properties.
/// Uses [`LspAdapter`] under the hood, but calls all 'static' methods
/// once at startup, and caches the results.
pub struct CachedLspAdapter {
    pub name: LanguageServerName,
    pub disk_based_diagnostic_sources: Vec<String>,
    pub disk_based_diagnostics_progress_token: Option<String>,
    language_ids: HashMap<LanguageName, String>,
    pub adapter: Arc<dyn LspAdapter>,
    cached_binary: Arc<ServerBinaryCache>,
}

impl Debug for CachedLspAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedLspAdapter")
            .field("name", &self.name)
            .field(
                "disk_based_diagnostic_sources",
                &self.disk_based_diagnostic_sources,
            )
            .field(
                "disk_based_diagnostics_progress_token",
                &self.disk_based_diagnostics_progress_token,
            )
            .field("language_ids", &self.language_ids)
            .finish_non_exhaustive()
    }
}

impl CachedLspAdapter {
    pub fn new(adapter: Arc<dyn LspAdapter>) -> Arc<Self> {
        let name = adapter.name();
        let disk_based_diagnostic_sources = adapter.disk_based_diagnostic_sources();
        let disk_based_diagnostics_progress_token = adapter.disk_based_diagnostics_progress_token();
        let language_ids = adapter.language_ids();

        Arc::new(CachedLspAdapter {
            name,
            disk_based_diagnostic_sources,
            disk_based_diagnostics_progress_token,
            language_ids,
            adapter,
            cached_binary: Default::default(),
        })
    }

    pub fn name(&self) -> LanguageServerName {
        self.adapter.name()
    }

    pub async fn get_language_server_command(
        self: Arc<Self>,
        delegate: Arc<dyn LspAdapterDelegate>,
        toolchains: Option<Toolchain>,
        binary_options: LanguageServerBinaryOptions,
        cx: &mut AsyncApp,
    ) -> LanguageServerBinaryLocations {
        let cached_binary = self.cached_binary.clone().lock_owned().await;
        self.adapter.clone().get_language_server_command(
            delegate,
            toolchains,
            binary_options,
            cached_binary,
            cx.clone(),
        )
    }

    pub fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.adapter.code_action_kinds()
    }

    pub fn process_diagnostics(
        &self,
        params: &mut lsp::PublishDiagnosticsParams,
        server_id: LanguageServerId,
    ) {
        self.adapter.process_diagnostics(params, server_id)
    }

    pub fn retain_old_diagnostic(&self, previous_diagnostic: &Diagnostic) -> bool {
        self.adapter.retain_old_diagnostic(previous_diagnostic)
    }

    pub fn underline_diagnostic(&self, diagnostic: &lsp::Diagnostic) -> bool {
        self.adapter.underline_diagnostic(diagnostic)
    }

    pub fn diagnostic_message_to_markdown(&self, message: &str) -> Option<String> {
        self.adapter.diagnostic_message_to_markdown(message)
    }

    pub async fn process_completions(&self, completion_items: &mut [lsp::CompletionItem]) {
        self.adapter.process_completions(completion_items).await
    }

    pub async fn labels_for_completions(
        &self,
        completion_items: &[lsp::CompletionItem],
        language: &Arc<Language>,
    ) -> Result<Vec<Option<CodeLabel>>> {
        self.adapter
            .clone()
            .labels_for_completions(completion_items, language)
            .await
    }

    pub async fn labels_for_symbols(
        &self,
        symbols: &[Symbol],
        language: &Arc<Language>,
    ) -> Result<Vec<Option<CodeLabel>>> {
        self.adapter
            .clone()
            .labels_for_symbols(symbols, language)
            .await
    }

    pub fn language_id(&self, language_name: &LanguageName) -> String {
        self.language_ids
            .get(language_name)
            .cloned()
            .unwrap_or_else(|| language_name.lsp_id())
    }

    pub async fn initialization_options_schema(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        self.adapter
            .clone()
            .initialization_options_schema(
                delegate,
                self.cached_binary.clone().lock_owned().await,
                cx,
            )
            .await
    }

    pub async fn settings_schema(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Option<serde_json::Value> {
        self.adapter
            .clone()
            .settings_schema(delegate, self.cached_binary.clone().lock_owned().await, cx)
            .await
    }

    pub fn process_prompt_response(&self, context: &PromptResponseContext, cx: &mut AsyncApp) {
        self.adapter.process_prompt_response(context, cx)
    }
}
