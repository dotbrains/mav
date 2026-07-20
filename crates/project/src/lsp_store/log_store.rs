mod project_events;
pub mod state;

use std::{collections::VecDeque, sync::Arc};

use collections::HashMap;
use futures::{StreamExt, channel::mpsc};
use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, Global, Subscription, TaskExt, WeakEntity,
};
use lsp::{IoKind, LanguageServer, LanguageServerId, LanguageServerName, LanguageServerSelector};
use lsp::{MessageType, TraceValue};
use rpc::proto;
use settings::WorktreeId;

use crate::{LanguageServerLogType, Project};

pub use state::{
    LanguageServerKind, LanguageServerRpcState, LanguageServerState, LogKind, LogMessage, Message,
    MessageKind, RpcMessage, TraceMessage,
};

const SEND_LINE: &str = "\n// Send:";
const RECEIVE_LINE: &str = "\n// Receive:";
pub(super) const MAX_STORED_LOG_ENTRIES: usize = 2000;

pub fn init(on_headless_host: bool, cx: &mut App) -> Entity<LogStore> {
    let log_store = cx.new(|cx| LogStore::new(on_headless_host, cx));
    cx.set_global(GlobalLogStore(log_store.clone()));
    log_store
}

pub struct GlobalLogStore(pub Entity<LogStore>);

impl Global for GlobalLogStore {}

#[derive(Debug)]
pub enum Event {
    NewServerLogEntry {
        id: LanguageServerId,
        kind: LanguageServerLogType,
        text: String,
    },
}

impl EventEmitter<Event> for LogStore {}

pub struct LogStore {
    on_headless_host: bool,
    projects: HashMap<WeakEntity<Project>, ProjectState>,
    pub language_servers: HashMap<LanguageServerId, LanguageServerState>,
    io_tx: mpsc::UnboundedSender<(LanguageServerId, IoKind, String)>,
}

struct ProjectState {
    _subscriptions: [Subscription; 2],
    copilot_log_subscription: Option<lsp::Subscription>,
}

