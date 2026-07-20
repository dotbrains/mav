use super::*;
use acp_thread::{
    AgentConnection, AgentModelGroupName, AgentModelList, ClientUserMessageId, PermissionOptions,
    ThreadStatus,
};
use agent_client_protocol::schema::v1 as acp;
use agent_settings::AgentProfileId;
use anyhow::Result;
use client::{Client, RefreshLlmTokenListener, UserStore};
use collections::IndexMap;
use context_server::{ContextServer, ContextServerCommand, ContextServerId};
use feature_flags::FeatureFlagAppExt as _;
use fs::{FakeFs, Fs};
use futures::{
    FutureExt as _, StreamExt,
    channel::{
        mpsc::{self, UnboundedReceiver},
        oneshot,
    },
    future::{Fuse, Shared},
};
use gpui::{
    App, AppContext, AsyncApp, Entity, Task, TestAppContext, UpdateGlobal,
    http_client::FakeHttpClient,
};
use indoc::indoc;
use language_model::{
    CompletionIntent, LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelId, LanguageModelImageExt, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelRegistry, LanguageModelRequest, LanguageModelRequestMessage,
    LanguageModelToolResult, LanguageModelToolSchemaFormat, LanguageModelToolUse, MessageContent,
    Role, StopReason, TokenUsage,
    fake_provider::{FakeLanguageModel, FakeLanguageModelProvider},
};
use pretty_assertions::assert_eq;
use project::{
    Project, context_server_store::ContextServerStore, project_settings::ProjectSettings,
};
use prompt_store::ProjectContext;
use reqwest_client::ReqwestClient;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use settings::{LanguageModelProviderSetting, LanguageModelSelection, Settings, SettingsStore};
use std::{
    path::Path,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use util::path;

mod authorization_resolution;
mod cancellation;
mod core_flow;
mod mcp_replay;
mod mcp_tools;
mod permission_options;
mod send_retry;
mod send_state;
mod subagent_context;
mod subagent_end_to_end;
mod subagent_errors;
mod subagent_gating;
mod subagent_lifecycle;
mod terminal_cancellation;
mod terminal_permissions;
mod test_tools;
mod title_generation;
mod token_accounting;
mod tool_allow_rules;
mod tool_deny_rules;
mod truncation_usage;

use test_tools::*;

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
}

pub(crate) struct FakeTerminalHandle {
    killed: Arc<AtomicBool>,
    stopped_by_user: Arc<AtomicBool>,
    exit_sender: std::cell::RefCell<Option<futures::channel::oneshot::Sender<()>>>,
    wait_for_exit: Shared<Task<acp::TerminalExitStatus>>,
    output: acp::TerminalOutputResponse,
    id: acp::TerminalId,
}

impl FakeTerminalHandle {
    pub(crate) fn new_never_exits(cx: &mut App) -> Self {
        let killed = Arc::new(AtomicBool::new(false));
        let stopped_by_user = Arc::new(AtomicBool::new(false));

        let (exit_sender, exit_receiver) = futures::channel::oneshot::channel();

        let wait_for_exit = cx
            .spawn(async move |_cx| {
                // Wait for the exit signal (sent when kill() is called)
                let _ = exit_receiver.await;
                acp::TerminalExitStatus::new()
            })
            .shared();

        Self {
            killed,
            stopped_by_user,
            exit_sender: std::cell::RefCell::new(Some(exit_sender)),
            wait_for_exit,
            output: acp::TerminalOutputResponse::new("partial output".to_string(), false),
            id: acp::TerminalId::new("fake_terminal".to_string()),
        }
    }

    pub(crate) fn new_with_immediate_exit(cx: &mut App, exit_code: u32) -> Self {
        let killed = Arc::new(AtomicBool::new(false));
        let stopped_by_user = Arc::new(AtomicBool::new(false));
        let (exit_sender, _exit_receiver) = futures::channel::oneshot::channel();

        let wait_for_exit = cx
            .spawn(async move |_cx| acp::TerminalExitStatus::new().exit_code(exit_code))
            .shared();

        Self {
            killed,
            stopped_by_user,
            exit_sender: std::cell::RefCell::new(Some(exit_sender)),
            wait_for_exit,
            output: acp::TerminalOutputResponse::new("command output".to_string(), false),
            id: acp::TerminalId::new("fake_terminal".to_string()),
        }
    }

    pub(crate) fn with_output(mut self, output: acp::TerminalOutputResponse) -> Self {
        self.output = output;
        self
    }

    pub(crate) fn was_killed(&self) -> bool {
        self.killed.load(Ordering::SeqCst)
    }

    pub(crate) fn set_stopped_by_user(&self, stopped: bool) {
        self.stopped_by_user.store(stopped, Ordering::SeqCst);
    }

    pub(crate) fn signal_exit(&self) {
        if let Some(sender) = self.exit_sender.borrow_mut().take() {
            let _ = sender.send(());
        }
    }
}

impl crate::TerminalHandle for FakeTerminalHandle {
    fn id(&self, _cx: &AsyncApp) -> Result<acp::TerminalId> {
        Ok(self.id.clone())
    }

    fn current_output(&self, _cx: &AsyncApp) -> Result<acp::TerminalOutputResponse> {
        Ok(self.output.clone())
    }

    fn wait_for_exit(&self, _cx: &AsyncApp) -> Result<Shared<Task<acp::TerminalExitStatus>>> {
        Ok(self.wait_for_exit.clone())
    }

    fn kill(&self, _cx: &AsyncApp) -> Result<()> {
        self.killed.store(true, Ordering::SeqCst);
        self.signal_exit();
        Ok(())
    }

    fn was_stopped_by_user(&self, _cx: &AsyncApp) -> Result<bool> {
        Ok(self.stopped_by_user.load(Ordering::SeqCst))
    }
}

struct FakeSubagentHandle {
    session_id: acp::SessionId,
    send_task: Shared<Task<String>>,
}

