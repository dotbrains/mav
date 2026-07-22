#[path = "copilot/auth.rs"]
mod auth;
#[path = "copilot/buffers.rs"]
mod buffers;
mod copilot_edit_prediction_delegate;
#[path = "copilot/lifecycle.rs"]
mod lifecycle;
pub mod request;
#[path = "copilot/status.rs"]
mod status;

#[cfg(test)]
#[path = "copilot/tests.rs"]
mod tests;

use crate::request::{
    DidFocus, DidFocusParams, FormattingOptions, InlineCompletionContext,
    InlineCompletionTriggerKind, InlineCompletions, NextEditSuggestions,
};
use ::fs::Fs;
use anyhow::{Context as _, Result, anyhow};
use collections::{HashMap, HashSet};
use command_palette_hooks::CommandPaletteFilter;
use futures::future;
use futures::{Future, FutureExt, TryFutureExt, channel::oneshot, future::Shared, select_biased};
use gpui::{
    App, AppContext as _, AsyncApp, Context, Entity, EntityId, EventEmitter, Global, Subscription,
    Task, WeakEntity, actions,
};
use language::language_settings::{AllLanguageSettings, CopilotSettings};
use language::{
    Anchor, Bias, Buffer, BufferSnapshot, Language, PointUtf16, ToPointUtf16,
    language_settings::{EditPredictionProvider, all_language_settings},
    point_from_lsp, point_to_lsp,
};
use lsp::{LanguageServer, LanguageServerBinary, LanguageServerId, LanguageServerName};
use node_runtime::{NodeRuntime, VersionStrategy};
use parking_lot::Mutex;
use project::project_settings::ProjectSettings;
use project::{DisableAiSettings, Project};
use request::DidChangeStatus;
use serde_json::json;
use settings::{Settings, SettingsStore};
use std::{
    any::TypeId,
    collections::hash_map::Entry,
    env,
    ffi::OsString,
    mem,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};
use sum_tree::Dimensions;
use util::{ResultExt, fs::remove_matching};
use workspace::AppState;

pub use crate::copilot_edit_prediction_delegate::CopilotEditPredictionDelegate;

actions!(
    copilot,
    [
        /// Requests a code completion suggestion from Copilot.
        Suggest,
        /// Cycles to the next Copilot suggestion.
        NextSuggestion,
        /// Cycles to the previous Copilot suggestion.
        PreviousSuggestion,
        /// Reinstalls the Copilot language server.
        Reinstall,
        /// Signs in to GitHub Copilot.
        SignIn,
        /// Signs out of GitHub Copilot.
        SignOut
    ]
);

enum CopilotServer {
    Disabled,
    Starting { task: Shared<Task<()>> },
    Error(Arc<str>),
    Running(RunningCopilotServer),
}

impl CopilotServer {
    fn as_authenticated(&mut self) -> Result<&mut RunningCopilotServer> {
        let server = self.as_running()?;
        anyhow::ensure!(
            matches!(server.sign_in_status, SignInStatus::Authorized),
            "must sign in before using copilot"
        );
        Ok(server)
    }

    fn as_running(&mut self) -> Result<&mut RunningCopilotServer> {
        match self {
            CopilotServer::Starting { .. } => anyhow::bail!("copilot is still starting"),
            CopilotServer::Disabled => anyhow::bail!("copilot is disabled"),
            CopilotServer::Error(error) => {
                anyhow::bail!("copilot was not started because of an error: {error}")
            }
            CopilotServer::Running(server) => Ok(server),
        }
    }
}

struct RunningCopilotServer {
    lsp: Arc<LanguageServer>,
    sign_in_status: SignInStatus,
    registered_buffers: HashMap<EntityId, RegisteredBuffer>,
}

#[derive(Clone, Debug)]
enum SignInStatus {
    Authorized,
    Unauthorized,
    SigningIn {
        prompt: Option<request::PromptUserDeviceFlow>,
        task: Shared<Task<Result<(), Arc<anyhow::Error>>>>,
    },
    SignedOut {
        awaiting_signing_in: bool,
    },
}

#[derive(Debug, Clone)]
pub enum Status {
    Starting {
        task: Shared<Task<()>>,
    },
    Error(Arc<str>),
    Disabled,
    SignedOut {
        awaiting_signing_in: bool,
    },
    SigningIn {
        prompt: Option<request::PromptUserDeviceFlow>,
    },
    Unauthorized,
    Authorized,
}

impl Status {
    pub fn is_authorized(&self) -> bool {
        matches!(self, Status::Authorized)
    }

    pub fn is_configured(&self) -> bool {
        matches!(
            self,
            Status::Starting { .. }
                | Status::Error(_)
                | Status::SigningIn { .. }
                | Status::Authorized
        )
    }
}

