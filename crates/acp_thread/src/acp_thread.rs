mod connection;
mod content_block;
mod diff;
mod entries;
mod mention;
mod metadata;
mod terminal;
mod thread_checkpoint;
mod thread_files;
mod thread_messages;
mod thread_plan;
mod thread_state;
mod thread_tool_calls;
mod thread_turn;
mod thread_types;
mod tool_call;
pub use ::terminal::HeadlessTerminal;
use action_log::{ActionLog, ActionLogTelemetry};
use agent_client_protocol::schema::{MaybeUndefined, v1 as acp};
use anyhow::{Context as _, Result, anyhow};
use collections::HashSet;
pub use connection::*;
pub use content_block::*;
pub use diff::*;
pub use entries::*;
use feature_flags::{AcpBetaFeatureFlag, FeatureFlagAppExt as _};
use futures::{FutureExt, channel::oneshot, future::BoxFuture};
use gpui::{
    AppContext, AsyncApp, Context, Entity, EventEmitter, SharedString, Subscription, Task,
    WeakEntity,
};
use itertools::Itertools;
use language::language_settings::FormatOnSave;
use language::{
    Anchor, Buffer, BufferEditSource, BufferSnapshot, LanguageRegistry, Point, ToPoint, text_diff,
};
use markdown::{Markdown, MarkdownOptions};
pub use mention::*;
pub use metadata::*;
use project::lsp_store::{FormatTrigger, LspFormatTarget};
use project::{
    AgentLocation, Project,
    git_store::{GitStoreCheckpoint, GitStoreEvent, RepositoryEvent},
};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Formatter, Write};
use std::ops::Range;
use std::process::ExitStatus;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{fmt::Display, mem, path::PathBuf, sync::Arc};
use task::{Shell, ShellBuilder};
pub use terminal::*;
use text::Bias;
pub use thread_types::*;
pub use tool_call::*;
use ui::App;
use util::markdown::MarkdownEscaped;
use util::path_list::PathList;
use util::{
    ResultExt, get_default_system_shell_preferring_bash,
    paths::{PathStyle, is_absolute},
};
use uuid::Uuid;

/// Returned when the model stops because it exhausted its output token budget.
#[derive(Debug)]
pub struct MaxOutputTokensError;

impl std::fmt::Display for MaxOutputTokensError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output token limit reached")
    }
}

impl std::error::Error for MaxOutputTokensError {}

struct RunningTurn {
    id: u32,
    send_task: Task<()>,
}

pub struct AcpThread {
    session_id: acp::SessionId,
    work_dirs: Option<PathList>,
    parent_session_id: Option<acp::SessionId>,
    title: Option<SharedString>,
    provisional_title: Option<SharedString>,
    entries: Vec<AgentThreadEntry>,
    plan: Plan,
    project: Entity<Project>,
    action_log: Entity<ActionLog>,
    _git_store_subscription: Subscription,
    update_last_checkpoint_if_changed_task: Option<Task<Result<()>>>,
    shared_buffers: HashMap<Entity<Buffer>, BufferSnapshot>,
    turn_id: u32,
    running_turn: Option<RunningTurn>,
    connection: Rc<dyn AgentConnection>,
    token_usage: Option<TokenUsage>,
    cost: Option<SessionCost>,
    prompt_capabilities: acp::PromptCapabilities,
    available_commands: Vec<acp::AvailableCommand>,
    _observe_prompt_capabilities: Task<anyhow::Result<()>>,
    terminals: HashMap<acp::TerminalId, Entity<Terminal>>,
    pending_terminal_output: HashMap<acp::TerminalId, Vec<Vec<u8>>>,
    pending_terminal_exit: HashMap<acp::TerminalId, acp::TerminalExitStatus>,
    had_error: bool,
    /// The user's unsent prompt text, persisted so it can be restored when reloading the thread.
    draft_prompt: Option<Vec<acp::ContentBlock>>,
    /// The initial scroll position for the thread view, set during session registration.
    ui_scroll_position: Option<gpui::ListOffset>,
    /// Buffer for smooth text streaming. Holds text that has been received from
    /// the model but not yet revealed in the UI. A timer task drains this buffer
    /// gradually to create a fluid typing effect instead of choppy chunk-at-a-time
    /// updates.
    streaming_text_buffer: Option<StreamingTextBuffer>,
}

struct StreamingTextBuffer {
    /// Text received from the model but not yet appended to the Markdown source.
    pending: String,
    /// The number of bytes to reveal per timer turn.
    bytes_to_reveal_per_tick: usize,
    /// The Markdown entity being streamed into.
    target: Entity<Markdown>,
    /// Timer task that periodically moves text from `pending` into `source`.
    _reveal_task: Task<()>,
}

impl StreamingTextBuffer {
    /// The number of milliseconds between each timer tick, controlling how quickly
    /// text is revealed.
    const TASK_UPDATE_MS: u64 = 16;
    /// The time in milliseconds to reveal the entire pending text.
    const REVEAL_TARGET: f32 = 200.0;
}

impl From<&AcpThread> for ActionLogTelemetry {
    fn from(value: &AcpThread) -> Self {
        Self {
            agent_telemetry_id: value.connection().telemetry_id(),
            session_id: value.session_id.0.clone(),
        }
    }
}

#[derive(Debug)]
pub enum AcpThreadEvent {
    StatusChanged,
    PromptUpdated,
    NewEntry,
    TitleUpdated,
    TokenUsageUpdated,
    EntryUpdated(usize),
    EntriesRemoved(Range<usize>),
    ToolAuthorizationRequested(acp::ToolCallId),
    ToolAuthorizationReceived(acp::ToolCallId),
    Retry(RetryStatus),
    SubagentSpawned(acp::SessionId),
    Stopped(acp::StopReason),
    Error,
    LoadError(LoadError),
    PromptCapabilitiesUpdated,
    Refusal,
    AvailableCommandsUpdated(Vec<acp::AvailableCommand>),
    ModeUpdated(acp::SessionModeId),
    ConfigOptionsUpdated(Vec<acp::SessionConfigOption>),
    WorkingDirectoriesUpdated,
}

impl EventEmitter<AcpThreadEvent> for AcpThread {}

#[derive(Debug, Clone)]
pub enum TerminalProviderEvent {
    Created {
        terminal_id: acp::TerminalId,
        label: String,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        terminal: Entity<::terminal::Terminal>,
    },
    Output {
        terminal_id: acp::TerminalId,
        data: Vec<u8>,
    },
    TitleChanged {
        terminal_id: acp::TerminalId,
        title: String,
    },
    Exit {
        terminal_id: acp::TerminalId,
        status: acp::TerminalExitStatus,
    },
}

#[derive(Debug, Clone)]
pub enum TerminalProviderCommand {
    WriteInput {
        terminal_id: acp::TerminalId,
        bytes: Vec<u8>,
    },
    Resize {
        terminal_id: acp::TerminalId,
        cols: u16,
        rows: u16,
    },
    Close {
        terminal_id: acp::TerminalId,
    },
}

#[derive(PartialEq, Eq, Debug)]
pub enum ThreadStatus {
    Idle,
    Generating,
}

#[derive(Debug, Clone)]
pub enum LoadError {
    Unsupported {
        command: SharedString,
        current_version: SharedString,
        minimum_version: SharedString,
    },
    FailedToInstall(SharedString),
    Exited {
        status: ExitStatus,
        stderr: Option<SharedString>,
    },
    Other(SharedString),
}

impl Display for LoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Unsupported {
                command: path,
                current_version,
                minimum_version,
            } => {
                write!(
                    f,
                    "version {current_version} from {path} is not supported (need at least {minimum_version})"
                )
            }
            LoadError::FailedToInstall(msg) => write!(f, "Failed to install: {msg}"),
            LoadError::Exited { status, .. } => write!(f, "Server exited with status {status}"),
            LoadError::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl Error for LoadError {}