impl SubagentHandle for FakeSubagentHandle {
    fn id(&self) -> acp::SessionId {
        self.session_id.clone()
    }

    fn num_entries(&self, _cx: &App) -> usize {
        unimplemented!()
    }

    fn send(&self, _message: String, cx: &AsyncApp) -> Task<Result<String>> {
        let task = self.send_task.clone();
        cx.background_spawn(async move { Ok(task.await) })
    }
}

#[derive(Default)]
pub(crate) struct FakeThreadEnvironment {
    terminal_handle: Option<Rc<FakeTerminalHandle>>,
    subagent_handle: Option<Rc<FakeSubagentHandle>>,
    terminal_creations: Arc<AtomicUsize>,
    terminal_output_limits: std::cell::RefCell<Vec<Option<u64>>>,
}

impl FakeThreadEnvironment {
    pub(crate) fn with_terminal(self, terminal_handle: FakeTerminalHandle) -> Self {
        Self {
            terminal_handle: Some(terminal_handle.into()),
            ..self
        }
    }

    pub(crate) fn terminal_creation_count(&self) -> usize {
        self.terminal_creations.load(Ordering::SeqCst)
    }

    pub(crate) fn terminal_output_limits(&self) -> Vec<Option<u64>> {
        self.terminal_output_limits.borrow().clone()
    }
}

impl crate::ThreadEnvironment for FakeThreadEnvironment {
    fn create_terminal(
        &self,
        _command: String,
        _extra_env: Vec<acp::EnvVariable>,
        _cwd: Option<std::path::PathBuf>,
        output_byte_limit: Option<u64>,
        _sandbox_wrap: Option<acp_thread::SandboxWrap>,
        _cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn crate::TerminalHandle>>> {
        self.terminal_creations.fetch_add(1, Ordering::SeqCst);
        self.terminal_output_limits
            .borrow_mut()
            .push(output_byte_limit);
        let handle = self
            .terminal_handle
            .clone()
            .expect("Terminal handle not available on FakeThreadEnvironment");
        Task::ready(Ok(handle as Rc<dyn crate::TerminalHandle>))
    }

    fn create_subagent(&self, _label: String, _cx: &mut App) -> Result<Rc<dyn SubagentHandle>> {
        Ok(self
            .subagent_handle
            .clone()
            .expect("Subagent handle not available on FakeThreadEnvironment")
            as Rc<dyn SubagentHandle>)
    }
}

/// Environment that creates multiple independent terminal handles for testing concurrent terminals.
struct MultiTerminalEnvironment {
    handles: std::cell::RefCell<Vec<Rc<FakeTerminalHandle>>>,
}

impl MultiTerminalEnvironment {
    fn new() -> Self {
        Self {
            handles: std::cell::RefCell::new(Vec::new()),
        }
    }

    fn handles(&self) -> Vec<Rc<FakeTerminalHandle>> {
        self.handles.borrow().clone()
    }
}

impl crate::ThreadEnvironment for MultiTerminalEnvironment {
    fn create_terminal(
        &self,
        _command: String,
        _extra_env: Vec<acp::EnvVariable>,
        _cwd: Option<std::path::PathBuf>,
        _output_byte_limit: Option<u64>,
        _sandbox_wrap: Option<acp_thread::SandboxWrap>,
        cx: &mut AsyncApp,
    ) -> Task<Result<Rc<dyn crate::TerminalHandle>>> {
        let handle = Rc::new(cx.update(|cx| FakeTerminalHandle::new_never_exits(cx)));
        self.handles.borrow_mut().push(handle.clone());
        Task::ready(Ok(handle as Rc<dyn crate::TerminalHandle>))
    }

    fn create_subagent(&self, _label: String, _cx: &mut App) -> Result<Rc<dyn SubagentHandle>> {
        unimplemented!()
    }
}

fn always_allow_tools(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.tool_permissions.default = settings::ToolPermissionMode::Allow;
        agent_settings::AgentSettings::override_global(settings, cx);
    });
}

/// Turns terminal sandboxing off so the non-sandboxed `TerminalTool` is the
/// variant exposed to the model as `terminal`. Tests that register
/// `TerminalTool` directly need this because sandboxing is enabled by default
/// for staff (and in debug builds), in which case `Thread::enabled_tools`
/// would otherwise expose `SandboxedTerminalTool` under that name instead.
fn disable_sandboxing(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = agent_settings::AgentSettings::get_global(cx).clone();
        settings.sandbox_permissions.allow_unsandboxed = true;
        agent_settings::AgentSettings::override_global(settings, cx);
    });
}

#[gpui::test]
async fn test_echo(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Testing: Reply with 'Hello'"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hello");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events = events.collect().await;
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.last_received_or_pending_message().unwrap().role(),
            Role::Assistant
        );
        assert_eq!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .to_markdown(),
            "Hello\n"
        )
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_terminal_tool_timeout_kills_handle(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::TerminalTool::new(project, environment));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::TerminalToolInput {
                command: "sleep 1000".to_string(),
                cd: ".".to_string(),
                timeout_ms: Some(5),
                ..Default::default()
            }),
            event_stream,
            cx,
        )
    });

    let update = rx.expect_update_fields().await;
    assert!(
        update.content.iter().any(|blocks| {
            blocks
                .iter()
                .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
        }),
        "expected tool call update to include terminal content"
    );

    let mut task_future: Pin<Box<Fuse<Task<Result<String, String>>>>> = Box::pin(task.fuse());

    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    loop {
        if let Some(result) = task_future.as_mut().now_or_never() {
            let result = result.expect("terminal tool task should complete");

            assert!(
                handle.was_killed(),
                "expected terminal handle to be killed on timeout"
            );
            assert!(
                result.contains("partial output"),
                "expected result to include terminal output, got: {result}"
            );
            return;
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for terminal tool task to complete");
        }

        cx.run_until_parked();
        cx.background_executor.timer(Duration::from_millis(1)).await;
    }
}