struct RegisteredBuffer {
    uri: lsp::Uri,
    language_id: String,
    snapshot: BufferSnapshot,
    snapshot_version: i32,
    _subscriptions: [gpui::Subscription; 2],
    pending_buffer_change: Task<Option<()>>,
}

impl RegisteredBuffer {
    fn report_changes(
        &mut self,
        buffer: &Entity<Buffer>,
        cx: &mut Context<Copilot>,
    ) -> oneshot::Receiver<(i32, BufferSnapshot)> {
        let (done_tx, done_rx) = oneshot::channel();

        if buffer.read(cx).version() == self.snapshot.version {
            let _ = done_tx.send((self.snapshot_version, self.snapshot.clone()));
        } else {
            let buffer = buffer.downgrade();
            let id = buffer.entity_id();
            let prev_pending_change =
                mem::replace(&mut self.pending_buffer_change, Task::ready(None));
            self.pending_buffer_change = cx.spawn(async move |copilot, cx| {
                prev_pending_change.await;

                let old_version = copilot
                    .update(cx, |copilot, _| {
                        let server = copilot.server.as_authenticated().log_err()?;
                        let buffer = server.registered_buffers.get_mut(&id)?;
                        Some(buffer.snapshot.version.clone())
                    })
                    .ok()??;
                let new_snapshot = buffer.read_with(cx, |buffer, _| buffer.snapshot()).ok()?;

                let content_changes = cx
                    .background_spawn({
                        let new_snapshot = new_snapshot.clone();
                        async move {
                            new_snapshot
                                .edits_since::<Dimensions<PointUtf16, usize>>(&old_version)
                                .map(|edit| {
                                    let edit_start = edit.new.start.0;
                                    let edit_end = edit_start + (edit.old.end.0 - edit.old.start.0);
                                    let new_text = new_snapshot
                                        .text_for_range(edit.new.start.1..edit.new.end.1)
                                        .collect();
                                    lsp::TextDocumentContentChangeEvent {
                                        range: Some(lsp::Range::new(
                                            point_to_lsp(edit_start),
                                            point_to_lsp(edit_end),
                                        )),
                                        range_length: None,
                                        text: new_text,
                                    }
                                })
                                .collect::<Vec<_>>()
                        }
                    })
                    .await;

                copilot
                    .update(cx, |copilot, _| {
                        let server = copilot.server.as_authenticated().log_err()?;
                        let buffer = server.registered_buffers.get_mut(&id)?;
                        if !content_changes.is_empty() {
                            buffer.snapshot_version += 1;
                            buffer.snapshot = new_snapshot;
                            server
                                .lsp
                                .notify::<lsp::notification::DidChangeTextDocument>(
                                    lsp::DidChangeTextDocumentParams {
                                        text_document: lsp::VersionedTextDocumentIdentifier::new(
                                            buffer.uri.clone(),
                                            buffer.snapshot_version,
                                        ),
                                        content_changes,
                                    },
                                )
                                .ok();
                        }
                        let _ = done_tx.send((buffer.snapshot_version, buffer.snapshot.clone()));
                        Some(())
                    })
                    .ok()?;

                Some(())
            });
        }

        done_rx
    }
}

#[derive(Debug)]
pub struct Completion {
    pub uuid: String,
    pub range: Range<Anchor>,
    pub text: String,
}

pub struct Copilot {
    fs: Arc<dyn Fs>,
    node_runtime: NodeRuntime,
    server: CopilotServer,
    buffers: HashSet<WeakEntity<Buffer>>,
    server_id: LanguageServerId,
    _subscriptions: Vec<Subscription>,
}

pub enum Event {
    CopilotAuthSignedIn,
    CopilotAuthSignedOut,
}

impl EventEmitter<Event> for Copilot {}

#[derive(Clone)]
pub struct GlobalCopilotAuth(pub Entity<Copilot>);

impl GlobalCopilotAuth {
    pub fn set_global(
        server_id: LanguageServerId,
        fs: Arc<dyn Fs>,
        node_runtime: NodeRuntime,
        cx: &mut App,
    ) -> GlobalCopilotAuth {
        let auth =
            GlobalCopilotAuth(cx.new(|cx| Copilot::new(None, server_id, fs, node_runtime, cx)));
        cx.set_global(auth.clone());
        auth
    }
    pub fn try_global(cx: &mut App) -> Option<&GlobalCopilotAuth> {
        cx.try_global()
    }