impl AcpThread {
    pub fn handle_session_update(
        &mut self,
        update: acp::SessionUpdate,
        cx: &mut Context<Self>,
    ) -> Result<(), acp::Error> {
        match update {
            acp::SessionUpdate::UserMessageChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                // We optimistically add the full user prompt before calling `prompt`.
                // Some ACP servers echo user chunks back over updates. Skip echoed
                // chunks only when they match the local optimistic message.
                let already_in_user_message = self
                    .entries
                    .last_mut()
                    .and_then(|entry| match entry {
                        AgentThreadEntry::UserMessage(message) => Some(message),
                        _ => None,
                    })
                    .is_some_and(|message| {
                        let already_in_user_message = message.is_optimistic
                            && message.chunks.contains(&content)
                            && can_merge_message_chunks(
                                message.protocol_id.as_ref(),
                                message_id.as_ref(),
                            );
                        if already_in_user_message && message.protocol_id.is_none() {
                            message.protocol_id = message_id.clone();
                        }
                        already_in_user_message
                    });
                if !already_in_user_message {
                    self.push_user_content_block_from_agent(message_id, content, cx);
                }
            }
            acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                self.push_assistant_content_block_with_message_id(
                    message_id, content, false, false, cx,
                );
            }
            acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk {
                content,
                message_id,
                ..
            }) => {
                self.push_assistant_content_block_with_message_id(
                    message_id, content, true, false, cx,
                );
            }
            acp::SessionUpdate::ToolCall(tool_call) => {
                self.upsert_tool_call(tool_call, cx)?;
            }
            acp::SessionUpdate::ToolCallUpdate(tool_call_update) => {
                self.update_tool_call(tool_call_update, cx)?;
            }
            acp::SessionUpdate::Plan(plan) => {
                self.update_plan(plan, cx);
            }
            acp::SessionUpdate::SessionInfoUpdate(info_update) => {
                if let MaybeUndefined::Value(title) = info_update.title {
                    let had_provisional = self.provisional_title.take().is_some();
                    let title: SharedString = title.into();
                    if self.title.as_ref() != Some(&title) {
                        self.title = Some(title);
                        cx.emit(AcpThreadEvent::TitleUpdated);
                    } else if had_provisional {
                        cx.emit(AcpThreadEvent::TitleUpdated);
                    }
                }
            }
            acp::SessionUpdate::AvailableCommandsUpdate(acp::AvailableCommandsUpdate {
                available_commands,
                ..
            }) => {
                self.available_commands = available_commands.clone();
                cx.emit(AcpThreadEvent::AvailableCommandsUpdated(available_commands));
            }
            acp::SessionUpdate::CurrentModeUpdate(acp::CurrentModeUpdate {
                current_mode_id,
                ..
            }) => cx.emit(AcpThreadEvent::ModeUpdated(current_mode_id)),
            acp::SessionUpdate::ConfigOptionUpdate(acp::ConfigOptionUpdate {
                config_options,
                ..
            }) => cx.emit(AcpThreadEvent::ConfigOptionsUpdated(config_options)),
            acp::SessionUpdate::UsageUpdate(update) => {
                let usage = self.token_usage.get_or_insert_with(Default::default);
                usage.max_tokens = update.size;
                usage.used_tokens = update.used;
                if let Some(cost) = update.cost {
                    self.cost = Some(SessionCost {
                        amount: cost.amount,
                        currency: cost.currency.into(),
                    });
                }
                cx.emit(AcpThreadEvent::TokenUsageUpdated);
            }
            _ => {}
        }
        Ok(())
    }

    pub fn create_terminal(
        &self,
        command: String,
        args: Vec<String>,
        extra_env: Vec<acp::EnvVariable>,
        cwd: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        sandbox_wrap: Option<SandboxWrap>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Entity<Terminal>>> {
        let env = match &cwd {
            Some(dir) => self.project.update(cx, |project, cx| {
                project.environment().update(cx, |env, cx| {
                    env.directory_environment(dir.as_path().into(), cx)
                })
            }),
            None => Task::ready(None).shared(),
        };
        let env = cx.spawn(async move |_, _| {
            let mut env = env.await.unwrap_or_default();
            // Disables paging for `git` and hopefully other commands
            env.insert("PAGER".into(), "".into());
            for var in extra_env {
                env.insert(var.name, var.value);
            }
            env
        });

        let project = self.project.clone();
        let language_registry = project.read(cx).languages().clone();
        let is_windows = project.read(cx).path_style(cx).is_windows();
        // Headless hosts (e.g. the eval CLI) have no controlling TTY, so PTY
        // setup fails with `ENOTTY`. Run the command non-interactively and
        // without a PTY in that case.
        let headless = HeadlessTerminal::is_enabled(cx);

        let terminal_id = acp::TerminalId::new(Uuid::new_v4().to_string());
        let terminal_task = cx.spawn({
            let terminal_id = terminal_id.clone();
            async move |_this, cx| {
                let env = env.await;
                let shell = project
                    .update(cx, |project, cx| {
                        project
                            .remote_client()
                            .and_then(|r| r.read(cx).default_system_shell())
                    })
                    .unwrap_or_else(|| get_default_system_shell_preferring_bash());

                // The sandbox owns the network proxy (for restricted-network
                // policies) and injects the child's proxy env vars, returning
                // the env to spawn with. On Windows, restricted host access is
                // rejected inside the sandbox before command preparation.
                #[cfg(target_os = "windows")]
                let (task_command, task_args, task_env, sandbox, spawn_cwd) =
                    if sandbox_wrap.is_some() {
                        let (task_command, task_args) = task::ShellBuilder::new(
                            &Shell::Program("/bin/sh".to_string()),
                            false,
                        )
                        .non_interactive()
                        .redirect_stdin_to_dev_null()
                        .build(Some(command.clone()), &args);
                        let wrap = cx.background_spawn(prepare_sandbox_wrap(
                            task_command,
                            task_args,
                            cwd.clone(),
                            sandbox_wrap,
                            env,
                        ));
                        let timeout = cx.background_executor().timer(WSL_SANDBOX_WRAP_TIMEOUT);
                        let (task_command, task_args, task_env, sandbox) = futures::select_biased! {
                            result = wrap.fuse() => result?,
                            _ = timeout.fuse() => return Err(anyhow::Error::new(
                                sandbox::SandboxError::WslUnavailable(format!(
                                    "WSL did not respond within {} seconds while preparing the sandboxed command",
                                    WSL_SANDBOX_WRAP_TIMEOUT.as_secs()
                                )),
                            )),
                        };
                        (task_command, task_args, task_env, sandbox, None)
                    } else {
                        // No sandbox wrap means we're running unsandboxed, and
                        // on Windows that deliberately changes the shell: the
                        // sandboxed path runs under WSL's Linux bash, but this
                        // fallback uses the host's `shell` against the native cwd.
                        let mut builder = ShellBuilder::new(&Shell::Program(shell), is_windows);
                        if headless {
                            builder = builder.non_interactive();
                        }
                        let (task_command, task_args) = builder
                            .redirect_stdin_to_dev_null()
                            .build(Some(command.clone()), &args);
                        (task_command, task_args, env, None, cwd.clone())
                    };

                #[cfg(not(target_os = "windows"))]
                let (task_command, task_args, task_env, sandbox, spawn_cwd) = {
                    let mut builder = ShellBuilder::new(&Shell::Program(shell), is_windows);
                    if headless {
                        builder = builder.non_interactive();
                    }
                    let (task_command, task_args) = builder
                        .redirect_stdin_to_dev_null()
                        .build(Some(command.clone()), &args);
                    let (task_command, task_args, task_env, sandbox) = prepare_sandbox_wrap(
                        task_command,
                        task_args,
                        cwd.clone(),
                        sandbox_wrap,
                        env,
                    )
                    .await?;
                    (task_command, task_args, task_env, sandbox, cwd.clone())
                };
                let terminal = project
                    .update(cx, |project, cx| {
                        project.create_terminal_task(
                            task::SpawnInTerminal {
                                command: Some(task_command),
                                args: task_args,
                                cwd: spawn_cwd,
                                env: task_env,
                                ..Default::default()
                            },
                            cx,
                        )
                    })
                    .await?;

                anyhow::Ok(cx.new(|cx| {
                    Terminal::new(
                        terminal_id,
                        &format!("{} {}", command, args.join(" ")),
                        cwd,
                        output_byte_limit.map(|l| l as usize),
                        terminal,
                        language_registry,
                        sandbox,
                        cx,
                    )
                }))
            }
        });

        cx.spawn(async move |this, cx| {
            let terminal = terminal_task.await?;
            this.update(cx, |this, _cx| {
                this.terminals.insert(terminal_id, terminal.clone());
                terminal
            })
        })
    }

    pub fn kill_terminal(
        &mut self,
        terminal_id: acp::TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.terminals
            .get(&terminal_id)
            .context("Terminal not found")?
            .update(cx, |terminal, cx| {
                terminal.kill(cx);
            });

        Ok(())
    }

    pub fn release_terminal(
        &mut self,
        terminal_id: acp::TerminalId,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        self.terminals
            .remove(&terminal_id)
            .context("Terminal not found")?
            .update(cx, |terminal, cx| {
                terminal.kill(cx);
            });

        Ok(())
    }

    pub fn terminal(&self, terminal_id: acp::TerminalId) -> Result<Entity<Terminal>> {
        self.terminals
            .get(&terminal_id)
            .context("Terminal not found")
            .cloned()
    }

    pub fn to_markdown(&self, cx: &App) -> String {
        self.entries.iter().map(|e| e.to_markdown(cx)).collect()
    }

    pub fn emit_load_error(&mut self, error: LoadError, cx: &mut Context<Self>) {
        cx.emit(AcpThreadEvent::LoadError(error));
    }

    pub fn register_terminal_created(
        &mut self,
        terminal_id: acp::TerminalId,
        command_label: String,
        working_dir: Option<PathBuf>,
        output_byte_limit: Option<u64>,
        terminal: Entity<::terminal::Terminal>,
        cx: &mut Context<Self>,
    ) -> Entity<Terminal> {
        let language_registry = self.project.read(cx).languages().clone();

        let entity = cx.new(|cx| {
            Terminal::new(
                terminal_id.clone(),
                &command_label,
                working_dir.clone(),
                output_byte_limit.map(|l| l as usize),
                terminal,
                language_registry,
                // External terminal providers manage their own sandboxing
                // (if any). We don't wrap their commands.
                None,
                cx,
            )
        });
        self.terminals.insert(terminal_id.clone(), entity.clone());
        entity
    }

    pub fn mark_as_subagent_output(&mut self, cx: &mut Context<Self>) {
        for entry in self.entries.iter_mut().rev() {
            if let AgentThreadEntry::AssistantMessage(assistant_message) = entry {
                assistant_message.is_subagent_output = true;
                cx.notify();
                return;
            }
        }
    }

    pub fn on_terminal_provider_event(
        &mut self,
        event: TerminalProviderEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            TerminalProviderEvent::Created {
                terminal_id,
                label,
                cwd,
                output_byte_limit,
                terminal,
            } => {
                let entity = self.register_terminal_created(
                    terminal_id.clone(),
                    label,
                    cwd,
                    output_byte_limit,
                    terminal,
                    cx,
                );

                if let Some(mut chunks) = self.pending_terminal_output.remove(&terminal_id) {
                    for data in chunks.drain(..) {
                        entity.update(cx, |term, cx| {
                            term.inner().update(cx, |inner, cx| {
                                inner.write_output(&data, cx);
                            })
                        });
                    }
                }

                if let Some(_status) = self.pending_terminal_exit.remove(&terminal_id) {
                    entity.update(cx, |_term, cx| {
                        cx.notify();
                    });
                }

                cx.notify();
            }
            TerminalProviderEvent::Output { terminal_id, data } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |term, cx| {
                        term.inner().update(cx, |inner, cx| {
                            inner.write_output(&data, cx);
                        })
                    });
                } else {
                    self.pending_terminal_output
                        .entry(terminal_id)
                        .or_default()
                        .push(data);
                }
            }
            TerminalProviderEvent::TitleChanged { terminal_id, title } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |term, cx| {
                        term.inner().update(cx, |inner, cx| {
                            inner.breadcrumb_text = title;
                            cx.emit(::terminal::Event::BreadcrumbsChanged);
                        })
                    });
                }
            }
            TerminalProviderEvent::Exit {
                terminal_id,
                status,
            } => {
                if let Some(entity) = self.terminals.get(&terminal_id) {
                    entity.update(cx, |_term, cx| {
                        cx.notify();
                    });
                } else {
                    self.pending_terminal_exit.insert(terminal_id, status);
                }
            }
        }
    }
}

