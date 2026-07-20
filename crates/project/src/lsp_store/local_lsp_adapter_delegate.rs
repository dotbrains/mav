use super::{LocalLspStore, LspStore, LspStoreEvent};
use crate::{ProjectEnvironment, Worktree};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use collections::HashMap;
use fs::Fs;
use futures::future::Shared;
use gpui::{App, Entity, Task, WeakEntity};
use http_client::HttpClient;
use language::{BinaryStatus, LanguageRegistry, LspAdapter, LspAdapterDelegate};
use lsp::{LanguageServerBinary, LanguageServerName};
use node_runtime::read_package_installed_version;
use semver::Version;
use settings::WorktreeId;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::Arc,
};
use util::{ResultExt, rel_path::RelPath};

pub struct LocalLspAdapterDelegate {
    lsp_store: WeakEntity<LspStore>,
    worktree: worktree::Snapshot,
    fs: Arc<dyn Fs>,
    http_client: Arc<dyn HttpClient>,
    language_registry: Arc<LanguageRegistry>,
    load_shell_env_task: Shared<Task<Option<HashMap<String, String>>>>,
}

impl LocalLspAdapterDelegate {
    pub fn new(
        language_registry: Arc<LanguageRegistry>,
        environment: &Entity<ProjectEnvironment>,
        lsp_store: WeakEntity<LspStore>,
        worktree: &Entity<Worktree>,
        http_client: Arc<dyn HttpClient>,
        fs: Arc<dyn Fs>,
        cx: &mut App,
    ) -> Arc<Self> {
        let load_shell_env_task =
            environment.update(cx, |env, cx| env.worktree_environment(worktree.clone(), cx));

        Arc::new(Self {
            lsp_store,
            worktree: worktree.read(cx).snapshot(),
            fs,
            http_client,
            language_registry,
            load_shell_env_task,
        })
    }

    pub fn from_local_lsp(
        local: &LocalLspStore,
        worktree: &Entity<Worktree>,
        cx: &mut App,
    ) -> Arc<Self> {
        Self::new(
            local.languages.clone(),
            &local.environment,
            local.weak.clone(),
            worktree,
            local.http_client.clone(),
            local.fs.clone(),
            cx,
        )
    }
}

#[async_trait]
impl LspAdapterDelegate for LocalLspAdapterDelegate {
    fn show_notification(&self, message: &str, cx: &mut App) {
        self.lsp_store
            .update(cx, |_, cx| {
                cx.emit(LspStoreEvent::Notification(message.to_owned()))
            })
            .ok();
    }

    fn http_client(&self) -> Arc<dyn HttpClient> {
        self.http_client.clone()
    }

    fn worktree_id(&self) -> WorktreeId {
        self.worktree.id()
    }

    fn worktree_root_path(&self) -> &Path {
        self.worktree.abs_path().as_ref()
    }

    fn resolve_relative_path(&self, path: PathBuf) -> PathBuf {
        self.worktree.resolve_relative_path(path)
    }

    async fn shell_env(&self) -> HashMap<String, String> {
        let task = self.load_shell_env_task.clone();
        task.await.unwrap_or_default()
    }

    async fn npm_package_installed_version(
        &self,
        package_name: &str,
    ) -> Result<Option<(PathBuf, Version)>> {
        let local_package_directory = self.worktree_root_path();
        let node_modules_directory = local_package_directory.join("node_modules");

        if let Some(version) =
            read_package_installed_version(node_modules_directory.clone(), package_name).await?
        {
            return Ok(Some((node_modules_directory, version)));
        }
        let Some(npm) = self.which("npm".as_ref()).await else {
            log::warn!(
                "Failed to find npm executable for {:?}",
                local_package_directory
            );
            return Ok(None);
        };

        let env = self.shell_env().await;
        let output = util::command::new_command(&npm)
            .args(["root", "-g"])
            .envs(env)
            .current_dir(local_package_directory)
            .output()
            .await?;
        let global_node_modules =
            PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());

        if let Some(version) =
            read_package_installed_version(global_node_modules.clone(), package_name).await?
        {
            return Ok(Some((global_node_modules, version)));
        }
        Ok(None)
    }

    async fn which(&self, command: &OsStr) -> Option<PathBuf> {
        let mut worktree_abs_path = self.worktree_root_path().to_path_buf();
        if self.fs.is_file(&worktree_abs_path).await {
            worktree_abs_path.pop();
        }

        let env = self.shell_env().await;

        let shell_path = env.get("PATH").cloned();

        which::which_in(command, shell_path.as_ref(), worktree_abs_path).ok()
    }

    async fn try_exec(&self, command: LanguageServerBinary) -> Result<()> {
        let mut working_dir = self.worktree_root_path().to_path_buf();
        if self.fs.is_file(&working_dir).await {
            working_dir.pop();
        }
        let output = util::command::new_command(&command.path)
            .args(command.arguments)
            .envs(command.env.clone().unwrap_or_default())
            .current_dir(working_dir)
            .output()
            .await?;

        anyhow::ensure!(
            output.status.success(),
            "{}, stdout: {:?}, stderr: {:?}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn update_status(&self, server_name: LanguageServerName, status: BinaryStatus) {
        self.language_registry
            .update_lsp_binary_status(server_name, status);
    }

    fn registered_lsp_adapters(&self) -> Vec<Arc<dyn LspAdapter>> {
        self.language_registry
            .all_lsp_adapters()
            .into_iter()
            .map(|adapter| adapter.adapter.clone() as Arc<dyn LspAdapter>)
            .collect()
    }

    async fn language_server_download_dir(&self, name: &LanguageServerName) -> Option<Arc<Path>> {
        let dir = self.language_registry.language_server_download_dir(name)?;

        if !dir.exists() {
            smol::fs::create_dir_all(&dir)
                .await
                .context("failed to create container directory")
                .log_err()?;
        }

        Some(dir)
    }

    async fn read_text_file(&self, path: &RelPath) -> Result<String> {
        let entry = self
            .worktree
            .entry_for_path(path)
            .with_context(|| format!("no worktree entry for path {path:?}"))?;
        let abs_path = self.worktree.absolutize(&entry.path);
        self.fs.load(&abs_path).await
    }
}