    pub fn try_get_or_init(app_state: Arc<AppState>, cx: &mut App) -> Option<GlobalCopilotAuth> {
        let ai_enabled = !DisableAiSettings::get(None, cx).disable_ai;

        if let Some(copilot) = cx.try_global::<Self>().cloned() {
            if ai_enabled {
                Some(copilot)
            } else {
                cx.remove_global::<Self>();
                None
            }
        } else if ai_enabled {
            Some(Self::set_global(
                app_state.languages.next_language_server_id(),
                app_state.fs.clone(),
                app_state.node_runtime.clone(),
                cx,
            ))
        } else {
            None
        }
    }
}
impl Global for GlobalCopilotAuth {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletionSource {
    NextEditSuggestion,
    InlineCompletion,
}

/// Copilot's NextEditSuggestion response, with coordinates converted to Anchors.
#[derive(Clone)]
pub(crate) struct CopilotEditPrediction {
    pub(crate) buffer: Entity<Buffer>,
    pub(crate) range: Range<Anchor>,
    pub(crate) text: String,
    pub(crate) command: Option<lsp::Command>,
    pub(crate) snapshot: BufferSnapshot,
    pub(crate) source: CompletionSource,
}

fn id_for_language(language: Option<&Arc<Language>>) -> String {
    language
        .map(|language| language.lsp_id())
        .unwrap_or_else(|| "plaintext".to_string())
}

fn uri_for_buffer(buffer: &Entity<Buffer>, cx: &App) -> Result<lsp::Uri, ()> {
    if let Some(file) = buffer.read(cx).file().and_then(|file| file.as_local()) {
        lsp::Uri::from_file_path(file.abs_path(cx))
    } else {
        format!("buffer://{}", buffer.entity_id())
            .parse()
            .map_err(|_| ())
    }
}

fn notify_did_change_config_to_server(
    server: &Arc<LanguageServer>,
    cx: &mut Context<Copilot>,
) -> std::result::Result<(), anyhow::Error> {
    let copilot_settings = all_language_settings(None, cx)
        .edit_predictions
        .copilot
        .clone();

    if let Some(copilot_chat) = copilot_chat::CopilotChat::global(cx) {
        copilot_chat.update(cx, |chat, cx| {
            chat.set_configuration(
                copilot_chat::CopilotChatConfiguration {
                    enterprise_uri: copilot_settings.enterprise_uri.clone(),
                },
                cx,
            );
        });
    }

    let settings = json!({
        "http": {
            "proxy": copilot_settings.proxy,
            "proxyStrictSSL": !copilot_settings.proxy_no_verify.unwrap_or(false)
        },
        "github-enterprise": {
            "uri": copilot_settings.enterprise_uri
        }
    });

    server
        .notify::<lsp::notification::DidChangeConfiguration>(lsp::DidChangeConfigurationParams {
            settings,
        })
        .ok();
    Ok(())
}

/// Notify Copilot Chat after the Copilot LSP reports an auth state change.
/// This replaces watching the SDK's token files, which is unreliable for
/// SQLite backed auth because writes may go through WAL files.
fn notify_copilot_chat_auth_changed(cx: &mut Context<Copilot>) {
    if let Some(copilot_chat) = copilot_chat::CopilotChat::global(cx) {
        copilot_chat.update(cx, |chat, cx| chat.reload_auth(cx));
    }
}

async fn clear_copilot_dir() {
    remove_matching(paths::copilot_dir(), |_| true).await
}

async fn clear_copilot_config_dir() {
    remove_matching(copilot_chat::copilot_chat_config_dir(), |_| true).await
}

async fn get_copilot_lsp(fs: Arc<dyn Fs>, node_runtime: NodeRuntime) -> anyhow::Result<PathBuf> {
    const PACKAGE_NAME: &str = "@github/copilot-language-server";
    const SERVER_PATH: &str =
        "node_modules/@github/copilot-language-server/dist/language-server.js";

    let latest_version = node_runtime
        .npm_package_latest_version(PACKAGE_NAME)
        .await?;
    let server_path = paths::copilot_dir().join(SERVER_PATH);
    let binary_path = copilot_lsp_native_binary_path()?;

    fs.create_dir(paths::copilot_dir()).await?;

    let should_install = !fs.is_file(&binary_path).await
        || node_runtime
            .should_install_npm_package(
                PACKAGE_NAME,
                &server_path,
                paths::copilot_dir(),
                VersionStrategy::Latest(&latest_version),
            )
            .await;
    if should_install {
        node_runtime
            .npm_install_latest_packages(paths::copilot_dir(), &[PACKAGE_NAME])
            .await?;
    }

    if fs.is_file(&binary_path).await {
        return Ok(binary_path);
    }

    anyhow::bail!("GitHub Copilot native language server binary was not installed")
}

fn copilot_lsp_native_binary_path() -> anyhow::Result<PathBuf> {
    let platform = match env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "win32",
        platform => anyhow::bail!("unsupported Copilot language server platform: {platform}"),
    };
    let architecture = match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        architecture => {
            anyhow::bail!("unsupported Copilot language server architecture: {architecture}")
        }
    };

    let package_name = format!("copilot-language-server-{platform}-{architecture}");

    let executable_name = if cfg!(target_os = "windows") {
        "copilot-language-server.exe"
    } else {
        "copilot-language-server"
    };
    Ok(paths::copilot_dir()
        .join("node_modules")
        .join("@github")
        .join(package_name)
        .join(executable_name))
}
