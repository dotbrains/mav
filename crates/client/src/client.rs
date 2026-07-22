#[cfg(any(test, feature = "test-support"))]
pub mod test;

mod llm_token;
pub mod mav_urls;
mod proxy;
pub mod telemetry;
pub mod user;

#[path = "client_impl/auth.rs"]
mod auth_impl;
#[path = "client_impl/browser_auth.rs"]
mod browser_auth;
#[path = "client_impl/settings.rs"]
mod client_settings;
#[path = "client_impl/connection.rs"]
mod connection;
#[path = "client_impl/core.rs"]
mod core;
#[path = "client_impl/credentials.rs"]
mod credentials;
#[path = "client_impl/handlers.rs"]
mod handlers;
#[path = "client_impl/links.rs"]
mod links;
#[path = "client_impl/llm.rs"]
mod llm;
#[path = "client_impl/proto_client.rs"]
mod proto_client;
#[path = "client_impl/rpc_transport.rs"]
mod rpc_transport;
#[path = "client_impl/subscriptions.rs"]
mod subscriptions;
#[cfg(test)]
#[path = "client_impl/tests.rs"]
mod tests;

use anyhow::{Context as _, Result, anyhow};
use async_tungstenite::tungstenite::{
    client::IntoClientRequest,
    error::Error as WebsocketError,
    http::{HeaderValue, Request, StatusCode},
};
use clock::SystemClock;
use cloud_api_client::LlmApiToken;
use cloud_api_client::websocket_protocol::MessageToClient;
use cloud_api_client::{ClientApiError, CloudApiClient};
use cloud_api_types::OrganizationId;
use credentials_provider::CredentialsProvider;
use feature_flags::FeatureFlagAppExt as _;
use futures::{
    AsyncReadExt, FutureExt, SinkExt, Stream, StreamExt, TryFutureExt as _, TryStreamExt,
    channel::{mpsc, oneshot},
    future::BoxFuture,
    stream::BoxStream,
};
use gpui::{App, AsyncApp, Entity, Global, Task, TaskExt, WeakEntity, actions};
use http_client::{HttpClient, HttpClientWithUrl, http, read_proxy_from_env};
use parking_lot::{Mutex, RwLock};
use postage::watch;
use proxy::connect_proxy_stream;
use rand::prelude::*;
use release_channel::{AppVersion, ReleaseChannel};
use rpc::proto::{AnyTypedEnvelope, EnvelopedMessage, PeerId, RequestMessage};
use serde::{Deserialize, Serialize};
use settings::{RegisterSetting, Settings, SettingsContent};
use std::{
    any::TypeId,
    convert::TryFrom,
    future::Future,
    marker::PhantomData,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use std::{cmp, pin::Pin};
use telemetry::Telemetry;
use thiserror::Error;
use tokio::net::TcpStream;
use url::Url;
use util::{ConnectionResult, ResultExt};

pub use client_settings::*;
pub use credentials::*;
pub use links::*;
pub use llm_token::*;
pub use rpc::*;
pub use subscriptions::*;
pub use telemetry_events::Event;
pub use user::*;

static MAV_SERVER_URL: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("MAV_SERVER_URL").ok());
static MAV_RPC_URL: LazyLock<Option<String>> = LazyLock::new(|| std::env::var("MAV_RPC_URL").ok());

pub static IMPERSONATE_LOGIN: LazyLock<Option<String>> = LazyLock::new(|| {
    std::env::var("MAV_IMPERSONATE")
        .ok()
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
});

pub static USE_WEB_LOGIN: LazyLock<bool> = LazyLock::new(|| std::env::var("MAV_WEB_LOGIN").is_ok());

pub static ADMIN_API_TOKEN: LazyLock<Option<String>> = LazyLock::new(|| {
    std::env::var("MAV_ADMIN_API_TOKEN")
        .ok()
        .and_then(|s| if s.is_empty() { None } else { Some(s) })
});

pub static MAV_APP_PATH: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var("MAV_APP_PATH").ok().map(PathBuf::from));

pub static MAV_ALWAYS_ACTIVE: LazyLock<bool> =
    LazyLock::new(|| std::env::var("MAV_ALWAYS_ACTIVE").is_ok_and(|e| !e.is_empty()));

pub const INITIAL_RECONNECTION_DELAY: Duration = Duration::from_millis(500);
pub const MAX_RECONNECTION_DELAY: Duration = Duration::from_secs(30);
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(20);

