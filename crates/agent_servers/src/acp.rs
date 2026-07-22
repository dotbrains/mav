use acp_thread::{
    AgentConnection, AgentSessionInfo, AgentSessionList, AgentSessionListRequest,
    AgentSessionListResponse,
};
use action_log::ActionLog;
use agent_client_protocol::schema::{
    ProtocolVersion,
    v1::{self as acp, ErrorCode},
};
use agent_client_protocol::{Agent, Client, ConnectionTo, JsonRpcResponse, Lines, Responder};
use anyhow::anyhow;
use async_channel;
use collections::{HashMap, HashSet};
use feature_flags::{AcpBetaFeatureFlag, FeatureFlagAppExt as _};
use futures::channel::mpsc;
use futures::future::Shared;
use futures::io::BufReader;
use futures::{AsyncBufReadExt as _, Future, FutureExt as _, StreamExt as _};
use project::agent_server_store::{
    AgentServerCommand, AgentServerStore, AllAgentServersSettings, CustomAgentServerSettings,
};
use project::{AgentId, Project};
use remote::remote_client::Interactive;
use serde::Deserialize;
use settings::{AgentConfigOptionValue, SettingsStore};
use std::path::PathBuf;
use std::process::Stdio;
use std::rc::Rc;
use std::sync::Arc;
use std::{any::Any, cell::RefCell};
use task::{Shell, ShellBuilder, SpawnInTerminal};
use thiserror::Error;
use util::ResultExt as _;
use util::path_list::PathList;
use util::process::Child;

use anyhow::{Context as _, Result};
use gpui::{App, AppContext as _, AsyncApp, Entity, SharedString, Subscription, Task, WeakEntity};

use acp_thread::{AcpThread, AuthRequired, LoadError, TerminalProviderEvent};
use terminal::TerminalBuilder;
use terminal::terminal_settings::{AlternateScroll, CursorShape};

use crate::{CURSOR_ID, GEMINI_ID};

pub const GEMINI_TERMINAL_AUTH_METHOD_ID: &str = "spawn-gemini-cli";
const PARAMETERIMAV_MODEL_PICKER_META_KEY: &str = "parameterizedModelPicker";
mod agent_connection_impl;
mod client_transport;
mod connection_sessions;
mod connection_stdio;
mod debug_log;
mod defaults;
mod foreground_work;
mod request_handlers;
mod session_helpers;
mod session_list;
mod session_options;
#[path = "acp/terminal_requests.rs"]
mod terminal_requests;

use debug_log::{AcpDebugLog, exited_load_error_with_stderr};
pub use debug_log::{AcpDebugMessage, AcpDebugMessageContent, AcpDebugMessageDirection};
use defaults::AcpConnectionDefaults;
use foreground_work::{ClientContext, ForegroundWork, enqueue_notification, enqueue_request};
use session_helpers::{
    SessionDirectories, emit_load_error_to_all_sessions, meta_terminal_auth_task,
    session_directories_from_work_dirs, terminal_auth_task,
};
use session_list::AcpSessionList;
use session_options::{
    AcpSessionConfigOptions, AcpSessionModes, config_state, mcp_servers_for_project,
};
use terminal_requests::{
    handle_create_terminal, handle_kill_terminal, handle_release_terminal, handle_terminal_output,
    handle_wait_for_terminal_exit,
};

#[derive(Debug, Error)]
#[error("Unsupported version")]
pub struct UnsupportedVersion;

/// Helper for flattening the nested `Result` shapes that come out of
/// `entity.update(cx, |_, cx| fallible_op(cx))` into a single `Result<T,
/// acp::Error>`.
///
/// `anyhow::Error` values get converted via `acp::Error::from`, which
/// downcasts an `acp::Error` back out of `anyhow` when present, so typed
/// errors like auth-required survive the trip.
trait FlattenAcpResult<T> {
    fn flatten_acp(self) -> Result<T, acp::Error>;
}

impl<T> FlattenAcpResult<T> for Result<Result<T, anyhow::Error>, anyhow::Error> {
    fn flatten_acp(self) -> Result<T, acp::Error> {
        match self {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(err.into()),
            Err(err) => Err(err.into()),
        }
    }
}

impl<T> FlattenAcpResult<T> for Result<Result<T, acp::Error>, anyhow::Error> {
    fn flatten_acp(self) -> Result<T, acp::Error> {
        match self {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(err),
            Err(err) => Err(err.into()),
        }
    }
}

pub struct AcpConnection {
    id: AgentId,
    telemetry_id: SharedString,
    agent_version: Option<SharedString>,
    connection: ConnectionTo<Agent>,
    sessions: Rc<RefCell<HashMap<acp::SessionId, AcpSession>>>,
    pending_sessions: Rc<RefCell<HashMap<acp::SessionId, PendingAcpSession>>>,
    auth_methods: Vec<acp::AuthMethod>,
    agent_server_store: WeakEntity<AgentServerStore>,
    agent_capabilities: acp::AgentCapabilities,
    defaults: AcpConnectionDefaults,
    child: Option<Child>,
    session_list: Option<Rc<AcpSessionList>>,
    debug_log: AcpDebugLog,
    _settings_subscription: Subscription,
    _io_task: Task<()>,
    _dispatch_task: Task<()>,
    _wait_task: Task<Result<()>>,
    _stderr_task: Task<Result<()>>,
}

struct PendingAcpSession {
    task: Shared<Task<Result<Entity<AcpThread>, Arc<anyhow::Error>>>>,
    ref_count: usize,
}

struct SessionConfigResponse {
    modes: Option<acp::SessionModeState>,
    config_options: Option<Vec<acp::SessionConfigOption>>,
}

#[derive(Clone)]
struct ConfigOptions {
    config_options: Rc<RefCell<Vec<acp::SessionConfigOption>>>,
    tx: Rc<RefCell<watch::Sender<()>>>,
    rx: watch::Receiver<()>,
}

impl ConfigOptions {
    fn new(config_options: Rc<RefCell<Vec<acp::SessionConfigOption>>>) -> Self {
        let (tx, rx) = watch::channel(());
        Self {
            config_options,
            tx: Rc::new(RefCell::new(tx)),
            rx,
        }
    }
}

pub struct AcpSession {
    thread: WeakEntity<AcpThread>,
    suppress_abort_err: bool,
    session_modes: Option<Rc<RefCell<acp::SessionModeState>>>,
    config_options: Option<ConfigOptions>,
    ref_count: usize,
}
pub async fn connect(
    agent_id: AgentId,
    project: Entity<Project>,
    command: AgentServerCommand,
    agent_server_store: WeakEntity<AgentServerStore>,
    default_mode: Option<acp::SessionModeId>,
    default_config_options: HashMap<String, AgentConfigOptionValue>,
    cx: &mut AsyncApp,
) -> Result<Rc<dyn AgentConnection>> {
    let conn = AcpConnection::stdio(
        agent_id,
        project,
        command.clone(),
        agent_server_store,
        default_mode,
        default_config_options,
        cx,
    )
    .await?;
    Ok(Rc::new(conn) as _)
}

const MINIMUM_SUPPORTED_VERSION: ProtocolVersion = ProtocolVersion::V1;
fn map_acp_error(err: acp::Error) -> anyhow::Error {
    if err.code == acp::ErrorCode::AuthRequired {
        let mut error = AuthRequired::new();

        if err.message != acp::ErrorCode::AuthRequired.to_string() {
            error = error.with_description(err.message);
        }

        anyhow!(error)
    } else {
        anyhow!(err)
    }
}
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;
