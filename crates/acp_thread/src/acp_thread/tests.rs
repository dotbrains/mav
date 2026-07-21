use super::*;
use anyhow::anyhow;
use futures::stream::StreamExt as _;
use futures::{channel::mpsc, future::LocalBoxFuture, select};
use gpui::{App, AsyncApp, TestAppContext, WeakEntity};
use indoc::indoc;
use project::{AgentId, FakeFs, Fs};
use rand::{distr, prelude::*};
use serde_json::json;
use settings::SettingsStore;
use std::{
    any::Any,
    cell::RefCell,
    path::Path,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst},
    time::Duration,
};
use util::{path, path_list::PathList};

#[path = "tests/checkpoint_tests.rs"]
mod checkpoint_tests;
#[path = "tests/content_block_tests.rs"]
mod content_block_tests;
#[path = "tests/echo_tests.rs"]
mod echo_tests;
#[path = "tests/file_tool_tests.rs"]
mod file_tool_tests;
#[path = "tests/message_chunk_tests.rs"]
mod message_chunk_tests;
#[path = "tests/permission_tests.rs"]
mod permission_tests;
#[path = "tests/refusal_tests.rs"]
mod refusal_tests;
#[path = "tests/restore_checkpoint_tests.rs"]
mod restore_checkpoint_tests;
#[path = "tests/terminal_provider_tests.rs"]
mod terminal_provider_tests;
#[path = "tests/title_usage_tests.rs"]
mod title_usage_tests;
#[path = "tests/tool_call_basic_tests.rs"]
mod tool_call_basic_tests;
#[path = "tests/turn_lifecycle_tests.rs"]
mod turn_lifecycle_tests;

#[test]
fn command_category_meta_round_trips() {
    // Exhaustive list of variants. The match below has no wildcard arm, so
    // adding a `CommandCategory` variant fails to compile here until it's
    // covered, keeping the `as_str`/`from_str` wire contract in sync.
    let all = [CommandCategory::Native, CommandCategory::Mcp];
    for category in all {
        match category {
            CommandCategory::Native | CommandCategory::Mcp => {}
        }
        let meta = meta_with_command_category(category);
        assert_eq!(command_category_from_meta(&Some(meta)), Some(category));
    }

    // Absent meta and unknown categories both decode to `None`.
    assert_eq!(command_category_from_meta(&None), None);
    let unknown =
        acp::Meta::from_iter([(COMMAND_CATEGORY_META_KEY.into(), "future-category".into())]);
    assert_eq!(command_category_from_meta(&Some(unknown)), None);
}

#[test]
fn client_user_message_id_serializes_as_string() {
    let serialized =
        serde_json::to_value(ClientUserMessageId::new()).expect("serialize client message id");
    assert!(
        serialized.is_string(),
        "expected string, got {serialized:?}"
    );

    let deserialized: ClientUserMessageId =
        serde_json::from_value(json!("client-id")).expect("deserialize client message id");
    assert_eq!(
        serde_json::to_value(deserialized).expect("serialize client message id"),
        json!("client-id")
    );
}

fn init_test(cx: &mut TestAppContext) {
    env_logger::try_init().ok();
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

#[test]
fn sandbox_authorization_details_deserialize_legacy_network_bool() {
    // Older builds persisted `network: bool`; the `alias` on
    // `network_all_hosts` must keep those details rendering as a
    // network request rather than silently dropping it.
    let details: SandboxAuthorizationDetails =
        serde_json::from_value(json!({ "network": true })).unwrap();
    assert!(details.network_all_hosts);
    assert!(details.network_hosts.is_empty());

    let details: SandboxAuthorizationDetails =
        serde_json::from_value(json!({ "network": false })).unwrap();
    assert!(!details.network_all_hosts);
}

async fn run_until_first_tool_call(thread: &Entity<AcpThread>, cx: &mut TestAppContext) -> usize {
    let (mut tx, mut rx) = mpsc::channel::<usize>(1);

    let subscription = cx.update(|cx| {
        cx.subscribe(thread, move |thread, _, cx| {
            for (ix, entry) in thread.read(cx).entries.iter().enumerate() {
                if matches!(entry, AgentThreadEntry::ToolCall(_)) {
                    return tx.try_send(ix).unwrap();
                }
            }
        })
    });

    select! {
        _ = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_secs(10))) => {
            panic!("Timeout waiting for tool call")
        }
        ix = rx.next().fuse() => {
            drop(subscription);
            ix.unwrap()
        }
    }
}

#[derive(Clone, Default)]
struct FakeAgentConnection {
    auth_methods: Vec<acp::AuthMethod>,
    supports_truncate: bool,
    sessions: Arc<parking_lot::Mutex<HashMap<acp::SessionId, WeakEntity<AcpThread>>>>,
    set_title_calls: Rc<RefCell<Vec<SharedString>>>,
    on_user_message: Option<
        Rc<
            dyn Fn(
                    acp::PromptRequest,
                    WeakEntity<AcpThread>,
                    AsyncApp,
                ) -> LocalBoxFuture<'static, Result<acp::PromptResponse>>
                + 'static,
        >,
    >,
}