actions!(
    client,
    [
        /// Signs in to Mav account.
        SignIn,
        /// Signs out of Mav account.
        SignOut,
        /// Reconnects to the collaboration server.
        Reconnect
    ]
);

pub type MessageToClientHandler = Box<dyn Fn(&MessageToClient, &mut App) + Send + Sync + 'static>;

struct GlobalClient(Arc<Client>);

impl Global for GlobalClient {}

pub struct Client {
    id: AtomicU64,
    peer: Arc<Peer>,
    http: Arc<HttpClientWithUrl>,
    cloud_client: Arc<CloudApiClient>,
    telemetry: Arc<Telemetry>,
    credentials_provider: ClientCredentialsProvider,
    state: RwLock<ClientState>,
    handler_set: Mutex<ProtoMessageHandlerSet>,
    message_to_client_handlers: Mutex<Vec<MessageToClientHandler>>,
    sign_out_tx: Mutex<Option<mpsc::UnboundedSender<()>>>,

    #[allow(clippy::type_complexity)]
    #[cfg(any(test, feature = "test-support"))]
    authenticate:
        RwLock<Option<Box<dyn 'static + Send + Sync + Fn(&AsyncApp) -> Task<Result<Credentials>>>>>,

    #[allow(clippy::type_complexity)]
    #[cfg(any(test, feature = "test-support"))]
    establish_connection: RwLock<
        Option<
            Box<
                dyn 'static
                    + Send
                    + Sync
                    + Fn(
                        &Credentials,
                        &AsyncApp,
                    ) -> Task<Result<Connection, EstablishConnectionError>>,
            >,
        >,
    >,

    #[cfg(any(test, feature = "test-support"))]
    rpc_url: RwLock<Option<Url>>,
}

#[derive(Error, Debug)]
pub enum EstablishConnectionError {
    #[error("upgrade required")]
    UpgradeRequired,
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Other(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidHeaderValue(#[from] async_tungstenite::tungstenite::http::header::InvalidHeaderValue),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Websocket(#[from] async_tungstenite::tungstenite::http::Error),
}

impl From<WebsocketError> for EstablishConnectionError {
    fn from(error: WebsocketError) -> Self {
        if let WebsocketError::Http(response) = &error {
            match response.status() {
                StatusCode::UNAUTHORIZED => return EstablishConnectionError::Unauthorized,
                StatusCode::UPGRADE_REQUIRED => return EstablishConnectionError::UpgradeRequired,
                _ => {}
            }
        }
        EstablishConnectionError::Other(error.into())
    }
}

impl EstablishConnectionError {
    pub fn other(error: impl Into<anyhow::Error> + Send + Sync) -> Self {
        Self::Other(error.into())
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Status {
    SignedOut,
    UpgradeRequired,
    Authenticating,
    Authenticated,
    AuthenticationError,
    Connecting,
    ConnectionError,
    Connected {
        peer_id: PeerId,
        connection_id: ConnectionId,
    },
    ConnectionLost,
    Reauthenticating,
    Reauthenticated,
    Reconnecting,
    ReconnectionError {
        next_reconnection: Instant,
    },
}

impl Status {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    pub fn was_connected(&self) -> bool {
        matches!(
            self,
            Self::ConnectionLost
                | Self::Reauthenticating
                | Self::Reauthenticated
                | Self::Reconnecting
        )
    }

    /// Returns whether the client is currently connected or was connected at some point.
    pub fn is_or_was_connected(&self) -> bool {
        self.is_connected() || self.was_connected()
    }

    pub fn is_signing_in(&self) -> bool {
        matches!(
            self,
            Self::Authenticating | Self::Reauthenticating | Self::Connecting | Self::Reconnecting
        )
    }

    pub fn is_signed_out(&self) -> bool {
        matches!(self, Self::SignedOut | Self::UpgradeRequired)
    }
}

struct ClientState {
    credentials: Option<Credentials>,
    status: (watch::Sender<Status>, watch::Receiver<Status>),
    /// Bumped each time the cloud websocket finishes its handshake. Starts at `0` so
    /// subscribers can distinguish "no connection yet" from a real reconnect.
    cloud_connection_id: (watch::Sender<u64>, watch::Receiver<u64>),
    _reconnect_task: Option<Task<()>>,
    _cloud_connection_task: Option<Task<()>>,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            credentials: None,
            status: watch::channel_with(Status::SignedOut),
            cloud_connection_id: watch::channel_with(0),
            _reconnect_task: None,
            _cloud_connection_task: None,
        }
    }
}
