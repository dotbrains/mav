use super::breakpoint_store::{
    BreakpointStore, BreakpointStoreEvent, BreakpointUpdatedReason, SourceBreakpoint,
};
use super::dap_command::{
    self, Attach, ConfigurationDone, ContinueCommand, DataBreakpointInfoCommand, DisconnectCommand,
    EvaluateCommand, Initialize, Launch, LoadedSourcesCommand, LocalDapCommand, LocationsCommand,
    ModulesCommand, NextCommand, PauseCommand, RestartCommand, RestartStackFrameCommand,
    ScopesCommand, SetDataBreakpointsCommand, SetExceptionBreakpoints, SetVariableValueCommand,
    StackTraceCommand, StepBackCommand, StepCommand, StepInCommand, StepOutCommand,
    TerminateCommand, TerminateThreadsCommand, ThreadsCommand, VariablesCommand,
};
use super::dap_store::DapStore;
use crate::debugger::breakpoint_store::BreakpointSessionState;
use crate::debugger::dap_command::{DataBreakpointContext, ReadMemory};
use crate::debugger::memory::{self, Memory, MemoryIterator, MemoryPageBuilder, PageAddress};
use anyhow::{Context as _, Result, anyhow, bail};
use base64::Engine;
use collections::{HashMap, HashSet, IndexMap, TypeIdHashMap};
use dap::adapters::{DebugAdapterBinary, DebugAdapterName};
use dap::messages::Response;
use dap::requests::{Request, RunInTerminal, StartDebugging};
use dap::transport::TcpTransport;
use dap::{
    Capabilities, ContinueArguments, EvaluateArgumentsContext, Module, Source, StackFrameId,
    SteppingGranularity, StoppedEvent, VariableReference,
    client::{DebugAdapterClient, SessionId},
    messages::{Events, Message},
};
use dap::{
    ExceptionBreakpointsFilter, ExceptionFilterOptions, OutputEvent, OutputEventCategory,
    RunInTerminalRequestArguments, StackFramePresentationHint, StartDebuggingRequestArguments,
    StartDebuggingRequestArgumentsRequest, VariablePresentationHint, WriteMemoryArguments,
};
use futures::channel::mpsc::UnboundedSender;
use futures::channel::{mpsc, oneshot};
use futures::io::BufReader;
use futures::{AsyncBufReadExt as _, SinkExt, StreamExt, TryStreamExt};
use futures::{FutureExt, future::Shared};
use gpui::{
    App, AppContext, AsyncApp, BackgroundExecutor, Context, Entity, EventEmitter, SharedString,
    Task, TaskExt, WeakEntity,
};
use http_client::HttpClient;
use node_runtime::NodeRuntime;
use remote::RemoteClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol::net::{TcpListener, TcpStream};
use std::any::TypeId;
use std::collections::{BTreeMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr};
use std::ops::RangeInclusive;
use std::path::PathBuf;
use std::time::Duration;
use std::u64;
use std::{
    any::Any,
    collections::hash_map::Entry,
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};
use task::SharedTaskContext;
use text::{PointUtf16, ToPointUtf16};
use url::Url;
use util::command::Stdio;
use util::command::new_command;
use util::{ResultExt, debug_panic, maybe};
use worktree::Worktree;

mod breakpoints;
mod companion;
mod control;
mod dap_events;
mod history_output;
mod lifecycle;
mod misc;
mod request_cache;
mod request_types;
mod running_mode;
mod stepping_stack;
mod terminal_requests;
mod threads_modules_memory;
mod types;
mod watchers_eval;
use request_types::*;
use types::*;
const MAX_TRACKED_OUTPUT_EVENTS: usize = 5000;
const DEBUG_HISTORY_LIMIT: usize = 10;

/// Represents a current state of a single debug adapter and provides ways to mutate it.
pub struct Session {
    pub state: SessionState,
    active_snapshot: SessionSnapshot,
    snapshots: VecDeque<SessionSnapshot>,
    selected_snapshot_index: Option<usize>,
    id: SessionId,
    label: Option<SharedString>,
    adapter: DebugAdapterName,
    pub(super) capabilities: Capabilities,
    child_session_ids: HashSet<SessionId>,
    parent_session: Option<Entity<Session>>,
    output_token: OutputToken,
    output: Box<circular_buffer::CircularBuffer<MAX_TRACKED_OUTPUT_EVENTS, dap::OutputEvent>>,
    watchers: HashMap<SharedString, Watcher>,
    is_session_terminated: bool,
    requests: TypeIdHashMap<HashMap<RequestSlot, Shared<Task<Option<()>>>>>,
    pub(crate) breakpoint_store: Entity<BreakpointStore>,
    ignore_breakpoints: bool,
    exception_breakpoints: BTreeMap<String, (ExceptionBreakpointsFilter, IsEnabled)>,
    data_breakpoints: BTreeMap<String, DataBreakpointState>,
    background_tasks: Vec<Task<()>>,
    restart_task: Option<Task<()>>,
    task_context: SharedTaskContext,
    memory: memory::Memory,
    quirks: SessionQuirks,
    remote_client: Option<Entity<RemoteClient>>,
    node_runtime: Option<NodeRuntime>,
    http_client: Option<Arc<dyn HttpClient>>,
    companion_port: Option<u16>,
}

// local session will send breakpoint updates to DAP for all new breakpoints
// remote side will only send breakpoint updates when it is a breakpoint created by that peer
// BreakpointStore notifies session on breakpoint changes
