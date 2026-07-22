use anyhow::{Result, anyhow};
use collections::HashMap;
use futures::{Stream, StreamExt};
use language_model_core::{
    CompactionContent, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelImage, LanguageModelRequest, LanguageModelRequestMessage, LanguageModelToolChoice,
    LanguageModelToolResultContent, LanguageModelToolUse, LanguageModelToolUseId, MessageContent,
    Role, StopReason, TokenUsage,
    util::{fix_streamed_json, parse_tool_arguments},
};
use std::pin::Pin;
use std::sync::Arc;

use crate::responses::{
    ContextManagement, Request as ResponseRequest, ResponseCompactionItem, ResponseError,
    ResponseFunctionCallItem, ResponseFunctionCallOutputContent, ResponseFunctionCallOutputItem,
    ResponseIncludable, ResponseInputContent, ResponseInputItem, ResponseMessageItem,
    ResponseOutputItem, ResponseOutputMessage, ResponseReasoningInputItem, ResponseReasoningItem,
    ResponseReasoningSummaryPart, ResponseSummary as ResponsesSummary,
    ResponseUsage as ResponsesUsage, StreamEvent as ResponsesStreamEvent,
};
use crate::{
    FunctionContent, FunctionDefinition, ImageUrl, MessagePart, ReasoningEffort,
    ResponseStreamEvent, ServiceTier, ToolCall, ToolCallContent,
};

const RESPONSE_MESSAGE_PHASE_COMMENTARY: &str = "commentary";
const RESPONSE_MESSAGE_PHASE_FINAL_ANSWER: &str = "final_answer";

mod chat_events;
mod request;
mod response_helpers;
mod response_items;
mod responses_events;

pub use chat_events::OpenAiEventMapper;
pub use request::{ChatCompletionMaxTokensParameter, into_open_ai};
pub use response_items::into_open_ai_response;
pub use responses_events::OpenAiResponseEventMapper;

#[cfg(test)]
mod tests;