#[gpui::test]
#[ignore]
async fn test_terminal_tool_without_timeout_does_not_kill_handle(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let fs = FakeFs::new(cx.executor());
    let project = Project::test(fs, [], cx).await;

    let environment = Rc::new(cx.update(|cx| {
        FakeThreadEnvironment::default().with_terminal(FakeTerminalHandle::new_never_exits(cx))
    }));
    let handle = environment.terminal_handle.clone().unwrap();

    #[allow(clippy::arc_with_non_send_sync)]
    let tool = Arc::new(crate::TerminalTool::new(project, environment));
    let (event_stream, mut rx) = crate::ToolCallEventStream::test();

    let _task = cx.update(|cx| {
        tool.run(
            ToolInput::resolved(crate::TerminalToolInput {
                command: "sleep 1000".to_string(),
                cd: ".".to_string(),
                timeout_ms: None,
                ..Default::default()
            }),
            event_stream,
            cx,
        )
    });

    let update = rx.expect_update_fields().await;
    assert!(
        update.content.iter().any(|blocks| {
            blocks
                .iter()
                .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
        }),
        "expected tool call update to include terminal content"
    );

    cx.background_executor
        .timer(Duration::from_millis(25))
        .await;

    assert!(
        !handle.was_killed(),
        "did not expect terminal handle to be killed without a timeout"
    );
}

#[gpui::test]
async fn test_thinking(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                [indoc! {"
                    Testing:

                    Generate a thinking step where you just think the word 'Think',
                    and have your final answer be 'Hello'
                "}],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Thinking {
        text: "Think".to_string(),
        signature: None,
    });
    fake_model.send_last_completion_stream_text_chunk("Hello");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events = events.collect().await;
    thread.update(cx, |thread, _cx| {
        assert_eq!(
            thread.last_received_or_pending_message().unwrap().role(),
            Role::Assistant
        );
        assert_eq!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .to_markdown(),
            indoc! {"
                <think>Think</think>
                Hello
            "}
        )
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

#[gpui::test]
async fn test_thinking_allowed_when_model_cannot_disable_thinking(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();
    fake_model.set_supports_thinking(true);

    // With thinking toggled off, a model that can disable thinking honors
    // the toggle...
    thread.update(cx, |thread, cx| {
        thread.set_thinking_enabled(false, cx);
        let request = thread
            .build_completion_request(CompletionIntent::UserPrompt, cx)
            .unwrap();
        assert!(!request.thinking_allowed);
    });

    // ...but a model that always thinks ignores the stale toggle state.
    fake_model.set_supports_disabling_thinking(false);
    thread.update(cx, |thread, cx| {
        let request = thread
            .build_completion_request(CompletionIntent::UserPrompt, cx)
            .unwrap();
        assert!(request.thinking_allowed);
    });
}

#[gpui::test]
async fn test_system_prompt(cx: &mut TestAppContext) {
    let ThreadTest {
        model,
        thread,
        project_context,
        ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    project_context.update(cx, |project_context, _cx| {
        project_context.shell = "test-shell".into()
    });
    thread.update(cx, |thread, _| thread.add_tool(EchoTool));
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(
        pending_completions.len(),
        1,
        "unexpected pending completions: {:?}",
        pending_completions
    );

    let pending_completion = pending_completions.pop().unwrap();
    assert_eq!(pending_completion.messages[0].role, Role::System);

    let system_message = &pending_completion.messages[0];
    let MessageContent::Text(system_prompt) = &system_message.content[0] else {
        panic!("Expected text content");
    };
    assert!(
        system_prompt.contains("test-shell"),
        "unexpected system message: {:?}",
        system_message
    );
    assert!(
        system_prompt.contains("## Fixing Diagnostics"),
        "unexpected system message: {:?}",
        system_message
    );
}

#[gpui::test]
async fn test_system_prompt_without_tools(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(
        pending_completions.len(),
        1,
        "unexpected pending completions: {:?}",
        pending_completions
    );

    let pending_completion = pending_completions.pop().unwrap();
    assert_eq!(pending_completion.messages[0].role, Role::System);

    let system_message = &pending_completion.messages[0];
    let MessageContent::Text(system_prompt) = &system_message.content[0] else {
        panic!("Expected text content");
    };
    assert!(
        !system_prompt.contains("## Tool Use"),
        "unexpected system message: {:?}",
        system_message
    );
    assert!(
        !system_prompt.contains("## Fixing Diagnostics"),
        "unexpected system message: {:?}",
        system_message
    );
}

#[gpui::test]
async fn test_prompt_caching(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Send initial user message and verify it's cached
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Message 1"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages[1..],
        vec![LanguageModelRequestMessage {
            role: Role::User,
            content: vec!["Message 1".into()],
            cache: true,
            reasoning_details: None,
        }]
    );
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Text(
        "Response to Message 1".into(),
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // Send another user message and verify only the latest is cached
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Message 2"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 2".into()],
                cache: true,
                reasoning_details: None,
            }
        ]
    );
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::Text(
        "Response to Message 2".into(),
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // Simulate a tool call and verify that the latest tool result is cached
    thread.update(cx, |thread, _| thread.add_tool(EchoTool));
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use = LanguageModelToolUse {
        id: "tool_1".into(),
        name: EchoTool::NAME.into(),
        raw_input: json!({"text": "test"}).to_string(),
        input: json!({"text": "test"}),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use.clone()));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let tool_result = LanguageModelToolResult {
        tool_use_id: "tool_1".into(),
        tool_name: EchoTool::NAME.into(),
        is_error: false,
        content: vec!["test".into()],
        output: Some("test".into()),
    };
    assert_eq!(
        completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 1".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Message 2".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec!["Response to Message 2".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Use the echo tool".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![MessageContent::ToolUse(tool_use)],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![MessageContent::ToolResult(tool_result)],
                cache: true,
                reasoning_details: None,
            }
        ]
    );
}

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_basic_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    // Test a tool call that's likely to complete *before* streaming stops.
    let events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(EchoTool);
            thread.send(
                ClientUserMessageId::new(),
                ["Now test the echo tool with 'Hello'. Does it work? Say 'Yes' or 'No'."],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);

    // Test a tool calls that's likely to complete *after* streaming stops.
    let events = thread
        .update(cx, |thread, cx| {
            thread.remove_tool(&EchoTool::NAME);
            thread.add_tool(DelayTool);
            thread.send(
                ClientUserMessageId::new(),
                [
                    "Now call the delay tool with 200ms.",
                    "When the timer goes off, then you echo the output of the tool.",
                ],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
    thread.update(cx, |thread, _cx| {
        assert!(
            thread
                .last_received_or_pending_message()
                .unwrap()
                .as_agent_message()
                .unwrap()
                .content
                .iter()
                .any(|content| {
                    if let AgentMessageContent::Text(text) = content {
                        text.contains("Ding")
                    } else {
                        false
                    }
                }),
            "{}",
            thread.to_markdown()
        );
    });
}

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_streaming_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    // Test a tool call that's likely to complete *before* streaming stops.
    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(WordListTool);
            thread.send(ClientUserMessageId::new(), ["Test the word_list tool."], cx)
        })
        .unwrap();

    let mut saw_partial_tool_use = false;
    while let Some(event) = events.next().await {
        if let Ok(ThreadEvent::ToolCall(tool_call)) = event {
            thread.update(cx, |thread, _cx| {
                // Look for a tool use in the thread's last message
                let message = thread.last_received_or_pending_message().unwrap();
                let agent_message = message.as_agent_message().unwrap();
                let last_content = agent_message.content.last().unwrap();
                if let AgentMessageContent::ToolUse(last_tool_use) = last_content {
                    assert_eq!(last_tool_use.name.as_ref(), "word_list");
                    if tool_call.status == acp::ToolCallStatus::Pending {
                        if !last_tool_use.is_input_complete
                            && last_tool_use.input.get("g").is_none()
                        {
                            saw_partial_tool_use = true;
                        }
                    } else {
                        last_tool_use
                            .input
                            .get("a")
                            .expect("'a' has streamed because input is now complete");
                        last_tool_use
                            .input
                            .get("g")
                            .expect("'g' has streamed because input is now complete");
                    }
                } else {
                    panic!("last content should be a tool use");
                }
            });
        }
    }

    assert!(
        saw_partial_tool_use,
        "should see at least one partially streamed tool use in the history"
    );
}

