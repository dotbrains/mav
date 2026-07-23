pub mod extension;
pub mod registry;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use collections::{HashMap, HashSet};
use context_server::oauth::{self, McpOAuthTokenProvider, OAuthDiscovery, OAuthSession};
use context_server::transport::{HttpTransport, TransportError};
use context_server::{ContextServer, ContextServerCommand, ContextServerId};
use credentials_provider::CredentialsProvider;
use futures::future::Either;
use futures::{FutureExt as _, StreamExt as _, future::join_all};
use gpui::{
    App, AsyncApp, Context, Entity, EventEmitter, Subscription, Task, TaskExt, WeakEntity, actions,
};
use http_client::HttpClient;
use itertools::Itertools;
use rand::Rng as _;
use registry::ContextServerDescriptorRegistry;
use remote::{Interactive, RemoteClient};
use rpc::{AnyProtoClient, TypedEnvelope, proto};
use settings::{Settings as _, SettingsStore};
use util::{ResultExt as _, rel_path::RelPath};

use crate::{
    DisableAiSettings, Project,
    project_settings::{ContextServerSettings, OAuthClientSettings, ProjectSettings},
    worktree_store::{WorktreeStore, WorktreeStoreEvent},
};

/// Maximum timeout for context server requests
/// Prevents extremely large timeout values from tying up resources indefinitely.
const MAX_TIMEOUT_SECS: u64 = 600; // 10 minutes

pub fn init(cx: &mut App) {
    extension::init(cx);
}

actions!(
    context_server,
    [
        /// Restarts the context server.
        Restart
    ]
);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContextServerStatus {
    Starting,
    Running,
    Stopped,
    Error(Arc<str>),
    /// The server returned 401 and OAuth authorization is needed. The UI
    /// should show an "Authenticate" button.
    AuthRequired,
    /// The server has a pre-registered OAuth client_id, but a client_secret
    /// is needed and not available in settings or the keychain.
    ClientSecretRequired {
        error: Option<Arc<str>>,
    },
    /// The OAuth browser flow is in progress — the user has been redirected
    /// to the authorization server and we're waiting for the callback.
    Authenticating,
}

impl ContextServerStatus {
    fn from_state(state: &ContextServerState) -> Self {
        match state {
            ContextServerState::Starting { .. } => ContextServerStatus::Starting,
            ContextServerState::Running { .. } => ContextServerStatus::Running,
            ContextServerState::Stopped { .. } => ContextServerStatus::Stopped,
            ContextServerState::Error { error, .. } => ContextServerStatus::Error(error.clone()),
            ContextServerState::AuthRequired { .. } => ContextServerStatus::AuthRequired,
            ContextServerState::ClientSecretRequired { error, .. } => {
                ContextServerStatus::ClientSecretRequired {
                    error: error.clone(),
                }
            }
            ContextServerState::Authenticating { .. } => ContextServerStatus::Authenticating,
        }
    }
}

enum ContextServerState {
    Starting {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        _task: Task<()>,
    },
    Running {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
    },
    Stopped {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
    },
    Error {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        error: Arc<str>,
    },
    /// The server requires OAuth authorization before it can be used. The
    /// `OAuthDiscovery` holds everything needed to start the browser flow.
    AuthRequired {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        discovery: Arc<OAuthDiscovery>,
    },
    /// A pre-registered client_id is configured but no client_secret was found
    /// in settings or the keychain.
    ClientSecretRequired {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        discovery: Arc<OAuthDiscovery>,
        error: Option<Arc<str>>,
    },
    /// The OAuth browser flow is in progress. The user has been redirected
    /// to the authorization server and we're waiting for the callback.
    Authenticating {
        server: Arc<ContextServer>,
        configuration: Arc<ContextServerConfiguration>,
        _task: Task<()>,
    },
}

impl ContextServerState {
    pub fn server(&self) -> Arc<ContextServer> {
        match self {
            ContextServerState::Starting { server, .. }
            | ContextServerState::Running { server, .. }
            | ContextServerState::Stopped { server, .. }
            | ContextServerState::Error { server, .. }
            | ContextServerState::AuthRequired { server, .. }
            | ContextServerState::ClientSecretRequired { server, .. }
            | ContextServerState::Authenticating { server, .. } => server.clone(),
        }
    }

