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
mod basic_flow;
mod cancellation;
mod context_server_helpers;
mod core_flow;
mod mcp_replay;
mod mcp_tools;
mod mid_turn_refresh;
mod permission_options;
mod queued_messages;
mod send_retry;
mod send_state;
mod streaming_tool_errors;
mod subagent_context;
mod subagent_end_to_end;
mod subagent_errors;
mod subagent_gating;
mod subagent_lifecycle;
mod terminal_basic;
mod terminal_cancellation;
mod terminal_permissions;
mod test_environment;
mod test_tools;
mod title_generation;
mod token_accounting;
mod tool_allow_rules;
mod tool_calls;
mod tool_deny_rules;
mod truncation_usage;

pub(crate) use context_server_helpers::*;
pub(crate) use test_environment::*;
use test_tools::*;

pub(crate) fn init_test(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let settings_store = SettingsStore::test(cx);
        cx.set_global(settings_store);
    });
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