#[gpui::test]
async fn test_tool_authorization(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(ToolRequiringPermission);
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_2".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    let tool_call_auth_1 = next_tool_call_authorization(&mut events).await;
    let tool_call_auth_2 = next_tool_call_authorization(&mut events).await;

    // Approve the first - send "allow" option_id (UI transforms "once" to "allow")
    tool_call_auth_1
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("allow"),
            acp::PermissionOptionKind::AllowOnce,
        ))
        .unwrap();
    cx.run_until_parked();

    // Reject the second - send "deny" option_id directly since Deny is now a button
    tool_call_auth_2
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("deny"),
            acp::PermissionOptionKind::RejectOnce,
        ))
        .unwrap();
    cx.run_until_parked();

    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![
            language_model::MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: tool_call_auth_1.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }),
            language_model::MessageContent::ToolResult(LanguageModelToolResult {
                tool_use_id: tool_call_auth_2.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: true,
                content: vec!["Permission to run tool denied by user".into()],
                output: Some("Permission to run tool denied by user".into())
            })
        ]
    );

    // Simulate yet another tool call.
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_3".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    // Respond by always allowing tools - send transformed option_id
    // (UI transforms "always:tool_requiring_permission" to "always_allow:tool_requiring_permission")
    let tool_call_auth_3 = next_tool_call_authorization(&mut events).await;
    tool_call_auth_3
        .response
        .send(acp_thread::SelectedPermissionOutcome::new(
            acp::PermissionOptionId::new("always_allow:tool_requiring_permission"),
            acp::PermissionOptionKind::AllowAlways,
        ))
        .unwrap();
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![language_model::MessageContent::ToolResult(
            LanguageModelToolResult {
                tool_use_id: tool_call_auth_3.tool_call.tool_call_id.0.to_string().into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }
        )]
    );

    // Simulate a final tool call, ensuring we don't trigger authorization.
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_4".into(),
            name: ToolRequiringPermission::NAME.into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();
    cx.run_until_parked();
    let completion = fake_model.pending_completions().pop().unwrap();
    let message = completion.messages.last().unwrap();
    assert_eq!(
        message.content,
        vec![language_model::MessageContent::ToolResult(
            LanguageModelToolResult {
                tool_use_id: "tool_id_4".into(),
                tool_name: ToolRequiringPermission::NAME.into(),
                is_error: false,
                content: vec!["Allowed".into()],
                output: Some("Allowed".into())
            }
        )]
    );
}

