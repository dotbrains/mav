#[cfg(any(test, feature = "test-support"))]
use crate::transport::mock::ConnectGuard;
use crate::{
    SshConnectionOptions,
    protocol::MessageId,
    proxy::ProxyLaunchError,
    transport::{
        docker::{DockerConnectionOptions, DockerExecConnection},
        ssh::SshRemoteConnection,
        wsl::{WslConnectionOptions, WslRemoteConnection},
    },
};
use anyhow::{Context as _, Result, anyhow};
use askpass::EncryptedPassword;
use async_trait::async_trait;
use collections::HashMap;
use futures::{
    Future, FutureExt as _, StreamExt as _,
    channel::{
        mpsc::{self, Sender, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    future::{BoxFuture, Shared, WeakShared},
    select, select_biased,
    stream::BoxStream,
};
use gpui::{
    App, AppContext as _, AsyncApp, BackgroundExecutor, BorrowAppContext, Context, Entity,
    EventEmitter, FutureExt, Global, Task, TaskExt, WeakEntity,
};
use parking_lot::Mutex;

use release_channel::ReleaseChannel;
use rpc::{
    AnyProtoClient, ErrorExt, ProtoClient, ProtoMessageHandlerSet, RpcError,
    proto::{self, Envelope, EnvelopedMessage, PeerId, RequestMessage, build_typed_envelope},
};
use semver::Version;
use std::{
    collections::VecDeque,
    fmt,
    ops::ControlFlow,
    path::PathBuf,
    sync::{
        Arc, Weak,
        atomic::{AtomicU32, AtomicU64, Ordering::SeqCst},
    },
    time::{Duration, Instant},
};
use util::{
    ResultExt,
    paths::{PathStyle, RemotePathBuf},
};

mod channel;
mod client_accessors;
mod client_heartbeat;
mod client_lifecycle;
mod client_reconnect;
mod client_test_support;
mod connection;
mod conversions;
mod options;
mod pool;
#[cfg(test)]
mod tests;

use self::{
    channel::ChannelClient,
    pool::{ConnectionPool, ConnectionPoolEntry},
};
pub use self::{connection::RemoteConnection, options::RemoteConnectionOptions};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RemoteOs {
    Linux,
    MacOs,
    Windows,
}

impl RemoteOs {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteOs::Linux => "linux",
            RemoteOs::MacOs => "macos",
            RemoteOs::Windows => "windows",
        }
    }

    pub fn is_windows(&self) -> bool {
        matches!(self, RemoteOs::Windows)
    }

    /// A human-readable OS name for telemetry. Matches `client::telemetry::os_name`
    /// ignoring the compositor (as we run headless on remotes).
    pub fn display_name(&self) -> &'static str {
        match self {
            RemoteOs::Linux => "Linux",
            RemoteOs::MacOs => "macOS",
            RemoteOs::Windows => "Windows",
        }
    }
}

impl std::fmt::Display for RemoteOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RemoteArch {
    X86_64,
    Aarch64,
}

impl RemoteArch {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteArch::X86_64 => "x86_64",
            RemoteArch::Aarch64 => "aarch64",
        }
    }
}

impl std::fmt::Display for RemoteArch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct RemotePlatform {
    pub os: RemoteOs,
    pub arch: RemoteArch,
}

#[derive(Clone, Debug)]
pub struct CommandTemplate {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
}

/// Whether a command should be run with TTY allocation for interactive use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interactive {
    /// Allocate a pseudo-TTY for interactive terminal use.
    Yes,
    /// Do not allocate a TTY - for commands that communicate via piped stdio.
    No,
}

pub trait RemoteClientDelegate: Send + Sync {
    fn ask_password(
        &self,
        prompt: String,
        tx: oneshot::Sender<EncryptedPassword>,
        cx: &mut AsyncApp,
    );
    fn get_download_url(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Option<String>>>;
    fn download_server_binary_locally(
        &self,
        platform: RemotePlatform,
        release_channel: ReleaseChannel,
        version: Option<Version>,
        cx: &mut AsyncApp,
    ) -> Task<Result<PathBuf>>;
    fn set_status(&self, status: Option<&str>, cx: &mut AsyncApp);
}

const MAX_MISSED_HEARTBEATS: usize = 5;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(5);
const INITIAL_CONNECTION_TIMEOUT: Duration =
    Duration::from_secs(if cfg!(debug_assertions) { 5 } else { 60 });

pub const MAX_RECONNECT_ATTEMPTS: usize = 3;

enum State {
    Connecting,
    Connected {
        remote_connection: Arc<dyn RemoteConnection>,
        delegate: Arc<dyn RemoteClientDelegate>,

        multiplex_task: Task<Result<()>>,
        heartbeat_task: Task<Result<()>>,
    },
    HeartbeatMissed {
        missed_heartbeats: usize,

        remote_connection: Arc<dyn RemoteConnection>,
        delegate: Arc<dyn RemoteClientDelegate>,

        multiplex_task: Task<Result<()>>,
        heartbeat_task: Task<Result<()>>,
    },
    Reconnecting,
    ReconnectFailed {
        remote_connection: Arc<dyn RemoteConnection>,
        delegate: Arc<dyn RemoteClientDelegate>,

        error: anyhow::Error,
        attempts: usize,
    },
    ReconnectExhausted,
    ServerNotRunning,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connecting => write!(f, "connecting"),
            Self::Connected { .. } => write!(f, "connected"),
            Self::Reconnecting => write!(f, "reconnecting"),
            Self::ReconnectFailed { .. } => write!(f, "reconnect failed"),
            Self::ReconnectExhausted => write!(f, "reconnect exhausted"),
            Self::HeartbeatMissed { .. } => write!(f, "heartbeat missed"),
            Self::ServerNotRunning { .. } => write!(f, "server not running"),
        }
    }
}

