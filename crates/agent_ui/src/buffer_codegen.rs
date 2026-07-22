use crate::{context::LoadedContext, inline_prompt_editor::CodegenStatus};
use agent_settings::AgentSettings;
use anyhow::{Context as _, Result};
use collections::{HashMap, HashSet};
use editor::{Anchor, AnchorRangeExt, MultiBuffer, MultiBufferSnapshot, ToOffset as _, ToPoint};
use futures::FutureExt;
use futures::{
    SinkExt, Stream, StreamExt, TryStreamExt as _,
    channel::mpsc,
    future::{LocalBoxFuture, Shared},
    join,
    stream::BoxStream,
};
use gpui::{App, AppContext as _, AsyncApp, Context, Entity, EventEmitter, Subscription, Task};
use language::{
    Buffer, BufferEditSource, IndentKind, LanguageName, Point, TransactionId, line_diff,
};
use language_model::{
    CompletionIntent, LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelRegistry, LanguageModelRequest, LanguageModelRequestMessage,
    LanguageModelRequestTool, LanguageModelTextStream, LanguageModelToolChoice,
    LanguageModelToolUse, LanguageModelToolUseId, Role, TokenUsage,
};
use language_models::provider::anthropic::telemetry::{
    AnthropicCompletionType, AnthropicEventData, AnthropicEventReporter, AnthropicEventType,
};
use multi_buffer::MultiBufferRow;
use parking_lot::Mutex;
use prompt_store::PromptBuilder;
use rope::Rope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::Settings as _;
use std::{
    cmp,
    future::Future,
    iter,
    ops::{Range, RangeInclusive},
    pin::Pin,
    sync::Arc,
    task::{self, Poll},
    time::Instant,
};
use streaming_diff::{CharOperation, LineDiff, LineOperation, StreamingDiff};
use uuid::Uuid;

/// Use this tool when you cannot or should not make a rewrite. This includes:
/// - The user's request is unclear, ambiguous, or nonsensical
/// - The requested change cannot be made by only editing the <rewrite_this> section
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FailureMessageInput {
    /// A brief message to the user explaining why you're unable to fulfill the request or to ask a question about the request.
    #[serde(default)]
    pub message: String,
}

/// Replaces text in <rewrite_this></rewrite_this> tags with your replacement_text.
/// Only use this tool when you are confident you understand the user's request and can fulfill it
/// by editing the marked section.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct RewriteSectionInput {
    /// The text to replace the section with.
    #[serde(default)]
    pub replacement_text: String,
}

pub struct BufferCodegen {
    alternatives: Vec<Entity<CodegenAlternative>>,
    pub active_alternative: usize,
    seen_alternatives: HashSet<usize>,
    subscriptions: Vec<Subscription>,
    buffer: Entity<MultiBuffer>,
    range: Range<Anchor>,
    initial_transaction_id: Option<TransactionId>,
    builder: Arc<PromptBuilder>,
    pub is_insertion: bool,
    session_id: Uuid,
}

pub const REWRITE_SECTION_TOOL_NAME: &str = "rewrite_section";
pub const FAILURE_MESSAGE_TOOL_NAME: &str = "failure_message";

impl EventEmitter<CodegenEvent> for BufferCodegen {}

pub struct CodegenAlternative {
    buffer: Entity<MultiBuffer>,
    old_buffer: Entity<Buffer>,
    snapshot: MultiBufferSnapshot,
    edit_position: Option<Anchor>,
    range: Range<Anchor>,
    last_equal_ranges: Vec<Range<Anchor>>,
    transformation_transaction_id: Option<TransactionId>,
    status: CodegenStatus,
    generation: Task<()>,
    diff: Diff,
    _subscription: gpui::Subscription,
    builder: Arc<PromptBuilder>,
    active: bool,
    edits: Vec<(Range<Anchor>, String)>,
    line_operations: Vec<LineOperation>,
    elapsed_time: Option<f64>,
    completion: Option<String>,
    selected_text: Option<String>,
    pub message_id: Option<String>,
    session_id: Uuid,
    pub description: Option<String>,
    pub failure: Option<String>,
}

impl EventEmitter<CodegenEvent> for CodegenAlternative {}

impl CodegenAlternative {
    pub fn new(
        buffer: Entity<MultiBuffer>,
        range: Range<Anchor>,
        active: bool,
        builder: Arc<PromptBuilder>,
        session_id: Uuid,
        cx: &mut Context<Self>,
    ) -> Self {
        let snapshot = buffer.read(cx).snapshot(cx);

        let (old_buffer, _, _) = snapshot
            .range_to_buffer_ranges(range.start..range.end)
            .pop()
            .unwrap();
        let old_buffer = cx.new(|cx| {
            let text = old_buffer.as_rope().clone();
            let line_ending = old_buffer.line_ending();
            let language = old_buffer.language().cloned();
            let language_registry = buffer
                .read(cx)
                .buffer(old_buffer.remote_id())
                .unwrap()
                .read(cx)
                .language_registry();

            let mut buffer = Buffer::local_normalized(text, line_ending, cx);
            buffer.set_language(language, cx);
            if let Some(language_registry) = language_registry {
                buffer.set_language_registry(language_registry);
            }
            buffer
        });

        Self {
            buffer: buffer.clone(),
            old_buffer,
            edit_position: None,
            message_id: None,
            snapshot,
            last_equal_ranges: Default::default(),
            transformation_transaction_id: None,
            status: CodegenStatus::Idle,
            generation: Task::ready(()),
            diff: Diff::default(),
            builder,
            active: active,
            edits: Vec::new(),
            line_operations: Vec::new(),
            range,
            elapsed_time: None,
            completion: None,
            selected_text: None,
            session_id,
            description: None,
            failure: None,
            _subscription: cx.subscribe(&buffer, Self::handle_buffer_event),
        }
    }