#[gpui::test]
async fn test_tool_hallucination(cx: &mut TestAppContext) {
    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["abc"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_id_1".into(),
            name: "nonexistent_tool".into(),
            raw_input: "{}".into(),
            input: json!({}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model.end_last_completion_stream();

    let tool_call = expect_tool_call(&mut events).await;
    assert_eq!(tool_call.title, "nonexistent_tool");
    assert_eq!(tool_call.status, acp::ToolCallStatus::Pending);
    let update = expect_tool_call_update_fields(&mut events).await;
    assert_eq!(update.fields.status, Some(acp::ToolCallStatus::Failed));
}

async fn expect_tool_call(events: &mut UnboundedReceiver<Result<ThreadEvent>>) -> acp::ToolCall {
    let event = events
        .next()
        .await
        .expect("no tool call authorization event received")
        .unwrap();
    match event {
        ThreadEvent::ToolCall(tool_call) => tool_call,
        event => {
            panic!("Unexpected event {event:?}");
        }
    }
}

async fn expect_tool_call_update_fields(
    events: &mut UnboundedReceiver<Result<ThreadEvent>>,
) -> acp::ToolCallUpdate {
    let event = events
        .next()
        .await
        .expect("no tool call authorization event received")
        .unwrap();
    match event {
        ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(update)) => update,
        event => {
            panic!("Unexpected event {event:?}");
        }
    }
}

async fn next_tool_call_authorization(
    events: &mut UnboundedReceiver<Result<ThreadEvent>>,
) -> ToolCallAuthorization {
    loop {
        let event = events
            .next()
            .await
            .expect("no tool call authorization event received")
            .unwrap();
        if let ThreadEvent::ToolCallAuthorization(tool_call_authorization) = event {
            let permission_kinds = tool_call_authorization
                .options
                .first_option_of_kind(acp::PermissionOptionKind::AllowAlways)
                .map(|option| option.kind);
            let allow_once = tool_call_authorization
                .options
                .first_option_of_kind(acp::PermissionOptionKind::AllowOnce)
                .map(|option| option.kind);

            assert_eq!(
                permission_kinds,
                Some(acp::PermissionOptionKind::AllowAlways)
            );
            assert_eq!(allow_once, Some(acp::PermissionOptionKind::AllowOnce));
            return tool_call_authorization;
        }
    }
}

#[gpui::test]
#[cfg_attr(not(feature = "e2e"), ignore)]
async fn test_concurrent_tool_calls(cx: &mut TestAppContext) {
    let ThreadTest { thread, .. } = setup(cx, TestModel::Sonnet4).await;

    // Test concurrent tool calls with different delay times
    let events = thread
        .update(cx, |thread, cx| {
            thread.add_tool(DelayTool);
            thread.send(
                ClientUserMessageId::new(),
                [
                    "Call the delay tool twice in the same message.",
                    "Once with 100ms. Once with 300ms.",
                    "When both timers are complete, describe the outputs.",
                ],
                cx,
            )
        })
        .unwrap()
        .collect()
        .await;

    let stop_reasons = stop_events(events);
    assert_eq!(stop_reasons, vec![acp::StopReason::EndTurn]);

    thread.update(cx, |thread, _cx| {
        let last_message = thread.last_received_or_pending_message().unwrap();
        let agent_message = last_message.as_agent_message().unwrap();
        let text = agent_message
            .content
            .iter()
            .filter_map(|content| {
                if let AgentMessageContent::Text(text) = content {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<String>();

        assert!(text.contains("Ding"));
    });
}

#[gpui::test]
async fn test_profiles(cx: &mut TestAppContext) {
    let ThreadTest {
        model, thread, fs, ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(DelayTool);
        thread.add_tool(EchoTool);
        thread.add_tool(InfiniteTool);
    });

    // Override profiles and wait for settings to be loaded.
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "profiles": {
                    "test-1": {
                        "name": "Test Profile 1",
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                        }
                    },
                    "test-2": {
                        "name": "Test Profile 2",
                        "tools": {
                            InfiniteTool::NAME: true,
                        }
                    }
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();

    // Test that test-1 profile (default) has echo and delay tools
    thread
        .update(cx, |thread, cx| {
            thread.set_profile(AgentProfileId("test-1".into()), cx);
            thread.send(ClientUserMessageId::new(), ["test"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(pending_completions.len(), 1);
    let completion = pending_completions.pop().unwrap();
    let tool_names: Vec<String> = completion
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    assert_eq!(tool_names, vec![DelayTool::NAME, EchoTool::NAME]);
    fake_model.end_last_completion_stream();

    // Switch to test-2 profile, and verify that it has only the infinite tool.
    thread
        .update(cx, |thread, cx| {
            thread.set_profile(AgentProfileId("test-2".into()), cx);
            thread.send(ClientUserMessageId::new(), ["test2"], cx)
        })
        .unwrap();
    cx.run_until_parked();
    let mut pending_completions = fake_model.pending_completions();
    assert_eq!(pending_completions.len(), 1);
    let completion = pending_completions.pop().unwrap();
    let tool_names: Vec<String> = completion
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    assert_eq!(tool_names, vec![InfiniteTool::NAME]);
}

async fn verify_thread_recovery(
    thread: &Entity<Thread>,
    fake_model: &FakeLanguageModel,
    cx: &mut TestAppContext,
) {
    let events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Testing: reply with 'Hello' then stop."],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    fake_model.send_last_completion_stream_text_chunk("Hello");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let events = events.collect::<Vec<_>>().await;
    thread.update(cx, |thread, _cx| {
        let message = thread.last_received_or_pending_message().unwrap();
        let agent_message = message.as_agent_message().unwrap();
        assert_eq!(
            agent_message.content,
            vec![AgentMessageContent::Text("Hello".to_string())]
        );
    });
    assert_eq!(stop_events(events), vec![acp::StopReason::EndTurn]);
}

/// Waits for a terminal tool to start by watching for a ToolCallUpdate with terminal content.
async fn wait_for_terminal_tool_started(
    events: &mut mpsc::UnboundedReceiver<Result<ThreadEvent>>,
    cx: &mut TestAppContext,
) {
    let deadline = cx.executor().num_cpus() * 100; // Scale with available parallelism
    for _ in 0..deadline {
        cx.run_until_parked();

        while let Some(Some(event)) = events.next().now_or_never() {
            if let Ok(ThreadEvent::ToolCallUpdate(acp_thread::ToolCallUpdate::UpdateFields(
                update,
            ))) = &event
            {
                if update.fields.content.as_ref().is_some_and(|content| {
                    content
                        .iter()
                        .any(|c| matches!(c, acp::ToolCallContent::Terminal(_)))
                }) {
                    return;
                }
            }
        }

        cx.background_executor
            .timer(Duration::from_millis(10))
            .await;
    }
    panic!("terminal tool did not start within the expected time");
}

/// Collects events until a Stop event is received, driving the executor to completion.
async fn collect_events_until_stop(
    events: &mut mpsc::UnboundedReceiver<Result<ThreadEvent>>,
    cx: &mut TestAppContext,
) -> Vec<Result<ThreadEvent>> {
    let mut collected = Vec::new();
    let deadline = cx.executor().num_cpus() * 200;

    for _ in 0..deadline {
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();

        while let Some(Some(event)) = events.next().now_or_never() {
            let is_stop = matches!(&event, Ok(ThreadEvent::Stop(_)));
            collected.push(event);
            if is_stop {
                return collected;
            }
        }
    }
    panic!(
        "did not receive Stop event within the expected time; collected {} events",
        collected.len()
    );
}

fn stop_events(result_events: Vec<Result<ThreadEvent>>) -> Vec<acp::StopReason> {
    result_events
        .into_iter()
        .filter_map(|event| match event.unwrap() {
            ThreadEvent::Stop(stop_reason) => Some(stop_reason),
            _ => None,
        })
        .collect()
}

struct ThreadTest {
    model: Arc<dyn LanguageModel>,
    thread: Entity<Thread>,
    project_context: Entity<ProjectContext>,
    context_server_store: Entity<ContextServerStore>,
    fs: Arc<FakeFs>,
}

enum TestModel {
    Sonnet4,
    Fake,
}

impl TestModel {
    fn id(&self) -> LanguageModelId {
        match self {
            TestModel::Sonnet4 => LanguageModelId("claude-sonnet-4-latest".into()),
            TestModel::Fake => unreachable!(),
        }
    }
}

async fn setup(cx: &mut TestAppContext, model: TestModel) -> ThreadTest {
    cx.executor().allow_parking();

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.create_dir(paths::settings_file().parent().unwrap())
        .await
        .unwrap();
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "default_profile": "test-profile",
                "profiles": {
                    "test-profile": {
                        "name": "Test Profile",
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                            WordListTool::NAME: true,
                            ToolRequiringPermission::NAME: true,
                            ToolRequiringPermission2::NAME: true,
                            InfiniteTool::NAME: true,
                            CancellationAwareTool::NAME: true,
                            StreamingEchoTool::NAME: true,
                            StreamingJsonErrorContextTool::NAME: true,
                            StreamingFailingEchoTool::NAME: true,
                            TerminalTool::NAME: true,
                        }
                    }
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;

    cx.update(|cx| {
        settings::init(cx);

        match model {
            TestModel::Fake => {}
            TestModel::Sonnet4 => {
                gpui_tokio::init(cx);
                let http_client = ReqwestClient::user_agent("agent tests").unwrap();
                cx.set_http_client(Arc::new(http_client));
                let client = Client::production(cx);
                let user_store = cx.new(|cx| UserStore::new(client.clone(), cx));
                language_model::init(cx);
                RefreshLlmTokenListener::register(client.clone(), user_store.clone(), cx);
                language_models::init(user_store, client.clone(), cx);
            }
        };

        watch_settings(fs.clone(), cx);
    });

    let templates = Templates::new();

    fs.insert_tree(path!("/test"), json!({})).await;
    let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

    let model = cx
        .update(|cx| {
            if let TestModel::Fake = model {
                Task::ready(Arc::new(FakeLanguageModel::default()) as Arc<_>)
            } else {
                let model_id = model.id();
                let models = LanguageModelRegistry::read_global(cx);
                let model = models
                    .available_models(cx)
                    .find(|model| model.id() == model_id)
                    .unwrap();

                let provider = models.provider(&model.provider_id()).unwrap();
                let authenticated = provider.authenticate(cx);

                cx.spawn(async move |_cx| {
                    authenticated.await.unwrap();
                    model
                })
            }
        })
        .await;

    let project_context = cx.new(|_cx| ProjectContext::default());
    let context_server_store = project.read_with(cx, |project, _| project.context_server_store());
    let context_server_registry =
        cx.new(|cx| ContextServerRegistry::new(context_server_store.clone(), cx));
    let thread = cx.new(|cx| {
        Thread::new(
            project,
            project_context.clone(),
            context_server_registry,
            templates,
            Some(model.clone()),
            cx,
        )
    });
    ThreadTest {
        model,
        thread,
        project_context,
        context_server_store,
        fs,
    }
}

#[cfg(test)]
#[ctor::ctor(unsafe)]
fn init_logger() {
    if std::env::var("RUST_LOG").is_ok() {
        env_logger::init();
    }
}

fn watch_settings(fs: Arc<dyn Fs>, cx: &mut App) {
    let fs = fs.clone();
    cx.spawn({
        async move |cx| {
            let (mut new_settings_content_rx, watcher_task) = settings::watch_config_file(
                cx.background_executor(),
                fs,
                paths::settings_file().clone(),
            );
            let _watcher_task = watcher_task;

            while let Some(new_settings_content) = new_settings_content_rx.next().await {
                cx.update(|cx| {
                    SettingsStore::update_global(cx, |settings, cx| {
                        settings.set_user_settings(&new_settings_content, cx)
                    })
                })
                .ok();
            }
        }
    })
    .detach();
}

fn tool_names_for_completion(completion: &LanguageModelRequest) -> Vec<String> {
    completion
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect()
}

fn setup_context_server(
    name: &'static str,
    tools: Vec<context_server::types::Tool>,
    context_server_store: &Entity<ContextServerStore>,
    cx: &mut TestAppContext,
) -> mpsc::UnboundedReceiver<(
    context_server::types::CallToolParams,
    oneshot::Sender<context_server::types::CallToolResponse>,
)> {
    cx.update(|cx| {
        let mut settings = ProjectSettings::get_global(cx).clone();
        settings.context_servers.insert(
            name.into(),
            project::project_settings::ContextServerSettings::Stdio {
                enabled: true,
                remote: false,
                command: ContextServerCommand {
                    path: "somebinary".into(),
                    args: Vec::new(),
                    env: None,
                    timeout: None,
                },
            },
        );
        ProjectSettings::override_global(settings, cx);
    });

    let (mcp_tool_calls_tx, mcp_tool_calls_rx) = mpsc::unbounded();
    let fake_transport = context_server::test::create_fake_transport(name, cx.executor())
        .on_request::<context_server::types::requests::Initialize, _>(move |_params| async move {
            context_server::types::InitializeResponse {
                protocol_version: context_server::types::ProtocolVersion(
                    context_server::types::LATEST_PROTOCOL_VERSION.to_string(),
                ),
                server_info: context_server::types::Implementation {
                    name: name.into(),
                    title: None,
                    version: "1.0.0".to_string(),
                    description: None,
                },
                capabilities: context_server::types::ServerCapabilities {
                    tools: Some(context_server::types::ToolsCapabilities {
                        list_changed: Some(true),
                    }),
                    ..Default::default()
                },
                meta: None,
            }
        })
        .on_request::<context_server::types::requests::ListTools, _>(move |_params| {
            let tools = tools.clone();
            async move {
                context_server::types::ListToolsResponse {
                    tools,
                    next_cursor: None,
                    meta: None,
                }
            }
        })
        .on_request::<context_server::types::requests::CallTool, _>(move |params| {
            let mcp_tool_calls_tx = mcp_tool_calls_tx.clone();
            async move {
                let (response_tx, response_rx) = oneshot::channel();
                mcp_tool_calls_tx
                    .unbounded_send((params, response_tx))
                    .unwrap();
                response_rx.await.unwrap()
            }
        });
    context_server_store.update(cx, |store, cx| {
        store.start_server(
            Arc::new(ContextServer::new(
                ContextServerId(name.into()),
                Arc::new(fake_transport),
            )),
            cx,
        );
    });
    cx.run_until_parked();
    mcp_tool_calls_rx
}

#[gpui::test]
async fn test_queued_message_ends_turn_at_boundary(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    // Add a tool so we can simulate tool calls
    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
    });

    // Start a turn by sending a message
    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Simulate the model making a tool call
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text": "hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::ToolUse));

    // Request that the turn end at the next boundary (a "steering" queued message)
    thread.update(cx, |thread, _cx| {
        thread.set_end_turn_at_next_boundary(true);
    });

    // Now end the stream - tool will run, and the boundary check should see the queue
    fake_model.end_last_completion_stream();

    // Collect all events until the turn stops
    let all_events = collect_events_until_stop(&mut events, cx).await;

    // Verify we received the tool call event
    let tool_call_ids: Vec<_> = all_events
        .iter()
        .filter_map(|e| match e {
            Ok(ThreadEvent::ToolCall(tc)) => Some(tc.tool_call_id.to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tool_call_ids,
        vec!["tool_1"],
        "Should have received a tool call event for our echo tool"
    );

    // The turn should have stopped with EndTurn
    let stop_reasons = stop_events(all_events);
    assert_eq!(
        stop_reasons,
        vec![acp::StopReason::EndTurn],
        "Turn should have ended after tool completion due to queued message"
    );

    // Verify the boundary flag is still set
    thread.update(cx, |thread, _cx| {
        assert!(
            thread.end_turn_at_next_boundary(),
            "Should still have the end-turn-at-boundary flag set"
        );
    });

    // Thread should be idle now
    thread.update(cx, |thread, _cx| {
        assert!(
            thread.is_turn_complete(),
            "Thread should not be running after turn ends"
        );
    });
}

#[gpui::test]
async fn test_queued_message_does_not_end_turn_without_boundary_flag(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
    });

    let mut events = thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["Use the echo tool"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text": "hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::ToolUse));

    // Default behavior: even though a message is conceptually queued, we do NOT
    // set the boundary flag, so the agent must keep going past the tool boundary
    // (running to completion) rather than ending the turn early.
    fake_model.end_last_completion_stream();
    cx.run_until_parked();

    // The agent should have issued a fresh completion request with the tool
    // results instead of stopping — proof it continued past the boundary.
    let continuation = fake_model.pending_completions();
    assert_eq!(
        continuation.len(),
        1,
        "Without the boundary flag, the turn should continue with another completion request"
    );

    // Let the continuation finish the turn naturally.
    fake_model.send_last_completion_stream_text_chunk("All done");
    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::Stop(StopReason::EndTurn));
    fake_model.end_last_completion_stream();

    let all_events = collect_events_until_stop(&mut events, cx).await;
    let stop_reasons = stop_events(all_events);
    assert_eq!(
        stop_reasons,
        vec![acp::StopReason::EndTurn],
        "Turn should end only after the agent finishes, not at the tool boundary"
    );
}

#[gpui::test]
async fn test_streaming_tool_error_breaks_stream_loop_immediately(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(StreamingFailingEchoTool {
            receive_chunks_until_failure: 1,
        });
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_failing_echo tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    let tool_use = LanguageModelToolUse {
        id: "call_1".into(),
        name: StreamingFailingEchoTool::NAME.into(),
        raw_input: "hello".into(),
        input: json!({}),
        is_input_complete: false,
        thought_signature: None,
    };

    fake_model
        .send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(tool_use.clone()));

    cx.run_until_parked();

    let completions = fake_model.pending_completions();
    let last_completion = completions.last().unwrap();

    assert_eq!(
        last_completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec!["Use the streaming_failing_echo tool".into()],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![language_model::MessageContent::ToolUse(tool_use.clone())],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![language_model::MessageContent::ToolResult(
                    LanguageModelToolResult {
                        tool_use_id: tool_use.id.clone(),
                        tool_name: tool_use.name,
                        is_error: true,
                        content: vec!["failed".into()],
                        output: Some("failed".into()),
                    }
                )],
                cache: true,
                reasoning_details: None,
            },
        ]
    );
}

