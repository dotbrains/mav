use anyhow::Result;
use async_trait::async_trait;
use gpui::AsyncApp;
use language::{LspAdapter, LspAdapterDelegate, LspInstaller, Toolchain};
use lsp::{CodeActionKind, LanguageServerBinary, LanguageServerName};
use std::{future::Future, path::PathBuf, sync::Arc};

pub struct SshLspAdapter {
    name: LanguageServerName,
    binary: LanguageServerBinary,
    initialization_options: Option<String>,
    code_action_kinds: Option<Vec<CodeActionKind>>,
}

impl SshLspAdapter {
    pub fn new(
        name: LanguageServerName,
        binary: LanguageServerBinary,
        initialization_options: Option<String>,
        code_action_kinds: Option<String>,
    ) -> Self {
        Self {
            name,
            binary,
            initialization_options,
            code_action_kinds: code_action_kinds
                .as_ref()
                .and_then(|c| serde_json::from_str(c).ok()),
        }
    }
}

impl LspInstaller for SshLspAdapter {
    type BinaryVersion = ();

    async fn check_if_user_installed(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        Some(self.binary.clone())
    }

    async fn cached_server_binary(
        &self,
        _: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        None
    }

    async fn fetch_latest_server_version(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<()> {
        anyhow::bail!("SshLspAdapter does not support fetch_latest_server_version")
    }

    fn fetch_server_binary(
        &self,
        _: (),
        _: PathBuf,
        _: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        async { anyhow::bail!("SshLspAdapter does not support fetch_server_binary") }
    }
}

#[async_trait(?Send)]
impl LspAdapter for SshLspAdapter {
    fn name(&self) -> LanguageServerName {
        self.name.clone()
    }

    async fn initialization_options(
        self: Arc<Self>,
        _: &Arc<dyn LspAdapterDelegate>,
        _: &mut AsyncApp,
    ) -> Result<Option<serde_json::Value>> {
        let Some(options) = &self.initialization_options else {
            return Ok(None);
        };
        let result = serde_json::from_str(options)?;
        Ok(result)
    }

    fn code_action_kinds(&self) -> Option<Vec<CodeActionKind>> {
        self.code_action_kinds.clone()
    }
}