impl State {
    fn remote_connection(&self) -> Option<Arc<dyn RemoteConnection>> {
        match self {
            Self::Connected {
                remote_connection, ..
            } => Some(remote_connection.clone()),
            Self::HeartbeatMissed {
                remote_connection, ..
            } => Some(remote_connection.clone()),
            Self::ReconnectFailed {
                remote_connection, ..
            } => Some(remote_connection.clone()),
            _ => None,
        }
    }

    fn can_reconnect(&self) -> bool {
        match self {
            Self::Connected { .. }
            | Self::HeartbeatMissed { .. }
            | Self::ReconnectFailed { .. } => true,
            State::Connecting
            | State::Reconnecting
            | State::ReconnectExhausted
            | State::ServerNotRunning => false,
        }
    }

    fn is_reconnect_failed(&self) -> bool {
        matches!(self, Self::ReconnectFailed { .. })
    }

    fn is_reconnect_exhausted(&self) -> bool {
        matches!(self, Self::ReconnectExhausted { .. })
    }

    fn is_server_not_running(&self) -> bool {
        matches!(self, Self::ServerNotRunning)
    }

    fn is_reconnecting(&self) -> bool {
        matches!(self, Self::Reconnecting { .. })
    }

    fn heartbeat_recovered(self) -> Self {
        match self {
            Self::HeartbeatMissed {
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
                ..
            } => Self::Connected {
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
            },
            _ => self,
        }
    }

    fn heartbeat_missed(self) -> Self {
        match self {
            Self::Connected {
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
            } => Self::HeartbeatMissed {
                missed_heartbeats: 1,
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
            },
            Self::HeartbeatMissed {
                missed_heartbeats,
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
            } => Self::HeartbeatMissed {
                missed_heartbeats: missed_heartbeats + 1,
                remote_connection,
                delegate,
                multiplex_task,
                heartbeat_task,
            },
            _ => self,
        }
    }
}

/// The state of the ssh connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    HeartbeatMissed,
    Reconnecting,
    Disconnected,
}

impl From<&State> for ConnectionState {
    fn from(value: &State) -> Self {
        match value {
            State::Connecting => Self::Connecting,
            State::Connected { .. } => Self::Connected,
            State::Reconnecting | State::ReconnectFailed { .. } => Self::Reconnecting,
            State::HeartbeatMissed { .. } => Self::HeartbeatMissed,
            State::ReconnectExhausted => Self::Disconnected,
            State::ServerNotRunning => Self::Disconnected,
        }
    }
}

pub struct RemoteClient {
    client: Arc<ChannelClient>,
    unique_identifier: String,
    connection_options: RemoteConnectionOptions,
    path_style: PathStyle,
    platform: RemotePlatform,
    os_version: Option<String>,
    state: Option<State>,
}

#[derive(Debug)]
pub enum RemoteClientEvent {
    Disconnected { server_not_running: bool },
}

impl EventEmitter<RemoteClientEvent> for RemoteClient {}

/// Identifies the socket on the remote server so that reconnects
/// can re-join the same project.
pub enum ConnectionIdentifier {
    Setup(u64),
    Workspace(i64),
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl ConnectionIdentifier {
    pub fn setup() -> Self {
        Self::Setup(NEXT_ID.fetch_add(1, SeqCst))
    }

    // This string gets used in a socket name, and so must be relatively short.
    // The total length of:
    //   /home/{username}/.local/share/mav/server_state/{name}/stdout.sock
    // Must be less than about 100 characters
    //   https://unix.stackexchange.com/questions/367008/why-is-socket-path-length-limited-to-a-hundred-chars
    // So our strings should be at most 20 characters or so.
    fn to_string(&self, cx: &App) -> String {
        let identifier_prefix = match ReleaseChannel::global(cx) {
            ReleaseChannel::Stable => "".to_string(),
            release_channel => format!("{}-", release_channel.dev_name()),
        };
        match self {
            Self::Setup(setup_id) => format!("{identifier_prefix}setup-{setup_id}"),
            Self::Workspace(workspace_id) => {
                format!("{identifier_prefix}workspace-{workspace_id}",)
            }
        }
    }
}

pub async fn connect(
    connection_options: RemoteConnectionOptions,
    delegate: Arc<dyn RemoteClientDelegate>,
    cx: &mut AsyncApp,
) -> Result<Arc<dyn RemoteConnection>> {
    cx.update(|cx| {
        cx.update_default_global(|pool: &mut ConnectionPool, cx| {
            pool.connect(connection_options.clone(), delegate.clone(), cx)
        })
    })
    .await
    .map_err(|e| e.cloned())
}

/// Returns `true` if the global [`ConnectionPool`] already has a live
/// connection for the given options. Callers can use this to decide
/// whether to show interactive UI (e.g., a password modal) before
/// connecting.
pub fn has_active_connection(opts: &RemoteConnectionOptions, cx: &App) -> bool {
    cx.try_global::<ConnectionPool>().is_some_and(|pool| {
        matches!(
            pool.connections.get(opts),
            Some(ConnectionPoolEntry::Connected(remote))
                if remote.upgrade().is_some_and(|r| !r.has_been_killed())
        )
    })
}