#[gpui::test]
async fn test_streaming_tool_error_waits_for_prior_tools_to_complete(cx: &mut TestAppContext) {
    init_test(cx);
    always_allow_tools(cx);

    let ThreadTest { model, thread, .. } = setup(cx, TestModel::Fake).await;
    let fake_model = model.as_fake();

    let (complete_streaming_echo_tool_call_tx, complete_streaming_echo_tool_call_rx) =
        oneshot::channel();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(
            StreamingEchoTool::new().with_wait_until_complete(complete_streaming_echo_tool_call_rx),
        );
        thread.add_tool(StreamingFailingEchoTool {
            receive_chunks_until_failure: 1,
        });
    });

    let _events = thread
        .update(cx, |thread, cx| {
            thread.send(
                ClientUserMessageId::new(),
                ["Use the streaming_echo tool and the streaming_failing_echo tool"],
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();

    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "call_1".into(),
            name: StreamingEchoTool::NAME.into(),
            raw_input: "hello".into(),
            input: json!({ "text": "hello" }),
            is_input_complete: false,
            thought_signature: None,
        },
    ));
    let first_tool_use = LanguageModelToolUse {
        id: "call_1".into(),
        name: StreamingEchoTool::NAME.into(),
        raw_input: "hello world".into(),
        input: json!({ "text": "hello world" }),
        is_input_complete: true,
        thought_signature: None,
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        first_tool_use.clone(),
    ));
    let second_tool_use = LanguageModelToolUse {
        name: StreamingFailingEchoTool::NAME.into(),
        raw_input: "hello".into(),
        input: json!({ "text": "hello" }),
        is_input_complete: false,
        thought_signature: None,
        id: "call_2".into(),
    };
    fake_model.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        second_tool_use.clone(),
    ));

    cx.run_until_parked();

    complete_streaming_echo_tool_call_tx.send(()).unwrap();

    cx.run_until_parked();

    let completions = fake_model.pending_completions();
    let last_completion = completions.last().unwrap();

    assert_eq!(
        last_completion.messages[1..],
        vec![
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    "Use the streaming_echo tool and the streaming_failing_echo tool".into()
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::Assistant,
                content: vec![
                    language_model::MessageContent::ToolUse(first_tool_use.clone()),
                    language_model::MessageContent::ToolUse(second_tool_use.clone())
                ],
                cache: false,
                reasoning_details: None,
            },
            LanguageModelRequestMessage {
                role: Role::User,
                content: vec![
                    language_model::MessageContent::ToolResult(LanguageModelToolResult {
                        tool_use_id: second_tool_use.id.clone(),
                        tool_name: second_tool_use.name,
                        is_error: true,
                        content: vec!["failed".into()],
                        output: Some("failed".into()),
                    }),
                    language_model::MessageContent::ToolResult(LanguageModelToolResult {
                        tool_use_id: first_tool_use.id.clone(),
                        tool_name: first_tool_use.name,
                        is_error: false,
                        content: vec!["hello world".into()],
                        output: Some("hello world".into()),
                    }),
                ],
                cache: true,
                reasoning_details: None,
            },
        ]
    );
}