fn markdown_for_raw_output(
    raw_output: &serde_json::Value,
    language_registry: &Arc<LanguageRegistry>,
    cx: &mut App,
) -> Option<Entity<Markdown>> {
    match raw_output {
        serde_json::Value::Null => None,
        serde_json::Value::Bool(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.to_string().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        serde_json::Value::Number(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.to_string().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        serde_json::Value::String(value) => Some(cx.new(|cx| {
            Markdown::new(
                value.clone().into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
        value => Some(cx.new(|cx| {
            let pretty_json = to_string_pretty(value).unwrap_or_else(|_| value.to_string());

            Markdown::new(
                format!("```json\n{}\n```", pretty_json).into(),
                Some(language_registry.clone()),
                None,
                cx,
            )
        })),
    }
}

#[cfg(test)]
mod tests {
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
    fn text_resource_markdown_uses_mime_type_for_code_blocks() {
        let shell = acp::TextResourceContents::new("echo 'hello from exec test'", "tool://preview")
            .mime_type("text/x-shellscript".to_string());
        assert_eq!(
            ContentBlock::text_resource_markdown(&shell),
            "```sh\necho 'hello from exec test'\n```"
        );

        let markdown = acp::TextResourceContents::new("**approval** requested", "tool://preview")
            .mime_type("text/markdown".to_string());
        assert_eq!(
            ContentBlock::text_resource_markdown(&markdown),
            "**approval** requested"
        );

        let plain = acp::TextResourceContents::new("plain preview", "tool://preview")
            .mime_type("text/plain".to_string());
        assert_eq!(
            ContentBlock::text_resource_markdown(&plain),
            "```\nplain preview\n```"
        );

        let cpp = acp::TextResourceContents::new("int main() {}", "tool://preview")
            .mime_type("text/x-c++; charset=utf-8".to_string());
        assert_eq!(
            ContentBlock::text_resource_markdown(&cpp),
            "```cpp\nint main() {}\n```"
        );

        let untyped = acp::TextResourceContents::new("# plain preview", "tool://preview");
        assert_eq!(
            ContentBlock::text_resource_markdown(&untyped),
            "```\n# plain preview\n```"
        );
    }

    #[gpui::test]
    async fn test_tool_call_content_preserves_embedded_text_resource(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        cx.update(|cx| {
            let language_registry =
                Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
            let content = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::TextResourceContents(
                    acp::TextResourceContents::new("echo 'hello from exec test'", "tool://preview")
                        .mime_type("text/x-shellscript".to_string()),
                ),
            ));

            let block = ContentBlock::new_tool_call_content(
                content,
                &language_registry,
                PathStyle::local(),
                cx,
            );

            let ContentBlock::EmbeddedResource { resource, markdown } = &block else {
                panic!("expected embedded resource block, got {block:?}");
            };
            match &resource.resource {
                acp::EmbeddedResourceResource::TextResourceContents(text) => {
                    assert_eq!(text.text, "echo 'hello from exec test'");
                    assert_eq!(text.uri, "tool://preview");
                    assert_eq!(text.mime_type.as_deref(), Some("text/x-shellscript"));
                }
                other => panic!("expected text resource contents, got {other:?}"),
            }

            let markdown = markdown
                .as_ref()
                .expect("text resources should have renderable markdown")
                .read(cx)
                .source()
                .to_string();
            assert_eq!(markdown, "```sh\necho 'hello from exec test'\n```");
            assert_eq!(
                block.to_markdown(cx),
                "```sh\necho 'hello from exec test'\n```"
            );
            assert_eq!(block.text_content(cx), Some("echo 'hello from exec test'"));

            let untyped = ContentBlock::new_tool_call_content(
                acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                    acp::EmbeddedResourceResource::TextResourceContents(
                        acp::TextResourceContents::new("# plain preview", "tool://preview"),
                    ),
                )),
                &language_registry,
                PathStyle::local(),
                cx,
            );
            assert_eq!(untyped.to_markdown(cx), "```\n# plain preview\n```");
            assert_eq!(untyped.text_content(cx), Some("# plain preview"));
        });
    }

    #[gpui::test]
    async fn test_tool_call_content_renders_embedded_image_blob_resource(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        cx.update(|cx| {
            let language_registry =
                Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
            let image_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::BlobResourceContents(
                    acp::BlobResourceContents::new(
                        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
                        "tool://preview.png",
                    )
                    .mime_type("image/png".to_string()),
                ),
            ));

            let block = ContentBlock::new_tool_call_content(
                image_blob,
                &language_registry,
                PathStyle::local(),
                cx,
            );

            let ContentBlock::Image { image, dimensions } = &block else {
                panic!("expected image block, got {block:?}");
            };
            assert_eq!(image.format(), gpui::ImageFormat::Png);
            assert_eq!(
                dimensions.as_ref().map(|size| (size.width, size.height)),
                Some((1, 1))
            );
            assert_eq!(block.to_markdown(cx), "`Image`");
            assert_eq!(block.text_content(cx), None);
        });
    }

    #[gpui::test]
    async fn test_tool_call_content_falls_back_for_non_image_blob_resource(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        cx.update(|cx| {
            let language_registry =
                Arc::new(LanguageRegistry::test(cx.background_executor().clone()));
            let archive_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::BlobResourceContents(
                    acp::BlobResourceContents::new("not an image", "tool://archive.bin")
                        .mime_type("application/octet-stream".to_string()),
                ),
            ));

            let block = ContentBlock::new_tool_call_content(
                archive_blob,
                &language_registry,
                PathStyle::local(),
                cx,
            );

            let ContentBlock::EmbeddedResource { resource, markdown } = &block else {
                panic!("expected embedded resource block, got {block:?}");
            };
            assert!(markdown.is_none());
            match &resource.resource {
                acp::EmbeddedResourceResource::BlobResourceContents(blob) => {
                    assert_eq!(blob.uri, "tool://archive.bin");
                    assert_eq!(blob.mime_type.as_deref(), Some("application/octet-stream"));
                }
                other => panic!("expected blob resource contents, got {other:?}"),
            }
            assert_eq!(block.to_markdown(cx), "tool://archive.bin");
            assert_eq!(block.text_content(cx), None);

            let invalid_image_blob = acp::ContentBlock::Resource(acp::EmbeddedResource::new(
                acp::EmbeddedResourceResource::BlobResourceContents(
                    acp::BlobResourceContents::new("not-base64", "tool://preview.png")
                        .mime_type("image/png".to_string()),
                ),
            ));
            let invalid = ContentBlock::new_tool_call_content(
                invalid_image_blob,
                &language_registry,
                PathStyle::local(),
                cx,
            );
            let ContentBlock::EmbeddedResource { resource, markdown } = &invalid else {
                panic!("expected embedded resource block, got {invalid:?}");
            };
            assert!(markdown.is_none());
            assert_eq!(
                ContentBlock::embedded_resource_label(resource),
                "tool://preview.png"
            );
            assert_eq!(invalid.to_markdown(cx), "tool://preview.png");
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

    #[gpui::test]
    async fn test_terminal_output_buffered_before_created_renders(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(
                    project,
                    PathList::new(&[std::path::Path::new(path!("/test"))]),
                    cx,
                )
            })
            .await
            .unwrap();

        let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

        // Send Output BEFORE Created - should be buffered by acp_thread
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Output {
                    terminal_id: terminal_id.clone(),
                    data: b"hello buffered".to_vec(),
                },
                cx,
            );
        });

        // Create a display-only terminal and then send Created
        let lower = cx.new(|cx| {
            let builder = ::terminal::TerminalBuilder::new_display_only(
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            );
            builder.subscribe(cx)
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id.clone(),
                    label: "Buffered Test".to_string(),
                    cwd: None,
                    output_byte_limit: None,
                    terminal: lower.clone(),
                },
                cx,
            );
        });

        // After Created, buffered Output should have been flushed into the renderer
        let content = thread.read_with(cx, |thread, cx| {
            let term = thread.terminal(terminal_id.clone()).unwrap();
            term.read_with(cx, |t, cx| t.inner().read(cx).get_content())
        });

        assert!(
            content.contains("hello buffered"),
            "expected buffered output to render, got: {content}"
        );
    }

    #[gpui::test]
    async fn test_terminal_output_and_exit_buffered_before_created(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(
                    project,
                    PathList::new(&[std::path::Path::new(path!("/test"))]),
                    cx,
                )
            })
            .await
            .unwrap();

        let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

        // Send Output BEFORE Created
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Output {
                    terminal_id: terminal_id.clone(),
                    data: b"pre-exit data".to_vec(),
                },
                cx,
            );
        });

        // Send Exit BEFORE Created
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Exit {
                    terminal_id: terminal_id.clone(),
                    status: acp::TerminalExitStatus::new().exit_code(0),
                },
                cx,
            );
        });

        // Now create a display-only lower-level terminal and send Created
        let lower = cx.new(|cx| {
            let builder = ::terminal::TerminalBuilder::new_display_only(
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            );
            builder.subscribe(cx)
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id.clone(),
                    label: "Buffered Exit Test".to_string(),
                    cwd: None,
                    output_byte_limit: None,
                    terminal: lower.clone(),
                },
                cx,
            );
        });

        // Output should be present after Created (flushed from buffer)
        let content = thread.read_with(cx, |thread, cx| {
            let term = thread.terminal(terminal_id.clone()).unwrap();
            term.read_with(cx, |t, cx| t.inner().read(cx).get_content())
        });

        assert!(
            content.contains("pre-exit data"),
            "expected pre-exit data to render, got: {content}"
        );
    }

    /// Test that killing a terminal via Terminal::kill properly:
    /// 1. Causes wait_for_exit to complete (doesn't hang forever)
    /// 2. The underlying terminal still has the output that was written before the kill
    ///
    /// This test verifies that the fix to kill_active_task (which now also kills
    /// the shell process in addition to the foreground process) properly allows
    /// wait_for_exit to complete instead of hanging indefinitely.
    #[cfg(unix)]
    #[gpui::test]
    async fn test_terminal_kill_allows_wait_for_exit_to_complete(cx: &mut gpui::TestAppContext) {
        use std::collections::HashMap;
        use task::Shell;
        use util::shell_builder::ShellBuilder;

        init_test(cx);
        cx.executor().allow_parking();

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(
                    project.clone(),
                    PathList::new(&[Path::new(path!("/test"))]),
                    cx,
                )
            })
            .await
            .unwrap();

        let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());

        // Create a real PTY terminal that runs a command which prints output then sleeps
        // We use printf instead of echo and chain with && sleep to ensure proper execution
        let (completion_tx, _completion_rx) = async_channel::unbounded();
        let (program, args) = ShellBuilder::new(&Shell::System, false).build(
            Some("printf 'output_before_kill\\n' && sleep 60".to_owned()),
            &[],
        );

        let builder = cx
            .update(|cx| {
                ::terminal::TerminalBuilder::new(
                    None,
                    None,
                    task::Shell::WithArguments {
                        program,
                        args,
                        title_override: None,
                    },
                    HashMap::default(),
                    ::terminal::terminal_settings::CursorShape::default(),
                    ::terminal::terminal_settings::AlternateScroll::On,
                    None,
                    vec![],
                    0,
                    false,
                    0,
                    Some(completion_tx),
                    cx,
                    vec![],
                    PathStyle::local(),
                )
            })
            .await
            .unwrap();

        let lower_terminal = cx.new(|cx| builder.subscribe(cx));

        // Create the acp_thread Terminal wrapper
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id.clone(),
                    label: "printf output_before_kill && sleep 60".to_string(),
                    cwd: None,
                    output_byte_limit: None,
                    terminal: lower_terminal.clone(),
                },
                cx,
            );
        });

        // Poll until the printf command produces output, rather than using a
        // fixed sleep which is flaky on loaded machines.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let has_output = thread.read_with(cx, |thread, cx| {
                let term = thread
                    .terminals
                    .get(&terminal_id)
                    .expect("terminal not found");
                let content = term.read(cx).inner().read(cx).get_content();
                content.contains("output_before_kill")
            });
            if has_output {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Timed out waiting for printf output to appear in terminal",
            );
            cx.executor().timer(Duration::from_millis(50)).await;
        }

        // Get the acp_thread Terminal and kill it
        let wait_for_exit = thread.update(cx, |thread, cx| {
            let term = thread.terminals.get(&terminal_id).unwrap();
            let wait_for_exit = term.read(cx).wait_for_exit();
            term.update(cx, |term, cx| {
                term.kill(cx);
            });
            wait_for_exit
        });

        // KEY ASSERTION: wait_for_exit should complete within a reasonable time (not hang).
        // Before the fix to kill_active_task, this would hang forever because
        // only the foreground process was killed, not the shell, so the PTY
        // child never exited and wait_for_completed_task never completed.
        let exit_result = futures::select! {
            result = futures::FutureExt::fuse(wait_for_exit) => Some(result),
            _ = futures::FutureExt::fuse(cx.background_executor.timer(Duration::from_secs(5))) => None,
        };

        assert!(
            exit_result.is_some(),
            "wait_for_exit should complete after kill, but it timed out. \
            This indicates kill_active_task is not properly killing the shell process."
        );

        // Give the system a chance to process any pending updates
        cx.run_until_parked();

        // Verify that the underlying terminal still has the output that was
        // written before the kill. This verifies that killing doesn't lose output.
        let inner_content = thread.read_with(cx, |thread, cx| {
            let term = thread.terminals.get(&terminal_id).unwrap();
            term.read(cx).inner().read(cx).get_content()
        });

        assert!(
            inner_content.contains("output_before_kill"),
            "Underlying terminal should contain output from before kill, got: {}",
            inner_content
        );
    }

    #[gpui::test]
    async fn test_push_user_content_block(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Test creating a new user message
        thread.update(cx, |thread, cx| {
            thread.push_user_content_block(None, "Hello, ".into(), cx);
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 1);
            if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[0] {
                assert_eq!(user_msg.protocol_id, None);
                assert_eq!(user_msg.client_id, None);
                assert_eq!(user_msg.content.to_markdown(cx), "Hello, ");
            } else {
                panic!("Expected UserMessage");
            }
        });

        // Test appending to existing user message
        let message_1_id = ClientUserMessageId::new();
        thread.update(cx, |thread, cx| {
            thread.push_user_content_block(Some(message_1_id.clone()), "world!".into(), cx);
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 1);
            if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[0] {
                assert_eq!(user_msg.protocol_id, None);
                assert_eq!(user_msg.client_id, Some(message_1_id));
                assert_eq!(user_msg.content.to_markdown(cx), "Hello, world!");
            } else {
                panic!("Expected UserMessage");
            }
        });

        // Test creating new user message after assistant message
        thread.update(cx, |thread, cx| {
            thread.push_assistant_content_block("Assistant response".into(), false, cx);
        });

        let message_2_id = ClientUserMessageId::new();
        thread.update(cx, |thread, cx| {
            thread.push_user_content_block(
                Some(message_2_id.clone()),
                "New user message".into(),
                cx,
            );
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 3);
            if let AgentThreadEntry::UserMessage(user_msg) = &thread.entries[2] {
                assert_eq!(user_msg.protocol_id, None);
                assert_eq!(user_msg.client_id, Some(message_2_id));
                assert_eq!(user_msg.content.to_markdown(cx), "New user message");
            } else {
                panic!("Expected UserMessage at index 2");
            }
        });
    }

    #[gpui::test]
    async fn test_user_message_chunks_use_protocol_message_id_boundaries(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("First ".into()).message_id("msg_user_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("message".into()).message_id("msg_user_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("Second message".into()).message_id("msg_user_2"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("Echo".into()).message_id("msg_user_3"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("Echo".into()).message_id("msg_user_3"),
                    ),
                    cx,
                )
                .unwrap();
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 3);

            let AgentThreadEntry::UserMessage(first_message) = &thread.entries[0] else {
                panic!("expected first entry to be a user message")
            };
            assert_eq!(first_message.content.to_markdown(cx), "First message");
            assert_eq!(
                first_message
                    .protocol_id
                    .as_ref()
                    .map(ToString::to_string)
                    .as_deref(),
                Some("msg_user_1")
            );

            let AgentThreadEntry::UserMessage(second_message) = &thread.entries[1] else {
                panic!("expected second entry to be a user message")
            };
            assert_eq!(second_message.content.to_markdown(cx), "Second message");
            assert_eq!(
                second_message
                    .protocol_id
                    .as_ref()
                    .map(ToString::to_string)
                    .as_deref(),
                Some("msg_user_2")
            );

            let AgentThreadEntry::UserMessage(third_message) = &thread.entries[2] else {
                panic!("expected third entry to be a user message")
            };
            assert_eq!(third_message.content.to_markdown(cx), "EchoEcho");
            assert_eq!(
                third_message
                    .protocol_id
                    .as_ref()
                    .map(ToString::to_string)
                    .as_deref(),
                Some("msg_user_3")
            );
        });
    }

    #[gpui::test]
    async fn test_protocol_user_chunk_does_not_merge_into_optimistic_prompt(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread.push_user_content_block_with_protocol_id(
                None,
                true,
                None,
                "Typed prompt".into(),
                false,
                cx,
            );
            thread
                .handle_session_update(
                    acp::SessionUpdate::UserMessageChunk(
                        acp::ContentChunk::new("Agent user chunk".into())
                            .message_id("agent_user_chunk"),
                    ),
                    cx,
                )
                .unwrap();
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 2);

            let AgentThreadEntry::UserMessage(optimistic_message) = &thread.entries[0] else {
                panic!("expected first entry to be optimistic user message")
            };
            assert!(optimistic_message.is_optimistic);
            assert_eq!(optimistic_message.content.to_markdown(cx), "Typed prompt");
            assert!(optimistic_message.protocol_id.is_none());
            assert!(optimistic_message.client_id.is_none());

            let AgentThreadEntry::UserMessage(agent_message) = &thread.entries[1] else {
                panic!("expected second entry to be protocol user chunk")
            };
            assert!(!agent_message.is_optimistic);
            assert_eq!(agent_message.content.to_markdown(cx), "Agent user chunk");
            assert_eq!(
                agent_message
                    .protocol_id
                    .as_ref()
                    .map(ToString::to_string)
                    .as_deref(),
                Some("agent_user_chunk")
            );
        });
    }

    #[gpui::test]
    async fn test_assistant_chunks_use_protocol_message_id_boundaries(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentThoughtChunk(
                        acp::ContentChunk::new("Thinking ".into()).message_id("msg_thought_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentThoughtChunk(
                        acp::ContentChunk::new("hard".into()).message_id("msg_thought_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentThoughtChunk(
                        acp::ContentChunk::new("A separate thought".into())
                            .message_id("msg_thought_2"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentMessageChunk(
                        acp::ContentChunk::new("Answer ".into()).message_id("msg_agent_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentMessageChunk(
                        acp::ContentChunk::new("done".into()).message_id("msg_agent_1"),
                    ),
                    cx,
                )
                .unwrap();
            thread
                .handle_session_update(
                    acp::SessionUpdate::AgentMessageChunk(
                        acp::ContentChunk::new("Follow-up".into()).message_id("msg_agent_2"),
                    ),
                    cx,
                )
                .unwrap();
        });

        thread.update(cx, |thread, cx| {
            assert_eq!(thread.entries.len(), 1);
            let AgentThreadEntry::AssistantMessage(message) = &thread.entries[0] else {
                panic!("expected assistant entry")
            };
            assert_eq!(message.chunks.len(), 4);

            let AssistantMessageChunk::Thought { id, block } = &message.chunks[0] else {
                panic!("expected first chunk to be a thought")
            };
            assert_eq!(block.to_markdown(cx), "Thinking hard");
            assert_eq!(
                id.as_ref().map(ToString::to_string).as_deref(),
                Some("msg_thought_1")
            );

            let AssistantMessageChunk::Thought { id, block } = &message.chunks[1] else {
                panic!("expected second chunk to be a thought")
            };
            assert_eq!(block.to_markdown(cx), "A separate thought");
            assert_eq!(
                id.as_ref().map(ToString::to_string).as_deref(),
                Some("msg_thought_2")
            );

            let AssistantMessageChunk::Message { id, block } = &message.chunks[2] else {
                panic!("expected third chunk to be a message")
            };
            assert_eq!(block.to_markdown(cx), "Answer done");
            assert_eq!(
                id.as_ref().map(ToString::to_string).as_deref(),
                Some("msg_agent_1")
            );

            let AssistantMessageChunk::Message { id, block } = &message.chunks[3] else {
                panic!("expected fourth chunk to be a message")
            };
            assert_eq!(block.to_markdown(cx), "Follow-up");
            assert_eq!(
                id.as_ref().map(ToString::to_string).as_deref(),
                Some("msg_agent_2")
            );
        });
    }

    #[gpui::test]
    async fn test_thinking_concatenation(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            |_, thread, mut cx| {
                async move {
                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
                                    "Thinking ".into(),
                                )),
                                cx,
                            )
                            .unwrap();
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::AgentThoughtChunk(acp::ContentChunk::new(
                                    "hard!".into(),
                                )),
                                cx,
                            )
                            .unwrap();
                    })?;
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            },
        ));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread
            .update(cx, |thread, cx| thread.send_raw("Hello from Mav!", cx))
            .await
            .unwrap();

        let output = thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
        assert_eq!(
            output,
            indoc! {r#"
            ## User

            Hello from Mav!

            ## Assistant

            <thinking>
            Thinking hard!
            </thinking>

            "#}
        );
    }

    /// `send_command` runs the turn (the connection receives the typed command)
    /// but never echoes a user-message bubble, so commands like `/compact` don't
    /// show a fake user message implying the text was sent to the model.
    #[gpui::test]
    async fn test_send_command_does_not_echo_user_message(cx: &mut gpui::TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let received_prompt: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let received_prompt = received_prompt.clone();
            move |request, thread, mut cx| {
                let received_prompt = received_prompt.clone();
                async move {
                    if let Some(acp::ContentBlock::Text(text)) = request.prompt.first() {
                        *received_prompt.borrow_mut() = Some(text.text.clone());
                    }
                    // Simulate a native command producing its own thread entry
                    // (here a compaction) rather than echoing a user message.
                    thread.update(&mut cx, |thread, cx| {
                        thread.push_context_compaction(
                            ContextCompaction {
                                id: ContextCompactionId("c1".into()),
                                status: ContextCompactionStatus::Completed,
                                summary: None,
                            },
                            cx,
                        );
                    })?;
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        cx.update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.send_command(vec!["/compact".into()], cx)
            })
        })
        .await
        .unwrap();

        // The command turn ran: the connection received the typed command.
        assert_eq!(received_prompt.borrow().as_deref(), Some("/compact"));

        thread.update(cx, |thread, _cx| {
            assert!(
                !thread
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, AgentThreadEntry::UserMessage(_))),
                "send_command must not echo a user message"
            );
            // The command's own entry (here a compaction) is still shown.
            assert!(
                thread
                    .entries
                    .iter()
                    .any(|entry| matches!(entry, AgentThreadEntry::ContextCompaction(_))),
                "the command's own thread entry should still be present"
            );
        });
    }

    #[gpui::test]
    async fn test_ignore_echoed_user_message_chunks_during_active_turn(
        cx: &mut gpui::TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(
            FakeAgentConnection::new()
                .without_truncate_support()
                .on_user_message(|request, thread, mut cx| {
                    async move {
                        let prompt = request.prompt.first().cloned().unwrap_or_else(|| "".into());

                        thread.update(&mut cx, |thread, cx| {
                            thread
                                .handle_session_update(
                                    acp::SessionUpdate::UserMessageChunk(acp::ContentChunk::new(
                                        prompt,
                                    )),
                                    cx,
                                )
                                .unwrap();
                        })?;

                        Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                    }
                    .boxed_local()
                }),
        );

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread
            .update(cx, |thread, cx| thread.send_raw("Hello from Mav!", cx))
            .await
            .unwrap();

        let output = thread.read_with(cx, |thread, cx| thread.to_markdown(cx));
        assert_eq!(output.matches("Hello from Mav!").count(), 1);
        thread.read_with(cx, |thread, _cx| {
            let Some(AgentThreadEntry::UserMessage(message)) = thread.entries.first() else {
                panic!("expected optimistic user message");
            };
            assert_eq!(message.protocol_id, None);
            assert_eq!(message.client_id, None);
            assert!(message.is_optimistic);
        });
    }

    #[gpui::test]
    async fn test_edits_concurrently_to_user(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/tmp"), json!({"foo": "one\ntwo\nthree\n"}))
            .await;
        let project = Project::test(fs.clone(), [], cx).await;
        let (read_file_tx, read_file_rx) = oneshot::channel::<()>();
        let read_file_tx = Rc::new(RefCell::new(Some(read_file_tx)));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            move |_, thread, mut cx| {
                let read_file_tx = read_file_tx.clone();
                async move {
                    let content = thread
                        .update(&mut cx, |thread, cx| {
                            thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
                        })
                        .unwrap()
                        .await
                        .unwrap();
                    assert_eq!(content, "one\ntwo\nthree\n");
                    read_file_tx.take().unwrap().send(()).unwrap();
                    thread
                        .update(&mut cx, |thread, cx| {
                            thread.write_text_file(
                                path!("/tmp/foo").into(),
                                "one\ntwo\nthree\nfour\nfive\n".to_string(),
                                cx,
                            )
                        })
                        .unwrap()
                        .await
                        .unwrap();
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            },
        ));

        let (worktree, pathbuf) = project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
            })
            .await
            .unwrap();
        let buffer = project
            .update(cx, |project, cx| {
                project.open_buffer((worktree.read(cx).id(), pathbuf), cx)
            })
            .await
            .unwrap();

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
            })
            .await
            .unwrap();

        let request = thread.update(cx, |thread, cx| {
            thread.send_raw("Extend the count in /tmp/foo", cx)
        });
        read_file_rx.await.ok();
        buffer.update(cx, |buffer, cx| {
            buffer.edit([(0..0, "zero\n".to_string())], None, cx);
        });
        cx.run_until_parked();
        assert_eq!(
            buffer.read_with(cx, |buffer, _| buffer.text()),
            "zero\none\ntwo\nthree\nfour\nfive\n"
        );
        assert_eq!(
            String::from_utf8(fs.read_file_sync(path!("/tmp/foo")).unwrap()).unwrap(),
            "zero\none\ntwo\nthree\nfour\nfive\n"
        );
        request.await.unwrap();
    }

    #[gpui::test]
    async fn test_reading_from_line(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/tmp"), json!({"foo": "one\ntwo\nthree\nfour\n"}))
            .await;
        let project = Project::test(fs.clone(), [], cx).await;
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
            })
            .await
            .unwrap();

        let connection = Rc::new(FakeAgentConnection::new());

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
            })
            .await
            .unwrap();

        // Whole file
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "one\ntwo\nthree\nfour\n");

        // Only start line
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(3), None, false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "three\nfour\n");

        // Only limit
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), None, Some(2), false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "one\ntwo\n");

        // Range
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(2), Some(2), false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "two\nthree\n");

        // Invalid
        let err = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(6), Some(2), false, cx)
            })
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "Invalid params: \"Attempting to read beyond the end of the file, line 5:0\""
        );
    }

    #[gpui::test]
    async fn test_reading_empty_file(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/tmp"), json!({"foo": ""})).await;
        let project = Project::test(fs.clone(), [], cx).await;
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/tmp/foo"), true, cx)
            })
            .await
            .unwrap();

        let connection = Rc::new(FakeAgentConnection::new());

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
            })
            .await
            .unwrap();

        // Whole file
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), None, None, false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "");

        // Only start line
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(1), None, false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "");

        // Only limit
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), None, Some(2), false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "");

        // Range
        let content = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(1), Some(1), false, cx)
            })
            .await
            .unwrap();

        assert_eq!(content, "");

        // Invalid
        let err = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/tmp/foo").into(), Some(5), Some(2), false, cx)
            })
            .await
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            "Invalid params: \"Attempting to read beyond the end of the file, line 1:0\""
        );
    }
    #[gpui::test]
    async fn test_reading_non_existing_file(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/tmp"), json!({})).await;
        let project = Project::test(fs.clone(), [], cx).await;
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(path!("/tmp"), true, cx)
            })
            .await
            .unwrap();

        let connection = Rc::new(FakeAgentConnection::new());

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/tmp"))]), cx)
            })
            .await
            .unwrap();

        // Out of project file
        let err = thread
            .update(cx, |thread, cx| {
                thread.read_text_file(path!("/foo").into(), None, None, false, cx)
            })
            .await
            .unwrap_err();

        assert_eq!(err.code, acp::ErrorCode::ResourceNotFound);
    }

    #[gpui::test]
    async fn test_succeeding_canceled_toolcall(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let id = acp::ToolCallId::new("test");

        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let id = id.clone();
            move |_, thread, mut cx| {
                let id = id.clone();
                async move {
                    thread
                        .update(&mut cx, |thread, cx| {
                            thread.handle_session_update(
                                acp::SessionUpdate::ToolCall(
                                    acp::ToolCall::new(id.clone(), "Label")
                                        .kind(acp::ToolKind::Fetch)
                                        .status(acp::ToolCallStatus::InProgress),
                                ),
                                cx,
                            )
                        })
                        .unwrap()
                        .unwrap();
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let request = thread.update(cx, |thread, cx| {
            thread.send_raw("Fetch https://example.com", cx)
        });

        run_until_first_tool_call(&thread, cx).await;

        thread.read_with(cx, |thread, _| {
            assert!(matches!(
                thread.entries[1],
                AgentThreadEntry::ToolCall(ToolCall {
                    status: ToolCallStatus::InProgress,
                    ..
                })
            ));
        });

        thread.update(cx, |thread, cx| thread.cancel(cx)).await;

        thread.read_with(cx, |thread, _| {
            assert!(matches!(
                &thread.entries[1],
                AgentThreadEntry::ToolCall(ToolCall {
                    status: ToolCallStatus::Canceled,
                    ..
                })
            ));
        });

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                        id,
                        acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                    )),
                    cx,
                )
            })
            .unwrap();

        request.await.unwrap();

        thread.read_with(cx, |thread, _| {
            assert!(matches!(
                thread.entries[1],
                AgentThreadEntry::ToolCall(ToolCall {
                    status: ToolCallStatus::Completed,
                    ..
                })
            ));
        });
    }

    #[gpui::test]
    async fn test_tool_call_location_resolves_external_file(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(
            path!("/tmp/skills/test-skill"),
            json!({ "SKILL.md": "skill body" }),
        )
        .await;
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/project"))]), cx)
            })
            .await
            .unwrap();

        let skill_path = std::path::PathBuf::from(path!("/tmp/skills/test-skill/SKILL.md"));
        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCall(
                        acp::ToolCall::new("write_file", "Write SKILL.md")
                            .kind(acp::ToolKind::Edit)
                            .status(acp::ToolCallStatus::Completed)
                            .locations(vec![acp::ToolCallLocation::new(skill_path.clone())]),
                    ),
                    cx,
                )
            })
            .unwrap();

        cx.run_until_parked();

        thread.read_with(cx, |thread, cx| {
            let (tool_call_location, agent_location) = thread.entries[0]
                .location(0)
                .expect("external tool-call location should resolve");
            assert_eq!(tool_call_location.path, skill_path);

            let buffer = agent_location
                .buffer
                .upgrade()
                .expect("resolved location should keep an open buffer");
            assert_eq!(buffer.read(cx).text(), "skill body");
        });
    }

    #[gpui::test]
    async fn test_duplicate_tool_call_update_preserves_open_permission_request_until_authorized(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let tool_call_id = acp::ToolCallId::new("toolu_01duplicate");
        let allow_option_id = acp::PermissionOptionId::new("allow");
        let permission_task = thread
            .update(cx, |thread, cx| {
                thread.request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), "Original title")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .content(vec!["original content".into()])
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        allow_option_id.clone(),
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    AuthorizationKind::PermissionGrant,
                    cx,
                )
            })
            .unwrap();

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCall(
                        acp::ToolCall::new(tool_call_id.clone(), "Updated title")
                            .kind(acp::ToolKind::Execute)
                            .status(acp::ToolCallStatus::Pending)
                            .content(vec!["updated content".into()]),
                    ),
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert_eq!(tool_call.label.read(cx).source(), "Updated title");
            assert!(matches!(
                tool_call.status,
                ToolCallStatus::WaitingForConfirmation { .. }
            ));
            assert_eq!(tool_call.content.len(), 1);
            assert_eq!(tool_call.content[0].to_markdown(cx), "updated content");
        });

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                        tool_call_id.clone(),
                        acp::ToolCallUpdateFields::new()
                            .status(acp::ToolCallStatus::InProgress)
                            .title("Updated again")
                            .content(vec!["updated again".into()]),
                    )),
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert_eq!(tool_call.label.read(cx).source(), "Updated again");
            assert!(matches!(
                tool_call.status,
                ToolCallStatus::WaitingForConfirmation { .. }
            ));
            assert_eq!(tool_call.content.len(), 1);
            assert_eq!(tool_call.content[0].to_markdown(cx), "updated again");
        });

        let selected_outcome = SelectedPermissionOutcome::new(
            allow_option_id.clone(),
            acp::PermissionOptionKind::AllowOnce,
        );
        thread.update(cx, |thread, cx| {
            thread.authorize_tool_call(tool_call_id.clone(), selected_outcome, cx);
        });

        thread.read_with(cx, |thread, _cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert!(matches!(tool_call.status, ToolCallStatus::InProgress));
        });

        match permission_task.await {
            RequestPermissionOutcome::Selected(outcome) => {
                assert_eq!(outcome.option_id, allow_option_id);
                assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
            }
            RequestPermissionOutcome::Cancelled => {
                panic!("permission request should remain open after duplicate tool call update")
            }
        }

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                        tool_call_id.clone(),
                        acp::ToolCallUpdateFields::new()
                            .status(acp::ToolCallStatus::Completed)
                            .title("Completed")
                            .content(vec!["done".into()]),
                    )),
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert_eq!(tool_call.label.read(cx).source(), "Completed");
            assert!(matches!(tool_call.status, ToolCallStatus::Completed));
            assert_eq!(tool_call.content.len(), 1);
            assert_eq!(tool_call.content[0].to_markdown(cx), "done");
        });
    }

    #[gpui::test]
    async fn test_permission_request_tracks_agent_status_until_resolved(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let tool_call_id = acp::ToolCallId::new("toolu_01auto_resolve");
        let permission_task = thread
            .update(cx, |thread, cx| {
                thread.request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), "Original title")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        acp::PermissionOptionId::new("allow"),
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    AuthorizationKind::PermissionGrant,
                    cx,
                )
            })
            .unwrap();

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                        tool_call_id.clone(),
                        acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::InProgress),
                    )),
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, _cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert!(matches!(
                tool_call.status,
                ToolCallStatus::WaitingForConfirmation {
                    current_status: acp::ToolCallStatus::InProgress,
                    ..
                }
            ));
        });

        thread.update(cx, |thread, cx| {
            thread.authorize_tool_call(
                tool_call_id.clone(),
                SelectedPermissionOutcome::new(
                    acp::PermissionOptionId::new("allow"),
                    acp::PermissionOptionKind::AllowOnce,
                ),
                cx,
            );
        });

        thread.read_with(cx, |thread, _cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert!(matches!(tool_call.status, ToolCallStatus::InProgress));
        });

        match permission_task.await {
            RequestPermissionOutcome::Selected(outcome) => {
                assert_eq!(outcome.option_id, acp::PermissionOptionId::new("allow"));
                assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
            }
            RequestPermissionOutcome::Cancelled => {
                panic!("resolved permission request should select an outcome")
            }
        }
    }

    #[gpui::test]
    async fn test_permission_request_sets_waiting_status_on_existing_tool_call(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let tool_call_id = acp::ToolCallId::new("toolu_01existing_permission");
        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCall(
                        acp::ToolCall::new(tool_call_id.clone(), "Running title")
                            .kind(acp::ToolKind::Execute)
                            .status(acp::ToolCallStatus::InProgress),
                    ),
                    cx,
                )
            })
            .unwrap();

        let permission_task = thread
            .update(cx, |thread, cx| {
                thread.request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        acp::PermissionOptionId::new("allow"),
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    AuthorizationKind::PermissionGrant,
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert_eq!(tool_call.label.read(cx).source(), "Needs permission");
            assert!(matches!(
                tool_call.status,
                ToolCallStatus::WaitingForConfirmation {
                    current_status: acp::ToolCallStatus::InProgress,
                    ..
                }
            ));
        });

        thread.update(cx, |thread, cx| {
            thread.authorize_tool_call(
                tool_call_id.clone(),
                SelectedPermissionOutcome::new(
                    acp::PermissionOptionId::new("allow"),
                    acp::PermissionOptionKind::AllowOnce,
                ),
                cx,
            );
        });

        match permission_task.await {
            RequestPermissionOutcome::Selected(outcome) => {
                assert_eq!(outcome.option_id, acp::PermissionOptionId::new("allow"));
                assert_eq!(outcome.option_kind, acp::PermissionOptionKind::AllowOnce);
            }
            RequestPermissionOutcome::Cancelled => {
                panic!("permission request should resolve after authorization")
            }
        }
    }

    #[gpui::test]
    async fn test_cancel_tool_call_authorization_resolves_permission_request(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let tool_call_id = acp::ToolCallId::new("toolu_01cancelled_permission");
        let permission_task = thread
            .update(cx, |thread, cx| {
                thread.request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        acp::PermissionOptionId::new("allow"),
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    AuthorizationKind::PermissionGrant,
                    cx,
                )
            })
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread.cancel_tool_call_authorization(&tool_call_id, cx);
        });

        thread.read_with(cx, |thread, _cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert!(matches!(tool_call.status, ToolCallStatus::Canceled));
        });

        match permission_task.await {
            RequestPermissionOutcome::Cancelled => {}
            RequestPermissionOutcome::Selected(_) => {
                panic!("cancelled permission request should not select an outcome")
            }
        }
    }

    #[gpui::test]
    async fn test_terminal_tool_call_update_closes_open_permission_request(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let tool_call_id = acp::ToolCallId::new("toolu_01completed_while_waiting");
        let permission_task = thread
            .update(cx, |thread, cx| {
                thread.request_tool_call_authorization(
                    acp::ToolCall::new(tool_call_id.clone(), "Needs permission")
                        .kind(acp::ToolKind::Execute)
                        .status(acp::ToolCallStatus::Pending)
                        .into(),
                    PermissionOptions::Flat(vec![acp::PermissionOption::new(
                        acp::PermissionOptionId::new("allow"),
                        "Allow",
                        acp::PermissionOptionKind::AllowOnce,
                    )]),
                    AuthorizationKind::PermissionGrant,
                    cx,
                )
            })
            .unwrap();

        thread
            .update(cx, |thread, cx| {
                thread.handle_session_update(
                    acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                        tool_call_id.clone(),
                        acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                    )),
                    cx,
                )
            })
            .unwrap();

        thread.read_with(cx, |thread, _cx| {
            let (_, tool_call) = thread
                .tool_call(&tool_call_id)
                .expect("tool call should exist");
            assert!(matches!(tool_call.status, ToolCallStatus::Completed));
        });

        match permission_task.await {
            RequestPermissionOutcome::Cancelled => {}
            RequestPermissionOutcome::Selected(_) => {
                panic!("terminal tool call update should close pending permission request")
            }
        }
    }

    #[gpui::test]
    async fn test_no_pending_edits_if_tool_calls_are_completed(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(path!("/test"), json!({})).await;
        let project = Project::test(fs, [path!("/test").as_ref()], cx).await;

        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            move |_, thread, mut cx| {
                async move {
                    thread
                        .update(&mut cx, |thread, cx| {
                            thread.handle_session_update(
                                acp::SessionUpdate::ToolCall(
                                    acp::ToolCall::new("test", "Label")
                                        .kind(acp::ToolKind::Edit)
                                        .status(acp::ToolCallStatus::Completed)
                                        .content(vec![acp::ToolCallContent::Diff(acp::Diff::new(
                                            "/test/test.txt",
                                            "foo",
                                        ))]),
                                ),
                                cx,
                            )
                        })
                        .unwrap()
                        .unwrap();
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["Hi".into()], cx)))
            .await
            .unwrap();

        assert!(cx.read(|cx| !thread.read(cx).has_pending_edit_tool_calls()));
    }

    #[gpui::test(iterations = 10)]
    async fn test_checkpoints(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/test"),
            json!({
                ".git": {}
            }),
        )
        .await;
        let project = Project::test(fs.clone(), [path!("/test").as_ref()], cx).await;

        let simulate_changes = Arc::new(AtomicBool::new(true));
        let next_filename = Arc::new(AtomicUsize::new(0));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let simulate_changes = simulate_changes.clone();
            let next_filename = next_filename.clone();
            let fs = fs.clone();
            move |request, thread, mut cx| {
                let fs = fs.clone();
                let simulate_changes = simulate_changes.clone();
                let next_filename = next_filename.clone();
                async move {
                    if simulate_changes.load(SeqCst) {
                        let filename = format!("/test/file-{}", next_filename.fetch_add(1, SeqCst));
                        fs.write(Path::new(&filename), b"").await?;
                    }

                    let acp::ContentBlock::Text(content) = &request.prompt[0] else {
                        panic!("expected text content block");
                    };
                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                                    content.text.to_uppercase().into(),
                                )),
                                cx,
                            )
                            .unwrap();
                    })?;
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["Lorem".into()], cx)))
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User (checkpoint)

                    Lorem

                    ## Assistant

                    LOREM

                "}
            );
        });
        assert_eq!(fs.files(), vec![Path::new(path!("/test/file-0"))]);

        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["ipsum".into()], cx)))
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User (checkpoint)

                    Lorem

                    ## Assistant

                    LOREM

                    ## User (checkpoint)

                    ipsum

                    ## Assistant

                    IPSUM

                "}
            );
        });
        assert_eq!(
            fs.files(),
            vec![
                Path::new(path!("/test/file-0")),
                Path::new(path!("/test/file-1"))
            ]
        );

        // Checkpoint isn't stored when there are no changes.
        simulate_changes.store(false, SeqCst);
        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["dolor".into()], cx)))
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User (checkpoint)

                    Lorem

                    ## Assistant

                    LOREM

                    ## User (checkpoint)

                    ipsum

                    ## Assistant

                    IPSUM

                    ## User

                    dolor

                    ## Assistant

                    DOLOR

                "}
            );
        });
        assert_eq!(
            fs.files(),
            vec![
                Path::new(path!("/test/file-0")),
                Path::new(path!("/test/file-1"))
            ]
        );

        // Rewinding the conversation truncates the history and restores the checkpoint.
        thread
            .update(cx, |thread, cx| {
                let AgentThreadEntry::UserMessage(message) = &thread.entries[2] else {
                    panic!("unexpected entries {:?}", thread.entries)
                };
                thread.restore_checkpoint(message.client_id.clone().unwrap(), cx)
            })
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User (checkpoint)

                    Lorem

                    ## Assistant

                    LOREM

                "}
            );
        });
        assert_eq!(fs.files(), vec![Path::new(path!("/test/file-0"))]);
    }

    #[gpui::test(iterations = 10)]
    async fn test_checkpoint_shows_when_file_changes_during_pending_message(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(
            path!("/test"),
            json!({
                ".git": {}
            }),
        )
        .await;
        let project = Project::test(fs, [path!("/test").as_ref()], cx).await;

        let (request_started_tx, request_started_rx) = oneshot::channel::<()>();
        let request_started_tx = Rc::new(RefCell::new(Some(request_started_tx)));
        let (write_file_tx, write_file_rx) = oneshot::channel::<()>();
        let write_file_rx = Rc::new(RefCell::new(Some(write_file_rx)));
        let (file_written_tx, file_written_rx) = oneshot::channel::<()>();
        let file_written_tx = Rc::new(RefCell::new(Some(file_written_tx)));
        let (finish_response_tx, finish_response_rx) = oneshot::channel::<()>();
        let finish_response_tx = Rc::new(RefCell::new(Some(finish_response_tx)));
        let finish_response_rx = Rc::new(RefCell::new(Some(finish_response_rx)));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let request_started_tx = request_started_tx.clone();
            let write_file_rx = write_file_rx.clone();
            let file_written_tx = file_written_tx.clone();
            let finish_response_rx = finish_response_rx.clone();
            move |_request, thread, mut cx| {
                let write_file_rx = write_file_rx.borrow_mut().take();
                let finish_response_rx = finish_response_rx.borrow_mut().take();
                let request_started_tx = request_started_tx.borrow_mut().take();
                let file_written_tx = file_written_tx.borrow_mut().take();
                async move {
                    if let Some(request_started_tx) = request_started_tx {
                        request_started_tx.send(()).ok();
                    }
                    if let Some(write_file_rx) = write_file_rx {
                        write_file_rx.await.ok();
                    }

                    thread
                        .update(&mut cx, |thread, cx| {
                            thread.write_text_file(
                                PathBuf::from(path!("/test/file")),
                                String::new(),
                                cx,
                            )
                        })?
                        .await?;

                    if let Some(file_written_tx) = file_written_tx {
                        file_written_tx.send(()).ok();
                    }
                    if let Some(finish_response_rx) = finish_response_rx {
                        finish_response_rx.await.ok();
                    }

                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let send = thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx));
        let send_task = cx.background_executor.spawn(send);
        request_started_rx.await.unwrap();
        cx.run_until_parked();

        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User

                    hello

                "}
            );
        });

        write_file_tx.send(()).ok();
        file_written_rx.await.unwrap();
        cx.run_until_parked();

        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User (checkpoint)

                    hello

                "}
            );
        });

        finish_response_tx
            .borrow_mut()
            .take()
            .unwrap()
            .send(())
            .ok();
        send_task.await.unwrap();
    }

    #[gpui::test]
    async fn test_tool_result_refusal(cx: &mut TestAppContext) {
        use std::sync::atomic::AtomicUsize;
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;

        // Create a connection that simulates refusal after tool result
        let prompt_count = Arc::new(AtomicUsize::new(0));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let prompt_count = prompt_count.clone();
            move |_request, thread, mut cx| {
                let count = prompt_count.fetch_add(1, SeqCst);
                async move {
                    if count == 0 {
                        // First prompt: Generate a tool call with result
                        thread.update(&mut cx, |thread, cx| {
                            thread
                                .handle_session_update(
                                    acp::SessionUpdate::ToolCall(
                                        acp::ToolCall::new("tool1", "Test Tool")
                                            .kind(acp::ToolKind::Fetch)
                                            .status(acp::ToolCallStatus::Completed)
                                            .raw_input(serde_json::json!({"query": "test"}))
                                            .raw_output(serde_json::json!({"result": "inappropriate content"})),
                                    ),
                                    cx,
                                )
                                .unwrap();
                        })?;

                        // Now return refusal because of the tool result
                        Ok(acp::PromptResponse::new(acp::StopReason::Refusal))
                    } else {
                        Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                    }
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Track if we see a Refusal event
        let saw_refusal_event = Arc::new(std::sync::Mutex::new(false));
        let saw_refusal_event_captured = saw_refusal_event.clone();
        thread.update(cx, |_thread, cx| {
            cx.subscribe(
                &thread,
                move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                    if matches!(event, AcpThreadEvent::Refusal) {
                        *saw_refusal_event_captured.lock().unwrap() = true;
                    }
                },
            )
            .detach();
        });

        // Send a user message - this will trigger tool call and then refusal
        let send_task = thread.update(cx, |thread, cx| thread.send(vec!["Hello".into()], cx));
        cx.background_executor.spawn(send_task).detach();
        cx.run_until_parked();

        // Verify that:
        // 1. A Refusal event WAS emitted (because it's a tool result refusal, not user prompt)
        // 2. The user message was NOT truncated
        assert!(
            *saw_refusal_event.lock().unwrap(),
            "Refusal event should be emitted for tool result refusals"
        );

        thread.read_with(cx, |thread, _| {
            let entries = thread.entries();
            assert!(entries.len() >= 2, "Should have user message and tool call");

            // Verify user message is still there
            assert!(
                matches!(entries[0], AgentThreadEntry::UserMessage(_)),
                "User message should not be truncated"
            );

            // Verify tool call is there with result
            if let AgentThreadEntry::ToolCall(tool_call) = &entries[1] {
                assert!(
                    tool_call.raw_output.is_some(),
                    "Tool call should have output"
                );
            } else {
                panic!("Expected tool call at index 1");
            }
        });
    }

    #[gpui::test]
    async fn test_user_prompt_refusal_emits_event(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, None, cx).await;

        let refuse_next = Arc::new(AtomicBool::new(false));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let refuse_next = refuse_next.clone();
            move |_request, _thread, _cx| {
                if refuse_next.load(SeqCst) {
                    async move { Ok(acp::PromptResponse::new(acp::StopReason::Refusal)) }
                        .boxed_local()
                } else {
                    async move { Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)) }
                        .boxed_local()
                }
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Track if we see a Refusal event
        let saw_refusal_event = Arc::new(std::sync::Mutex::new(false));
        let saw_refusal_event_captured = saw_refusal_event.clone();
        thread.update(cx, |_thread, cx| {
            cx.subscribe(
                &thread,
                move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                    if matches!(event, AcpThreadEvent::Refusal) {
                        *saw_refusal_event_captured.lock().unwrap() = true;
                    }
                },
            )
            .detach();
        });

        // Send a message that will be refused
        refuse_next.store(true, SeqCst);
        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx)))
            .await
            .unwrap();

        // Verify that a Refusal event WAS emitted for user prompt refusal
        assert!(
            *saw_refusal_event.lock().unwrap(),
            "Refusal event should be emitted for user prompt refusals"
        );

        // Verify the message was truncated (user prompt refusal)
        thread.read_with(cx, |thread, cx| {
            assert_eq!(thread.to_markdown(cx), "");
        });
    }

    #[gpui::test]
    async fn test_refusal(cx: &mut TestAppContext) {
        init_test(cx);
        let fs = FakeFs::new(cx.background_executor.clone());
        fs.insert_tree(path!("/"), json!({})).await;
        let project = Project::test(fs.clone(), [path!("/").as_ref()], cx).await;

        let refuse_next = Arc::new(AtomicBool::new(false));
        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let refuse_next = refuse_next.clone();
            move |request, thread, mut cx| {
                let refuse_next = refuse_next.clone();
                async move {
                    if refuse_next.load(SeqCst) {
                        return Ok(acp::PromptResponse::new(acp::StopReason::Refusal));
                    }

                    let acp::ContentBlock::Text(content) = &request.prompt[0] else {
                        panic!("expected text content block");
                    };
                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::AgentMessageChunk(acp::ContentChunk::new(
                                    content.text.to_uppercase().into(),
                                )),
                                cx,
                            )
                            .unwrap();
                    })?;
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["hello".into()], cx)))
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User

                    hello

                    ## Assistant

                    HELLO

                "}
            );
        });

        // Simulate refusing the second message. The message should be truncated
        // when a user prompt is refused.
        refuse_next.store(true, SeqCst);
        cx.update(|cx| thread.update(cx, |thread, cx| thread.send(vec!["world".into()], cx)))
            .await
            .unwrap();
        thread.read_with(cx, |thread, cx| {
            assert_eq!(
                thread.to_markdown(cx),
                indoc! {"
                    ## User

                    hello

                    ## Assistant

                    HELLO

                "}
            );
        });
    }

    async fn run_until_first_tool_call(
        thread: &Entity<AcpThread>,
        cx: &mut TestAppContext,
    ) -> usize {
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
        fn run(
            &self,
            _client_user_message_id: ClientUserMessageId,
            _cx: &mut App,
        ) -> Task<Result<()>> {
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

    #[gpui::test]
    async fn test_tool_call_not_found_creates_failed_entry(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Try to update a tool call that doesn't exist
        let nonexistent_id = acp::ToolCallId::new("nonexistent-tool-call");
        thread.update(cx, |thread, cx| {
            let result = thread.handle_session_update(
                acp::SessionUpdate::ToolCallUpdate(acp::ToolCallUpdate::new(
                    nonexistent_id.clone(),
                    acp::ToolCallUpdateFields::new().status(acp::ToolCallStatus::Completed),
                )),
                cx,
            );

            // The update should succeed (not return an error)
            assert!(result.is_ok());

            // There should now be exactly one entry in the thread
            assert_eq!(thread.entries.len(), 1);

            // The entry should be a failed tool call
            if let AgentThreadEntry::ToolCall(tool_call) = &thread.entries[0] {
                assert_eq!(tool_call.id, nonexistent_id);
                assert!(matches!(tool_call.status, ToolCallStatus::Failed));
                assert_eq!(tool_call.kind, acp::ToolKind::Fetch);

                // Check that the content contains the error message
                assert_eq!(tool_call.content.len(), 1);
                if let ToolCallContent::ContentBlock(content_block) = &tool_call.content[0] {
                    match content_block {
                        ContentBlock::Markdown { markdown } => {
                            let markdown_text = markdown.read(cx).source();
                            assert!(markdown_text.contains("Tool call not found"));
                        }
                        ContentBlock::Empty => panic!("Expected markdown content, got empty"),
                        ContentBlock::ResourceLink { .. } => {
                            panic!("Expected markdown content, got resource link")
                        }
                        ContentBlock::EmbeddedResource { .. } => {
                            panic!("Expected markdown content, got embedded resource")
                        }
                        ContentBlock::Image { .. } => {
                            panic!("Expected markdown content, got image")
                        }
                    }
                } else {
                    panic!("Expected ContentBlock, got: {:?}", tool_call.content[0]);
                }
            } else {
                panic!("Expected ToolCall entry, got: {:?}", thread.entries[0]);
            }
        });
    }

    /// Tests that restoring a checkpoint properly cleans up terminals that were
    /// created after that checkpoint, and cancels any in-progress generation.
    ///
    /// Reproduces issue #35142: When a checkpoint is restored, any terminal processes
    /// that were started after that checkpoint should be terminated, and any in-progress
    /// AI generation should be canceled.
    #[gpui::test]
    async fn test_restore_checkpoint_kills_terminal(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Send first user message to create a checkpoint
        cx.update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.send(vec!["first message".into()], cx)
            })
        })
        .await
        .unwrap();

        // Send second message (creates another checkpoint) - we'll restore to this one
        cx.update(|cx| {
            thread.update(cx, |thread, cx| {
                thread.send(vec!["second message".into()], cx)
            })
        })
        .await
        .unwrap();

        // Create 2 terminals BEFORE the checkpoint that have completed running
        let terminal_id_1 = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
        let mock_terminal_1 = cx.new(|cx| {
            let builder = ::terminal::TerminalBuilder::new_display_only(
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            );
            builder.subscribe(cx)
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id_1.clone(),
                    label: "echo 'first'".to_string(),
                    cwd: Some(PathBuf::from("/test")),
                    output_byte_limit: None,
                    terminal: mock_terminal_1.clone(),
                },
                cx,
            );
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Output {
                    terminal_id: terminal_id_1.clone(),
                    data: b"first\n".to_vec(),
                },
                cx,
            );
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Exit {
                    terminal_id: terminal_id_1.clone(),
                    status: acp::TerminalExitStatus::new().exit_code(0),
                },
                cx,
            );
        });

        let terminal_id_2 = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
        let mock_terminal_2 = cx.new(|cx| {
            let builder = ::terminal::TerminalBuilder::new_display_only(
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            );
            builder.subscribe(cx)
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id_2.clone(),
                    label: "echo 'second'".to_string(),
                    cwd: Some(PathBuf::from("/test")),
                    output_byte_limit: None,
                    terminal: mock_terminal_2.clone(),
                },
                cx,
            );
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Output {
                    terminal_id: terminal_id_2.clone(),
                    data: b"second\n".to_vec(),
                },
                cx,
            );
        });

        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Exit {
                    terminal_id: terminal_id_2.clone(),
                    status: acp::TerminalExitStatus::new().exit_code(0),
                },
                cx,
            );
        });

        // Get the second message ID to restore to
        let second_message_id = thread.read_with(cx, |thread, _| {
            // At this point we have:
            // - Index 0: First user message (with checkpoint)
            // - Index 1: Second user message (with checkpoint)
            // No assistant responses because FakeAgentConnection just returns EndTurn
            let AgentThreadEntry::UserMessage(message) = &thread.entries[1] else {
                panic!("expected user message at index 1");
            };
            message.client_id.clone().unwrap()
        });

        // Create a terminal AFTER the checkpoint we'll restore to.
        // This simulates the AI agent starting a long-running terminal command.
        let terminal_id = acp::TerminalId::new(uuid::Uuid::new_v4().to_string());
        let mock_terminal = cx.new(|cx| {
            let builder = ::terminal::TerminalBuilder::new_display_only(
                ::terminal::terminal_settings::CursorShape::default(),
                ::terminal::terminal_settings::AlternateScroll::On,
                None,
                0,
                cx.background_executor(),
                PathStyle::local(),
            );
            builder.subscribe(cx)
        });

        // Register the terminal as created
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Created {
                    terminal_id: terminal_id.clone(),
                    label: "sleep 1000".to_string(),
                    cwd: Some(PathBuf::from("/test")),
                    output_byte_limit: None,
                    terminal: mock_terminal.clone(),
                },
                cx,
            );
        });

        // Simulate the terminal producing output (still running)
        thread.update(cx, |thread, cx| {
            thread.on_terminal_provider_event(
                TerminalProviderEvent::Output {
                    terminal_id: terminal_id.clone(),
                    data: b"terminal is running...\n".to_vec(),
                },
                cx,
            );
        });

        // Create a tool call entry that references this terminal
        // This represents the agent requesting a terminal command
        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::ToolCall(
                        acp::ToolCall::new("terminal-tool-1", "Running command")
                            .kind(acp::ToolKind::Execute)
                            .status(acp::ToolCallStatus::InProgress)
                            .content(vec![acp::ToolCallContent::Terminal(acp::Terminal::new(
                                terminal_id.clone(),
                            ))])
                            .raw_input(serde_json::json!({"command": "sleep 1000", "cd": "/test"})),
                    ),
                    cx,
                )
                .unwrap();
        });

        // Verify terminal exists and is in the thread
        let terminal_exists_before =
            thread.read_with(cx, |thread, _| thread.terminals.contains_key(&terminal_id));
        assert!(
            terminal_exists_before,
            "Terminal should exist before checkpoint restore"
        );

        // Verify the terminal's underlying task is still running (not completed)
        let terminal_running_before = thread.read_with(cx, |thread, _cx| {
            let terminal_entity = thread.terminals.get(&terminal_id).unwrap();
            terminal_entity.read_with(cx, |term, _cx| {
                term.output().is_none() // output is None means it's still running
            })
        });
        assert!(
            terminal_running_before,
            "Terminal should be running before checkpoint restore"
        );

        // Verify we have the expected entries before restore
        let entry_count_before = thread.read_with(cx, |thread, _| thread.entries.len());
        assert!(
            entry_count_before > 1,
            "Should have multiple entries before restore"
        );

        // Restore the checkpoint to the second message.
        // This should:
        // 1. Cancel any in-progress generation (via the cancel() call)
        // 2. Remove the terminal that was created after that point
        thread
            .update(cx, |thread, cx| {
                thread.restore_checkpoint(second_message_id, cx)
            })
            .await
            .unwrap();

        // Verify that no send_task is in progress after restore
        // (cancel() clears the send_task)
        let has_send_task_after = thread.read_with(cx, |thread, _| thread.running_turn.is_some());
        assert!(
            !has_send_task_after,
            "Should not have a send_task after restore (cancel should have cleared it)"
        );

        // Verify the entries were truncated (restoring to index 1 truncates at 1, keeping only index 0)
        let entry_count = thread.read_with(cx, |thread, _| thread.entries.len());
        assert_eq!(
            entry_count, 1,
            "Should have 1 entry after restore (only the first user message)"
        );

        // Verify the 2 completed terminals from before the checkpoint still exist
        let terminal_1_exists = thread.read_with(cx, |thread, _| {
            thread.terminals.contains_key(&terminal_id_1)
        });
        assert!(
            terminal_1_exists,
            "Terminal 1 (from before checkpoint) should still exist"
        );

        let terminal_2_exists = thread.read_with(cx, |thread, _| {
            thread.terminals.contains_key(&terminal_id_2)
        });
        assert!(
            terminal_2_exists,
            "Terminal 2 (from before checkpoint) should still exist"
        );

        // Verify they're still in completed state
        let terminal_1_completed = thread.read_with(cx, |thread, _cx| {
            let terminal_entity = thread.terminals.get(&terminal_id_1).unwrap();
            terminal_entity.read_with(cx, |term, _cx| term.output().is_some())
        });
        assert!(terminal_1_completed, "Terminal 1 should still be completed");

        let terminal_2_completed = thread.read_with(cx, |thread, _cx| {
            let terminal_entity = thread.terminals.get(&terminal_id_2).unwrap();
            terminal_entity.read_with(cx, |term, _cx| term.output().is_some())
        });
        assert!(terminal_2_completed, "Terminal 2 should still be completed");

        // Verify the running terminal (created after checkpoint) was removed
        let terminal_3_exists =
            thread.read_with(cx, |thread, _| thread.terminals.contains_key(&terminal_id));
        assert!(
            !terminal_3_exists,
            "Terminal 3 (created after checkpoint) should have been removed"
        );

        // Verify total count is 2 (the two from before the checkpoint)
        let terminal_count = thread.read_with(cx, |thread, _| thread.terminals.len());
        assert_eq!(
            terminal_count, 2,
            "Should have exactly 2 terminals (the completed ones from before checkpoint)"
        );
    }

    /// Tests that update_last_checkpoint correctly updates the original message's checkpoint
    /// even when a new user message is added while the async checkpoint comparison is in progress.
    ///
    /// This is a regression test for a bug where update_last_checkpoint would fail with
    /// "no checkpoint" if a new user message (without a checkpoint) was added between when
    /// update_last_checkpoint started and when its async closure ran.
    #[gpui::test]
    async fn test_update_last_checkpoint_with_new_message_added(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        fs.insert_tree(path!("/test"), json!({".git": {}, "file.txt": "content"}))
            .await;
        let project = Project::test(fs.clone(), [Path::new(path!("/test"))], cx).await;

        let handler_done = Arc::new(AtomicBool::new(false));
        let handler_done_clone = handler_done.clone();
        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            move |_, _thread, _cx| {
                handler_done_clone.store(true, SeqCst);
                async move { Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)) }.boxed_local()
            },
        ));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let send_future = thread.update(cx, |thread, cx| thread.send_raw("First message", cx));
        let send_task = cx.background_executor.spawn(send_future);

        // Tick until handler completes, then a few more to let update_last_checkpoint start
        while !handler_done.load(SeqCst) {
            cx.executor().tick();
        }
        for _ in 0..5 {
            cx.executor().tick();
        }

        thread.update(cx, |thread, cx| {
            thread.push_entry(
                AgentThreadEntry::UserMessage(UserMessage {
                    protocol_id: None,
                    client_id: Some(ClientUserMessageId::new()),
                    is_optimistic: true,
                    content: ContentBlock::Empty,
                    chunks: vec!["Injected message (no checkpoint)".into()],
                    checkpoint: None,
                    indented: false,
                }),
                cx,
            );
        });

        cx.run_until_parked();
        let result = send_task.await;

        assert!(
            result.is_ok(),
            "send should succeed even when new message added during update_last_checkpoint: {:?}",
            result.err()
        );
    }

    /// Tests that when a follow-up message is sent during generation,
    /// the first turn completing does NOT clear `running_turn` because
    /// it now belongs to the second turn.
    #[gpui::test]
    async fn test_follow_up_message_during_generation_does_not_clear_turn(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        // First handler waits for this signal before completing
        let (first_complete_tx, first_complete_rx) = futures::channel::oneshot::channel::<()>();
        let first_complete_rx = RefCell::new(Some(first_complete_rx));

        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            move |params, _thread, _cx| {
                let first_complete_rx = first_complete_rx.borrow_mut().take();
                let is_first = params
                    .prompt
                    .iter()
                    .any(|c| matches!(c, acp::ContentBlock::Text(t) if t.text.contains("first")));

                async move {
                    if is_first {
                        // First handler waits until signaled
                        if let Some(rx) = first_complete_rx {
                            rx.await.ok();
                        }
                    }
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Send first message (turn_id=1) - handler will block
        let first_request = thread.update(cx, |thread, cx| thread.send_raw("first", cx));
        assert_eq!(thread.read_with(cx, |t, _| t.turn_id), 1);

        // Send second message (turn_id=2) while first is still blocked
        // This calls cancel() which takes turn 1's running_turn and sets turn 2's
        let second_request = thread.update(cx, |thread, cx| thread.send_raw("second", cx));
        assert_eq!(thread.read_with(cx, |t, _| t.turn_id), 2);

        let running_turn_after_second_send =
            thread.read_with(cx, |thread, _| thread.running_turn.as_ref().map(|t| t.id));
        assert_eq!(
            running_turn_after_second_send,
            Some(2),
            "running_turn should be set to turn 2 after sending second message"
        );

        // Now signal first handler to complete
        first_complete_tx.send(()).ok();

        // First request completes - should NOT clear running_turn
        // because running_turn now belongs to turn 2
        first_request.await.unwrap();

        let running_turn_after_first =
            thread.read_with(cx, |thread, _| thread.running_turn.as_ref().map(|t| t.id));
        assert_eq!(
            running_turn_after_first,
            Some(2),
            "first turn completing should not clear running_turn (belongs to turn 2)"
        );

        // Second request completes - SHOULD clear running_turn
        second_request.await.unwrap();

        let running_turn_after_second =
            thread.read_with(cx, |thread, _| thread.running_turn.is_some());
        assert!(
            !running_turn_after_second,
            "second turn completing should clear running_turn"
        );
    }

    #[gpui::test]
    async fn test_stale_cancelled_response_does_not_cancel_current_compaction(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let (first_complete_tx, first_complete_rx) = futures::channel::oneshot::channel::<()>();
        let first_complete_rx = RefCell::new(Some(first_complete_rx));
        let compaction_id = ContextCompactionId("test-compaction".into());

        let connection = Rc::new(FakeAgentConnection::new().on_user_message({
            let compaction_id = compaction_id.clone();
            move |params, thread, mut cx| {
                let first_complete_rx = first_complete_rx.borrow_mut().take();
                let is_first = params.prompt.iter().any(|content| {
                    matches!(content, acp::ContentBlock::Text(text) if text.text.contains("first"))
                });
                let compaction_id = compaction_id.clone();

                async move {
                    if is_first {
                        if let Some(rx) = first_complete_rx {
                            rx.await
                                .expect("first completion sender should still be alive");
                        }

                        thread.update(&mut cx, |thread, cx| {
                            thread.push_context_compaction(
                                ContextCompaction {
                                    id: compaction_id,
                                    status: ContextCompactionStatus::InProgress,
                                    summary: None,
                                },
                                cx,
                            );
                        })?;

                        Ok(acp::PromptResponse::new(acp::StopReason::Cancelled))
                    } else {
                        Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
                    }
                }
                .boxed_local()
            }
        }));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let first_request = thread.update(cx, |thread, cx| thread.send_raw("first", cx));
        assert_eq!(thread.read_with(cx, |thread, _| thread.turn_id), 1);

        let second_request = thread.update(cx, |thread, cx| thread.send_raw("second", cx));
        assert_eq!(thread.read_with(cx, |thread, _| thread.turn_id), 2);

        first_complete_tx
            .send(())
            .expect("first completion receiver should still be alive");

        let response = first_request
            .await
            .expect("first request should complete")
            .expect("first request should have response");
        assert_eq!(response.stop_reason, acp::StopReason::Cancelled);

        thread.read_with(cx, |thread, _| {
            let compaction = thread
                .entries
                .iter()
                .find_map(|entry| {
                    let AgentThreadEntry::ContextCompaction(compaction) = entry else {
                        return None;
                    };
                    (compaction.id == compaction_id).then_some(compaction)
                })
                .expect("compaction entry should exist");

            assert_eq!(
                compaction.status,
                ContextCompactionStatus::InProgress,
                "a stale cancelled response from an older turn should not cancel current compaction"
            );
        });

        second_request
            .await
            .expect("second request should complete");
    }

    #[gpui::test]
    async fn test_send_omits_message_id_without_client_user_message_id_support(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let connection = Rc::new(FakeAgentConnection::new().without_truncate_support());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let response = thread
            .update(cx, |thread, cx| thread.send_raw("test message", cx))
            .await;

        assert!(response.is_ok(), "send should not fail: {response:?}");
        thread.read_with(cx, |thread, _| {
            let AgentThreadEntry::UserMessage(message) = &thread.entries[0] else {
                panic!("expected first entry to be a user message")
            };
            assert_eq!(message.protocol_id, None);
            assert_eq!(message.client_id, None);
            assert!(message.is_optimistic);
        });
    }

    #[gpui::test]
    async fn test_send_returns_cancelled_response_and_marks_tools_as_cancelled(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            move |_params, thread, mut cx| {
                async move {
                    thread
                        .update(&mut cx, |thread, cx| {
                            thread.handle_session_update(
                                acp::SessionUpdate::ToolCall(
                                    acp::ToolCall::new(
                                        acp::ToolCallId::new("test-tool"),
                                        "Test Tool",
                                    )
                                    .kind(acp::ToolKind::Fetch)
                                    .status(acp::ToolCallStatus::InProgress),
                                ),
                                cx,
                            )
                        })
                        .unwrap()
                        .unwrap();

                    Ok(acp::PromptResponse::new(acp::StopReason::Cancelled))
                }
                .boxed_local()
            },
        ));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let response = thread
            .update(cx, |thread, cx| thread.send_raw("test message", cx))
            .await;

        let response = response
            .expect("send should succeed")
            .expect("should have response");
        assert_eq!(
            response.stop_reason,
            acp::StopReason::Cancelled,
            "response should have Cancelled stop_reason"
        );

        thread.read_with(cx, |thread, _| {
            let tool_entry = thread
                .entries
                .iter()
                .find_map(|e| {
                    if let AgentThreadEntry::ToolCall(call) = e {
                        Some(call)
                    } else {
                        None
                    }
                })
                .expect("should have tool call entry");

            assert!(
                matches!(tool_entry.status, ToolCallStatus::Canceled),
                "tool should be marked as Canceled when response is Cancelled, got {:?}",
                tool_entry.status
            );
        });
    }

    #[gpui::test]
    async fn test_provisional_title_replaced_by_real_title(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let set_title_calls = connection.set_title_calls.clone();

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        // Initial title is the default.
        thread.read_with(cx, |thread, _| {
            assert_eq!(thread.title(), None);
        });

        // Setting a provisional title updates the display title.
        thread.update(cx, |thread, cx| {
            thread.set_provisional_title("Hello, can you help…".into(), cx);
        });
        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread.title().as_ref().map(|s| s.as_str()),
                Some("Hello, can you help…")
            );
        });

        // The provisional title should NOT have propagated to the connection.
        assert_eq!(
            set_title_calls.borrow().len(),
            0,
            "provisional title should not propagate to the connection"
        );

        // When the real title arrives via set_title, it replaces the
        // provisional title and propagates to the connection.
        let task = thread.update(cx, |thread, cx| {
            thread.set_title("Helping with Rust question".into(), cx)
        });
        task.await.expect("set_title should succeed");
        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread.title().as_ref().map(|s| s.as_str()),
                Some("Helping with Rust question")
            );
        });
        assert_eq!(
            set_title_calls.borrow().as_slice(),
            &[SharedString::from("Helping with Rust question")],
            "real title should propagate to the connection"
        );
    }

    #[gpui::test]
    async fn test_session_info_update_replaces_provisional_title_and_emits_event(
        cx: &mut TestAppContext,
    ) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());

        let thread = cx
            .update(|cx| {
                connection.clone().new_session(
                    project,
                    PathList::new(&[Path::new(path!("/test"))]),
                    cx,
                )
            })
            .await
            .unwrap();

        let title_updated_events = Rc::new(RefCell::new(0usize));
        let title_updated_events_for_subscription = title_updated_events.clone();
        thread.update(cx, |_thread, cx| {
            cx.subscribe(
                &thread,
                move |_thread, _event_thread, event: &AcpThreadEvent, _cx| {
                    if matches!(event, AcpThreadEvent::TitleUpdated) {
                        *title_updated_events_for_subscription.borrow_mut() += 1;
                    }
                },
            )
            .detach();
        });

        thread.update(cx, |thread, cx| {
            thread.set_provisional_title("Hello, can you help…".into(), cx);
        });
        assert_eq!(
            *title_updated_events.borrow(),
            1,
            "setting a provisional title should emit TitleUpdated"
        );

        let result = thread.update(cx, |thread, cx| {
            thread.handle_session_update(
                acp::SessionUpdate::SessionInfoUpdate(
                    acp::SessionInfoUpdate::new().title("Helping with Rust question"),
                ),
                cx,
            )
        });
        result.expect("session info update should succeed");

        thread.read_with(cx, |thread, _| {
            assert_eq!(
                thread.title().as_ref().map(|s| s.as_str()),
                Some("Helping with Rust question")
            );
            assert!(
                !thread.has_provisional_title(),
                "session info title update should clear provisional title"
            );
        });

        assert_eq!(
            *title_updated_events.borrow(),
            2,
            "session info title update should emit TitleUpdated"
        );
        assert!(
            connection.set_title_calls.borrow().is_empty(),
            "session info title update should not propagate back to the connection"
        );
    }

    #[gpui::test]
    async fn test_usage_update_populates_token_usage_and_cost(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(
                        acp::UsageUpdate::new(5000, 10000).cost(acp::Cost::new(0.42, "USD")),
                    ),
                    cx,
                )
                .unwrap();
        });

        thread.read_with(cx, |thread, _| {
            let usage = thread.token_usage().expect("token_usage should be set");
            assert_eq!(usage.max_tokens, 10000);
            assert_eq!(usage.used_tokens, 5000);

            let cost = thread.cost().expect("cost should be set");
            assert!((cost.amount - 0.42).abs() < f64::EPSILON);
            assert_eq!(cost.currency.as_ref(), "USD");
        });
    }

    #[gpui::test]
    async fn test_context_compaction_preserves_token_usage(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(
                        acp::UsageUpdate::new(5000, 10000).cost(acp::Cost::new(0.42, "USD")),
                    ),
                    cx,
                )
                .unwrap();

            thread.push_context_compaction(
                ContextCompaction {
                    id: ContextCompactionId("compaction-1".into()),
                    status: ContextCompactionStatus::InProgress,
                    summary: None,
                },
                cx,
            );
        });

        thread.read_with(cx, |thread, _| {
            let usage = thread
                .token_usage()
                .expect("context compaction should not clear token usage on its own");
            assert_eq!(usage.used_tokens, 5000);
            assert_eq!(usage.max_tokens, 10000);

            let cost = thread
                .cost()
                .expect("context compaction should not clear cost on its own");
            assert!((cost.amount - 0.42).abs() < f64::EPSILON);
        });

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(acp::UsageUpdate::new(1000, 10000)),
                    cx,
                )
                .unwrap();
        });

        thread.read_with(cx, |thread, _| {
            let usage = thread
                .token_usage()
                .expect("token_usage should be restored by the next usage update");
            assert_eq!(usage.used_tokens, 1000);
            assert_eq!(usage.max_tokens, 10000);
        });
    }

    #[gpui::test]
    async fn test_usage_update_without_cost_preserves_existing_cost(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(
                        acp::UsageUpdate::new(1000, 10000).cost(acp::Cost::new(0.10, "USD")),
                    ),
                    cx,
                )
                .unwrap();

            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(acp::UsageUpdate::new(2000, 10000)),
                    cx,
                )
                .unwrap();
        });

        thread.read_with(cx, |thread, _| {
            let usage = thread.token_usage().expect("token_usage should be set");
            assert_eq!(usage.used_tokens, 2000);

            let cost = thread.cost().expect("cost should be preserved");
            assert!((cost.amount - 0.10).abs() < f64::EPSILON);
        });
    }

    #[gpui::test]
    async fn test_response_usage_does_not_clobber_session_usage(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            move |_, thread, mut cx| {
                async move {
                    thread.update(&mut cx, |thread, cx| {
                        thread
                            .handle_session_update(
                                acp::SessionUpdate::UsageUpdate(
                                    acp::UsageUpdate::new(3000, 10000)
                                        .cost(acp::Cost::new(0.05, "EUR")),
                                ),
                                cx,
                            )
                            .unwrap();
                    })?;
                    Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)
                        .usage(acp::Usage::new(500, 200, 300)))
                }
                .boxed_local()
            },
        ));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread
            .update(cx, |thread, cx| thread.send_raw("hello", cx))
            .await
            .unwrap();

        thread.read_with(cx, |thread, _| {
            let usage = thread.token_usage().expect("token_usage should be set");
            assert_eq!(usage.max_tokens, 10000, "max_tokens from UsageUpdate");
            assert_eq!(usage.used_tokens, 3000, "used_tokens from UsageUpdate");
            assert_eq!(usage.input_tokens, 200, "input_tokens from response usage");
            assert_eq!(
                usage.output_tokens, 300,
                "output_tokens from response usage"
            );

            let cost = thread.cost().expect("cost should be set");
            assert!((cost.amount - 0.05).abs() < f64::EPSILON);
            assert_eq!(cost.currency.as_ref(), "EUR");
        });
    }

    #[gpui::test]
    async fn test_clearing_token_usage_also_clears_cost(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;
        let connection = Rc::new(FakeAgentConnection::new());
        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        thread.update(cx, |thread, cx| {
            thread
                .handle_session_update(
                    acp::SessionUpdate::UsageUpdate(
                        acp::UsageUpdate::new(1000, 10000).cost(acp::Cost::new(0.25, "USD")),
                    ),
                    cx,
                )
                .unwrap();

            assert!(thread.token_usage().is_some());
            assert!(thread.cost().is_some());

            thread.update_token_usage(None, cx);

            assert!(thread.token_usage().is_none());
            assert!(
                thread.cost().is_none(),
                "cost should be cleared when token usage is cleared"
            );
        });
    }

    /// Regression test: if the inner send_task is cancelled before it can
    /// fire `tx.send(...)` (e.g. because the underlying future was dropped),
    /// the outer task observes `rx.await` returning `Err(Cancelled)` and
    /// must still clear `running_turn` so the panel transitions out of
    /// `Generating`. Without this, the agent thread is wedged in the
    /// loading state until Mav restarts.
    #[gpui::test]
    async fn test_running_turn_cleared_when_send_task_dropped(cx: &mut TestAppContext) {
        init_test(cx);

        let fs = FakeFs::new(cx.executor());
        let project = Project::test(fs, [], cx).await;

        // Handler hangs forever so the spawn at run_turn is parked inside
        // `f(this, cx).await` with `tx` still alive but unsent.
        let connection = Rc::new(FakeAgentConnection::new().on_user_message(
            |_params, _thread, _cx| {
                async move { futures::future::pending::<Result<acp::PromptResponse>>().await }
                    .boxed_local()
            },
        ));

        let thread = cx
            .update(|cx| {
                connection.new_session(project, PathList::new(&[Path::new(path!("/test"))]), cx)
            })
            .await
            .unwrap();

        let request = thread.update(cx, |thread, cx| thread.send_raw("hello", cx));
        cx.run_until_parked();

        assert_eq!(
            thread.read_with(cx, |t, _| t.status()),
            ThreadStatus::Generating,
            "thread should be generating while the handler is parked"
        );

        // Replace the in-flight send_task with a no-op. Dropping the original
        // Task cancels its inner future, which drops `tx` without ever calling
        // `tx.send(...)`. This mirrors the production scenario where the
        // send_task future is cancelled before completion.
        thread.update(cx, |thread, _| {
            thread.running_turn.as_mut().unwrap().send_task = Task::ready(());
        });

        let result = request.await;
        assert!(
            matches!(result, Ok(None)),
            "outer task should resolve to Ok(None) on dropped tx, got {result:?}"
        );

        assert_eq!(
            thread.read_with(cx, |t, _| t.status()),
            ThreadStatus::Idle,
            "running_turn must be cleared even when tx was dropped without send"
        );
    }
}