    pub fn language_name(&self, cx: &App) -> Option<LanguageName> {
        self.old_buffer
            .read(cx)
            .language()
            .map(|language| language.name())
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if active != self.active {
            self.active = active;

            if self.active {
                let edits = self.edits.clone();
                self.apply_edits(edits, cx);
                if matches!(self.status, CodegenStatus::Pending) {
                    let line_operations = self.line_operations.clone();
                    self.reapply_line_based_diff(line_operations, cx);
                } else {
                    self.reapply_batch_diff(cx).detach();
                }
            } else if let Some(transaction_id) = self.transformation_transaction_id.take() {
                self.buffer.update(cx, |buffer, cx| {
                    buffer.undo_transaction(transaction_id, cx);
                    buffer.forget_transaction(transaction_id, cx);
                });
            }
        }
    }

    fn handle_buffer_event(
        &mut self,
        _buffer: Entity<MultiBuffer>,
        event: &multi_buffer::Event,
        cx: &mut Context<Self>,
    ) {
        if let multi_buffer::Event::TransactionUndone { transaction_id } = event
            && self.transformation_transaction_id == Some(*transaction_id)
        {
            self.transformation_transaction_id = None;
            self.generation = Task::ready(());
            cx.emit(CodegenEvent::Undone);
        }
    }

    pub fn last_equal_ranges(&self) -> &[Range<Anchor>] {
        &self.last_equal_ranges
    }

    pub fn use_streaming_tools(model: &dyn LanguageModel, cx: &App) -> bool {
        model.supports_streaming_tools()
            && AgentSettings::get_global(cx).inline_assistant_use_streaming_tools
    }

    pub fn start(
        &mut self,
        user_prompt: String,
        context_task: Shared<Task<Option<LoadedContext>>>,
        model: Arc<dyn LanguageModel>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        // Clear the model explanation since the user has started a new generation.
        self.description = None;

        if let Some(transformation_transaction_id) = self.transformation_transaction_id.take() {
            self.buffer.update(cx, |buffer, cx| {
                buffer.undo_transaction(transformation_transaction_id, cx);
            });
        }

        self.edit_position = Some(self.range.start.bias_right(&self.snapshot));

        if Self::use_streaming_tools(model.as_ref(), cx) {
            let request = self.build_request(&model, user_prompt, context_task, cx)?;
            let completion_events = cx.spawn({
                let model = model.clone();
                async move |_, cx| model.stream_completion(request.await, cx).await
            });
            self.generation = self.handle_completion(model, completion_events, cx);
        } else {
            let stream: LocalBoxFuture<Result<LanguageModelTextStream>> =
                if user_prompt.trim().to_lowercase() == "delete" {
                    async { Ok(LanguageModelTextStream::default()) }.boxed_local()
                } else {
                    let request = self.build_request(&model, user_prompt, context_task, cx)?;
                    cx.spawn({
                        let model = model.clone();
                        async move |_, cx| {
                            Ok(model.stream_completion_text(request.await, cx).await?)
                        }
                    })
                    .boxed_local()
                };
            self.generation =
                self.handle_stream(model, /* strip_invalid_spans: */ true, stream, cx);
        }

        Ok(())
    }

    pub fn current_completion(&self) -> Option<String> {
        self.completion.clone()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn current_description(&self) -> Option<String> {
        self.description.clone()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn current_failure(&self) -> Option<String> {
        self.failure.clone()
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selected_text.as_deref()
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        self.last_equal_ranges.clear();
        if self.diff.is_empty() {
            self.status = CodegenStatus::Idle;
        } else {
            self.status = CodegenStatus::Done;
        }
        self.generation = Task::ready(());
        cx.emit(CodegenEvent::Finished);
        cx.notify();
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.buffer.update(cx, |buffer, cx| {
            if let Some(transaction_id) = self.transformation_transaction_id.take() {
                buffer.undo_transaction(transaction_id, cx);
                buffer.refresh_preview(cx);
            }
        });
    }
}

#[derive(Copy, Clone, Debug)]
pub enum CodegenEvent {
    Finished,
    Undone,
}

mod codegen;
mod completion;
mod diff;
mod edits;
mod request;
mod stream;
mod strip_invalid_spans;

pub use diff::Diff;
use strip_invalid_spans::StripInvalidSpans;

#[cfg(test)]
mod test_autoindent;
#[cfg(test)]
mod test_tools;