    pub fn configuration(&self) -> Arc<ContextServerConfiguration> {
        match self {
            ContextServerState::Starting { configuration, .. }
            | ContextServerState::Running { configuration, .. }
            | ContextServerState::Stopped { configuration, .. }
            | ContextServerState::Error { configuration, .. }
            | ContextServerState::AuthRequired { configuration, .. }
            | ContextServerState::ClientSecretRequired { configuration, .. }
            | ContextServerState::Authenticating { configuration, .. } => configuration.clone(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ContextServerConfiguration {
    Custom {
        command: ContextServerCommand,
        remote: bool,
    },
    Extension {
        command: ContextServerCommand,
        settings: serde_json::Value,
        remote: bool,
    },
    Http {
        url: url::Url,
        headers: HashMap<String, String>,
        timeout: Option<u64>,
        oauth: Option<OAuthClientSettings>,
    },
}

impl ContextServerConfiguration {
    pub fn command(&self) -> Option<&ContextServerCommand> {
        match self {
            ContextServerConfiguration::Custom { command, .. } => Some(command),
            ContextServerConfiguration::Extension { command, .. } => Some(command),
            ContextServerConfiguration::Http { .. } => None,
        }
    }

    pub fn has_static_auth_header(&self) -> bool {
        match self {
            ContextServerConfiguration::Http { headers, .. } => headers
                .keys()
                .any(|k| k.eq_ignore_ascii_case("authorization")),
            _ => false,
        }
    }

    pub fn remote(&self) -> bool {
        match self {
            ContextServerConfiguration::Custom { remote, .. } => *remote,
            ContextServerConfiguration::Extension { remote, .. } => *remote,
            ContextServerConfiguration::Http { .. } => false,
        }
    }

    pub async fn from_settings(
        settings: ContextServerSettings,
        id: ContextServerId,
        registry: Entity<ContextServerDescriptorRegistry>,
        worktree_store: Entity<WorktreeStore>,
        cx: &AsyncApp,
    ) -> Option<Self> {
        const EXTENSION_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

        match settings {
            ContextServerSettings::Stdio {
                enabled: _,
                command,
                remote,
            } => Some(ContextServerConfiguration::Custom { command, remote }),
            ContextServerSettings::Extension {
                enabled: _,
                settings,
                remote,
            } => {
                let descriptor =
                    cx.update(|cx| registry.read(cx).context_server_descriptor(&id.0))?;

                let command_future = descriptor.command(worktree_store, cx);
                let timeout_future = cx.background_executor().timer(EXTENSION_COMMAND_TIMEOUT);

                match futures::future::select(command_future, timeout_future).await {
                    Either::Left((Ok(command), _)) => Some(ContextServerConfiguration::Extension {
                        command,
                        settings,
                        remote,
                    }),
                    Either::Left((Err(e), _)) => {
                        log::error!(
                            "Failed to create context server configuration from settings: {e:#}"
                        );
                        None
                    }
                    Either::Right(_) => {
                        log::error!(
                            "Timed out resolving command for extension context server {id}"
                        );
                        None
                    }
                }
            }
            ContextServerSettings::Http {
                enabled: _,
                url,
                headers: auth,
                timeout,
                oauth,
            } => {
                let url = url::Url::parse(&url).log_err()?;
                Some(ContextServerConfiguration::Http {
                    url,
                    headers: auth,
                    timeout,
                    oauth,
                })
            }
        }
    }
}

pub type ContextServerFactory =
    Box<dyn Fn(ContextServerId, Arc<ContextServerConfiguration>) -> Arc<ContextServer>>;

enum ContextServerStoreState {
    Local {
        downstream_client: Option<(u64, AnyProtoClient)>,
        is_headless: bool,
    },
    Remote {
        project_id: u64,
        upstream_client: Entity<RemoteClient>,
    },
}

pub struct ContextServerStore {
    state: ContextServerStoreState,
    context_server_settings: HashMap<Arc<str>, ContextServerSettings>,
    servers: HashMap<ContextServerId, ContextServerState>,
    server_ids: Vec<ContextServerId>,
    worktree_store: Entity<WorktreeStore>,
    project: Option<WeakEntity<Project>>,
    registry: Entity<ContextServerDescriptorRegistry>,
    update_servers_task: Option<Task<Result<()>>>,
    context_server_factory: Option<ContextServerFactory>,
    needs_server_update: bool,
    ai_disabled: bool,
    _subscriptions: Vec<Subscription>,
}

pub struct ServerStatusChangedEvent {
    pub server_id: ContextServerId,
    pub status: ContextServerStatus,
}

impl EventEmitter<ServerStatusChangedEvent> for ContextServerStore {}

mod construction;
mod creation;
mod lifecycle;
mod maintenance;
mod oauth_flow;
mod start_failure;