#[gpui::test]
async fn test_mid_turn_model_and_settings_refresh(cx: &mut TestAppContext) {
    let ThreadTest {
        model, thread, fs, ..
    } = setup(cx, TestModel::Fake).await;
    let fake_model_a = model.as_fake();

    thread.update(cx, |thread, _cx| {
        thread.add_tool(EchoTool);
        thread.add_tool(DelayTool);
    });

    // Set up two profiles: profile-a has both tools, profile-b has only DelayTool.
    fs.insert_file(
        paths::settings_file(),
        json!({
            "agent": {
                "profiles": {
                    "profile-a": {
                        "name": "Profile A",
                        "tools": {
                            EchoTool::NAME: true,
                            DelayTool::NAME: true,
                        }
                    },
                    "profile-b": {
                        "name": "Profile B",
                        "tools": {
                            DelayTool::NAME: true,
                        }
                    }
                }
            }
        })
        .to_string()
        .into_bytes(),
    )
    .await;
    cx.run_until_parked();

    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("profile-a".into()), cx);
        thread.set_thinking_enabled(false, cx);
    });

    // Send a message — first iteration starts with model A, profile-a, thinking off.
    thread
        .update(cx, |thread, cx| {
            thread.send(ClientUserMessageId::new(), ["test mid-turn refresh"], cx)
        })
        .unwrap();
    cx.run_until_parked();

    // Verify first request has both tools and thinking disabled.
    let completions = fake_model_a.pending_completions();
    assert_eq!(completions.len(), 1);
    let first_tools = tool_names_for_completion(&completions[0]);
    assert_eq!(first_tools, vec![DelayTool::NAME, EchoTool::NAME]);
    assert!(!completions[0].thinking_allowed);

    // Model A responds with an echo tool call.
    fake_model_a.send_last_completion_stream_event(LanguageModelCompletionEvent::ToolUse(
        LanguageModelToolUse {
            id: "tool_1".into(),
            name: "echo".into(),
            raw_input: r#"{"text":"hello"}"#.into(),
            input: json!({"text": "hello"}),
            is_input_complete: true,
            thought_signature: None,
        },
    ));
    fake_model_a.end_last_completion_stream();

    // Before the next iteration runs, switch to profile-b (only DelayTool),
    // swap in a new model, and enable thinking.
    let fake_model_b = Arc::new(FakeLanguageModel::with_id_and_thinking(
        "test-provider",
        "model-b",
        "Model B",
        true,
    ));
    thread.update(cx, |thread, cx| {
        thread.set_profile(AgentProfileId("profile-b".into()), cx);
        thread.set_model(fake_model_b.clone() as Arc<dyn LanguageModel>, cx);
        thread.set_thinking_enabled(true, cx);
    });

    // Run until parked — processes the echo tool call, loops back, picks up
    // the new model/profile/thinking, and makes a second request to model B.
    cx.run_until_parked();

    // The second request should have gone to model B.
    let model_b_completions = fake_model_b.pending_completions();
    assert_eq!(
        model_b_completions.len(),
        1,
        "second request should go to model B"
    );

    // Profile-b only has DelayTool, so echo should be gone.
    let second_tools = tool_names_for_completion(&model_b_completions[0]);
    assert_eq!(second_tools, vec![DelayTool::NAME]);

    // Thinking should now be enabled.
    assert!(model_b_completions[0].thinking_allowed);
}