impl FakeAgentConnection {
    fn new() -> Self {
        Self {
            auth_methods: Vec::new(),
            supports_truncate: true,
            on_user_message: None,
            sessions: Arc::default(),
            set_title_calls: Default::default(),
        }
    }

    fn without_truncate_support(mut self) -> Self {
        self.supports_truncate = false;
        self
    }

    #[expect(unused)]
    fn with_auth_methods(mut self, auth_methods: Vec<acp::AuthMethod>) -> Self {
        self.auth_methods = auth_methods;
        self
    }

    fn on_user_message(
        mut self,
        handler: impl Fn(
            acp::PromptRequest,
            WeakEntity<AcpThread>,
            AsyncApp,
        ) -> LocalBoxFuture<'static, Result<acp::PromptResponse>>
        + 'static,
    ) -> Self {
        self.on_user_message.replace(Rc::new(handler));
        self
    }
}

impl AgentConnection for FakeAgentConnection {
    fn agent_id(&self) -> AgentId {
        AgentId::new("fake")
    }

    fn telemetry_id(&self) -> SharedString {
        "fake".into()
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &self.auth_methods
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        work_dirs: PathList,
        cx: &mut App,
    ) -> Task<gpui::Result<Entity<AcpThread>>> {
        let session_id = acp::SessionId::new(
            rand::rng()
                .sample_iter(&distr::Alphanumeric)
                .take(7)
                .map(char::from)
                .collect::<String>(),
        );
        let action_log = cx.new(|_| ActionLog::new(project.clone()));
        let thread = cx.new(|cx| {
            AcpThread::new(
                None,
                None,
                Some(work_dirs),
                self.clone(),
                project,
                action_log,
                session_id.clone(),
                watch::Receiver::constant(
                    acp::PromptCapabilities::new()
                        .image(true)
                        .audio(true)
                        .embedded_context(true),
                ),
                cx,
            )
        });
        self.sessions.lock().insert(session_id, thread.downgrade());
        Task::ready(Ok(thread))
    }

    fn authenticate(&self, method: acp::AuthMethodId, _cx: &mut App) -> Task<gpui::Result<()>> {
        if self.auth_methods().iter().any(|m| m.id() == &method) {
            Task::ready(Ok(()))
        } else {
            Task::ready(Err(anyhow!("Invalid Auth Method")))
        }
    }

    fn prompt(
        &self,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<gpui::Result<acp::PromptResponse>> {
        let sessions = self.sessions.lock();
        let thread = sessions.get(&params.session_id).unwrap();
        if let Some(handler) = &self.on_user_message {
            let handler = handler.clone();
            let thread = thread.clone();
            cx.spawn(async move |cx| handler(params, thread, cx.clone()).await)
        } else {
            Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
        }
    }

    fn client_user_message_ids(
        &self,
        _cx: &App,
    ) -> Option<Rc<dyn AgentSessionClientUserMessageIds>> {
        self.supports_truncate.then(|| {
            Rc::new(FakeAgentSessionClientUserMessageIds {
                connection: self.clone(),
            }) as Rc<dyn AgentSessionClientUserMessageIds>
        })
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {}

    fn truncate(
        &self,
        session_id: &acp::SessionId,
        _cx: &App,
    ) -> Option<Rc<dyn AgentSessionTruncate>> {
        self.supports_truncate.then(|| {
            Rc::new(FakeAgentSessionEditor {
                _session_id: session_id.clone(),
            }) as Rc<dyn AgentSessionTruncate>
        })
    }

    fn set_title(
        &self,
        _session_id: &acp::SessionId,
        _cx: &App,
    ) -> Option<Rc<dyn AgentSessionSetTitle>> {
        Some(Rc::new(FakeAgentSessionSetTitle {
            calls: self.set_title_calls.clone(),
        }))
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

struct FakeAgentSessionSetTitle {
    calls: Rc<RefCell<Vec<SharedString>>>,
}

impl AgentSessionSetTitle for FakeAgentSessionSetTitle {
    fn run(&self, title: SharedString, _cx: &mut App) -> Task<Result<()>> {
        self.calls.borrow_mut().push(title);
        Task::ready(Ok(()))
    }
}

struct FakeAgentSessionEditor {
    _session_id: acp::SessionId,
}

impl AgentSessionTruncate for FakeAgentSessionEditor {
    fn run(&self, _client_user_message_id: ClientUserMessageId, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }
}

struct FakeAgentSessionClientUserMessageIds {
    connection: FakeAgentConnection,
}

impl AgentSessionClientUserMessageIds for FakeAgentSessionClientUserMessageIds {
    fn prompt(
        &self,
        _client_user_message_id: ClientUserMessageId,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        self.connection.prompt(params, cx)
    }
}
