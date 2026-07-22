use crate::responses::{
    ReasoningSummaryPart, ResponseError, ResponseFunctionToolCall, ResponseIncompleteDetails,
    ResponseInputTokensDetails, ResponseOutputItem, ResponseOutputMessage, ResponseReasoningItem,
    ResponseSummary, ResponseUsage, StreamEvent as ResponsesStreamEvent,
};
use futures::{StreamExt, executor::block_on};
use language_model_core::{
    LanguageModelImage, LanguageModelRequestMessage, LanguageModelRequestTool,
    LanguageModelToolResult, LanguageModelToolResultContent, LanguageModelToolUse,
    LanguageModelToolUseId, SharedString, Speed,
};
use serde_json::json;

use super::*;
use crate::{ChoiceDelta, FunctionChunk, ResponseMessageDelta, ResponseStreamEvent, ToolCallChunk};

#[path = "request_payload_tests.rs"]
mod request_payload_tests;
#[path = "request_reasoning_tests.rs"]
mod request_reasoning_tests;

fn map_response_events(events: Vec<ResponsesStreamEvent>) -> Vec<LanguageModelCompletionEvent> {
    block_on(async {
        OpenAiResponseEventMapper::new()
            .map_stream(Box::pin(futures::stream::iter(events.into_iter().map(Ok))))
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .map(Result::unwrap)
            .collect()
    })
}

fn map_completion_events(events: Vec<ResponseStreamEvent>) -> Vec<LanguageModelCompletionEvent> {
    let mut mapper = OpenAiEventMapper::new();
    let mut all_events = Vec::new();
    for event in events {
        all_events.extend(mapper.map_event(event));
    }
    all_events.into_iter().filter_map(|e| e.ok()).collect()
}

fn response_item_message(id: &str) -> ResponseOutputItem {
    ResponseOutputItem::Message(ResponseOutputMessage {
        id: Some(id.to_string()),
        role: Some("assistant".to_string()),
        status: Some("in_progress".to_string()),
        content: vec![],
        phase: None,
    })
}

fn response_item_function_call(id: &str, args: Option<&str>) -> ResponseOutputItem {
    ResponseOutputItem::FunctionCall(ResponseFunctionToolCall {
        id: Some(id.to_string()),
        status: Some("in_progress".to_string()),
        name: Some("get_weather".to_string()),
        call_id: Some("call_123".to_string()),
        arguments: args.map(|s| s.to_string()).unwrap_or_default(),
    })
}

fn response_reasoning_item(
    id: &str,
    summary: Vec<ReasoningSummaryPart>,
    encrypted_content: Option<&str>,
    status: Option<String>,
) -> ResponseReasoningItem {
    ResponseReasoningItem {
        id: Some(id.to_string()),
        summary,
        content: Vec::new(),
        encrypted_content: encrypted_content.map(str::to_string),
        status,
    }
}

mod chat_stream;
mod compaction;
mod response_reasoning;
mod response_stream_core;
mod response_tool_stream;