impl LogStore {
    pub fn new(on_headless_host: bool, cx: &mut Context<Self>) -> Self {
        let (io_tx, mut io_rx) = mpsc::unbounded();

        let log_store = Self {
            projects: HashMap::default(),
            language_servers: HashMap::default(),

            on_headless_host,
            io_tx,
        };
        cx.spawn(async move |log_store, cx| {
            while let Some((server_id, io_kind, message)) = io_rx.next().await {
                if let Some(log_store) = log_store.upgrade() {
                    log_store.update(cx, |log_store, cx| {
                        log_store.on_io(server_id, io_kind, &message, cx);
                    });
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);

        log_store
    }

    pub fn get_language_server_state(
        &mut self,
        id: LanguageServerId,
    ) -> Option<&mut LanguageServerState> {
        self.language_servers.get_mut(&id)
    }

    pub fn add_language_server(
        &mut self,
        kind: LanguageServerKind,
        server_id: LanguageServerId,
        name: Option<LanguageServerName>,
        worktree_id: Option<WorktreeId>,
        server: Option<Arc<LanguageServer>>,
        cx: &mut Context<Self>,
    ) -> Option<&mut LanguageServerState> {
        let server_state = self.language_servers.entry(server_id).or_insert_with(|| {
            cx.notify();
            LanguageServerState::new(kind)
        });

        if let Some(name) = name {
            server_state.name = Some(name);
        }
        if let Some(worktree_id) = worktree_id {
            server_state.worktree_id = Some(worktree_id);
        }

        if let Some(server) = server.filter(|_| server_state.io_logs_subscription.is_none()) {
            let io_tx = self.io_tx.clone();
            let server_id = server.server_id();
            server_state.io_logs_subscription = Some(server.on_io(move |io_kind, message| {
                io_tx
                    .unbounded_send((server_id, io_kind, message.to_string()))
                    .ok();
            }));
        }

        Some(server_state)
    }

    pub fn add_language_server_log(
        &mut self,
        id: LanguageServerId,
        typ: MessageType,
        message: &str,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let store_logs = !self.on_headless_host;
        let language_server_state = self.get_language_server_state(id)?;

        let log_lines = &mut language_server_state.log_messages;
        let message = message.trim_end().to_string();
        if !store_logs {
            // Send all messages regardless of the visibility in case of not storing, to notify the receiver anyway
            self.emit_event(
                Event::NewServerLogEntry {
                    id,
                    kind: LanguageServerLogType::Log(typ),
                    text: message,
                },
                cx,
            );
        } else if let Some(new_message) = Self::push_new_message(
            log_lines,
            LogMessage::new(message, typ),
            language_server_state.log_level,
        ) {
            self.emit_event(
                Event::NewServerLogEntry {
                    id,
                    kind: LanguageServerLogType::Log(typ),
                    text: new_message,
                },
                cx,
            );
        }
        Some(())
    }

    fn add_language_server_trace(
        &mut self,
        id: LanguageServerId,
        message: &str,
        verbose_info: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let store_logs = !self.on_headless_host;
        let language_server_state = self.get_language_server_state(id)?;

        let log_lines = &mut language_server_state.trace_messages;
        if !store_logs {
            // Send all messages regardless of the visibility in case of not storing, to notify the receiver anyway
            self.emit_event(
                Event::NewServerLogEntry {
                    id,
                    kind: LanguageServerLogType::Trace { verbose_info },
                    text: message.trim().to_string(),
                },
                cx,
            );
        } else if let Some(new_message) = Self::push_new_message(
            log_lines,
            TraceMessage::new(message.trim().to_string(), false),
            TraceValue::Messages,
        ) {
            if let Some(verbose_message) = verbose_info.as_ref() {
                Self::push_new_message(
                    log_lines,
                    TraceMessage::new(verbose_message.clone(), true),
                    TraceValue::Verbose,
                );
            }
            self.emit_event(
                Event::NewServerLogEntry {
                    id,
                    kind: LanguageServerLogType::Trace { verbose_info },
                    text: new_message,
                },
                cx,
            );
        }
        Some(())
    }

    fn push_new_message<T: Message>(
        log_lines: &mut VecDeque<T>,
        message: T,
        current_severity: <T as Message>::Level,
    ) -> Option<String> {
        while log_lines.len() + 1 >= MAX_STORED_LOG_ENTRIES {
            log_lines.pop_front();
        }
        let visible = message.should_include(current_severity);

        let visible_message = visible.then(|| message.as_ref().to_string());
        log_lines.push_back(message);
        visible_message
    }

    fn add_language_server_rpc(
        &mut self,
        language_server_id: LanguageServerId,
        kind: MessageKind,
        message: &str,
        cx: &mut Context<'_, Self>,
    ) {
        let store_logs = !self.on_headless_host;
        let Some(state) = self
            .get_language_server_state(language_server_id)
            .and_then(|state| state.rpc_state.as_mut())
        else {
            return;
        };

        let received = kind == MessageKind::Receive;
        let rpc_log_lines = &mut state.rpc_messages;
        if state.last_message_kind != Some(kind) {
            while rpc_log_lines.len() + 1 >= MAX_STORED_LOG_ENTRIES {
                rpc_log_lines.pop_front();
            }
            let line_before_message = match kind {
                MessageKind::Send => SEND_LINE,
                MessageKind::Receive => RECEIVE_LINE,
            };
            if store_logs {
                rpc_log_lines.push_back(RpcMessage::new(line_before_message.to_string()));
            }
            // Do not send a synthetic message over the wire, it will be derived from the actual RPC message
            cx.emit(Event::NewServerLogEntry {
                id: language_server_id,
                kind: LanguageServerLogType::Rpc { received },
                text: line_before_message.to_string(),
            });
        }

        while rpc_log_lines.len() + 1 >= MAX_STORED_LOG_ENTRIES {
            rpc_log_lines.pop_front();
        }

        if store_logs {
            rpc_log_lines.push_back(RpcMessage::new(message.trim().to_owned()));
        }

        self.emit_event(
            Event::NewServerLogEntry {
                id: language_server_id,
                kind: LanguageServerLogType::Rpc { received },
                text: message.to_owned(),
            },
            cx,
        );
    }

    pub fn remove_language_server(&mut self, id: LanguageServerId, cx: &mut Context<Self>) {
        self.language_servers.remove(&id);
        cx.notify();
    }

    pub fn server_logs(&self, server_id: LanguageServerId) -> Option<&VecDeque<LogMessage>> {
        Some(&self.language_servers.get(&server_id)?.log_messages)
    }

    pub fn server_trace(&self, server_id: LanguageServerId) -> Option<&VecDeque<TraceMessage>> {
        Some(&self.language_servers.get(&server_id)?.trace_messages)
    }

    pub fn server_ids_for_project<'a>(
        &'a self,
        lookup_project: &'a WeakEntity<Project>,
    ) -> impl Iterator<Item = LanguageServerId> + 'a {
        self.language_servers
            .iter()
            .filter_map(move |(id, state)| match &state.kind {
                LanguageServerKind::Local { project } | LanguageServerKind::Remote { project } => {
                    if project == lookup_project {
                        Some(*id)
                    } else {
                        None
                    }
                }
                LanguageServerKind::Global | LanguageServerKind::LocalSsh { .. } => Some(*id),
            })
    }

    pub fn enable_rpc_trace_for_language_server(
        &mut self,
        server_id: LanguageServerId,
    ) -> Option<&mut LanguageServerRpcState> {
        let rpc_state = self
            .language_servers
            .get_mut(&server_id)?
            .rpc_state
            .get_or_insert_with(LanguageServerRpcState::new);
        Some(rpc_state)
    }

    pub fn disable_rpc_trace_for_language_server(
        &mut self,
        server_id: LanguageServerId,
    ) -> Option<()> {
        self.language_servers.get_mut(&server_id)?.rpc_state.take();
        Some(())
    }

    pub fn has_server_logs(&self, server: &LanguageServerSelector) -> bool {
        match server {
            LanguageServerSelector::Id(id) => self.language_servers.contains_key(id),
            LanguageServerSelector::Name(name) => self
                .language_servers
                .iter()
                .any(|(_, state)| state.name.as_ref() == Some(name)),
        }
    }

    fn on_io(
        &mut self,
        language_server_id: LanguageServerId,
        io_kind: IoKind,
        message: &str,
        cx: &mut Context<Self>,
    ) -> Option<()> {
        let is_received = match io_kind {
            IoKind::StdOut => true,
            IoKind::StdIn => false,
            IoKind::StdErr => {
                self.add_language_server_log(language_server_id, MessageType::LOG, message, cx);
                return Some(());
            }
        };

        let kind = if is_received {
            MessageKind::Receive
        } else {
            MessageKind::Send
        };

        self.add_language_server_rpc(language_server_id, kind, message, cx);
        cx.notify();
        Some(())
    }

    fn emit_event(&mut self, e: Event, cx: &mut Context<Self>) {
        match &e {
            Event::NewServerLogEntry { id, kind, text } => {
                if let Some(state) = self.get_language_server_state(*id) {
                    let downstream_client = match &state.kind {
                        LanguageServerKind::Remote { project }
                        | LanguageServerKind::Local { project } => project
                            .upgrade()
                            .map(|project| project.read(cx).lsp_store()),
                        LanguageServerKind::LocalSsh { lsp_store } => lsp_store.upgrade(),
                        LanguageServerKind::Global => None,
                    }
                    .and_then(|lsp_store| lsp_store.read(cx).downstream_client());
                    if let Some((client, project_id)) = downstream_client {
                        if Some(LogKind::from_server_log_type(kind)) == state.toggled_log_kind {
                            client
                                .send(proto::LanguageServerLog {
                                    project_id,
                                    language_server_id: id.to_proto(),
                                    message: text.clone(),
                                    log_type: Some(kind.to_proto()),
                                })
                                .ok();
                        }
                    }
                }
            }
        }

        cx.emit(e);
    }

    pub fn toggle_lsp_logs(
        &mut self,
        server_id: LanguageServerId,
        enabled: bool,
        toggled_log_kind: LogKind,
    ) {
        if let Some(server_state) = self.get_language_server_state(server_id) {
            if enabled {
                server_state.toggled_log_kind = Some(toggled_log_kind);
            } else {
                server_state.toggled_log_kind = None;
            }
        }
        if LogKind::Rpc == toggled_log_kind {
            if enabled {
                self.enable_rpc_trace_for_language_server(server_id);
            } else {
                self.disable_rpc_trace_for_language_server(server_id);
            }
        }
    }
    pub fn copilot_state_for_project(
        &mut self,
        project: &WeakEntity<Project>,
    ) -> Option<&mut Option<lsp::Subscription>> {
        self.projects
            .get_mut(project)
            .map(|project| &mut project.copilot_log_subscription)
    }
}
