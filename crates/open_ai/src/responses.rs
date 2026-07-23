use anyhow::{Result, anyhow};
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, Method, Request as HttpRequest, RequestBuilderExt,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use crate::{ReasoningEffort, RequestError, Role, ServiceTier, ToolChoice};

#[derive(Serialize, Debug)]
pub struct Request {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub input: Vec<ResponseInputItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<ResponseIncludable>,
    #[serde(default)]
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<Vec<ContextManagement>>,
}

/// Server-side context management configuration.
///
/// <https://developers.openai.com/api/docs/guides/compaction>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContextManagement {
    Compaction { compact_threshold: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseIncludable {
    #[serde(rename = "reasoning.encrypted_content")]
    ReasoningEncryptedContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseInputItem {
    Message(ResponseMessageItem),
    FunctionCall(ResponseFunctionCallItem),
    FunctionCallOutput(ResponseFunctionCallOutputItem),
    Reasoning(ResponseReasoningInputItem),
    Compaction(ResponseCompactionItem),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseCompactionItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Arc<str>>,
    pub encrypted_content: Arc<str>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMessageItem {
    pub role: Role,
    pub content: Vec<ResponseInputContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseFunctionCallItem {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseFunctionCallOutputItem {
    pub call_id: String,
    pub output: ResponseFunctionCallOutputContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResponseReasoningInputItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Vec<ResponseReasoningSummaryPart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseReasoningSummaryPart {
    SummaryText { text: String },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFunctionCallOutputContent {
    List(Vec<ResponseInputContent>),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseInputContent {
    #[serde(rename = "input_text")]
    Text { text: String },
    #[serde(rename = "input_image")]
    Image { image_url: String },
    #[serde(rename = "output_text")]
    OutputText {
        text: String,
        #[serde(default)]
        annotations: Vec<serde_json::Value>,
    },
    #[serde(rename = "refusal")]
    Refusal { refusal: String },
}

#[derive(Serialize, Debug)]
pub struct ReasoningConfig {
    pub effort: ReasoningEffort,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummaryMode>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningSummaryMode {
    Auto,
    Concise,
    Detailed,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolDefinition {
    Function {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseError {
    #[serde(default)]
    pub code: Option<String>,
    pub message: String,
    #[serde(default)]
    pub param: Option<Value>,
}

/// Payload of the top-level `error` SSE event from the Responses API.
///
/// OpenAI's spec documents the error fields as being at the top level of the
/// event, but in practice the API often nests them under an `error` object.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct GenericStreamErrorPayload {
    #[serde(flatten)]
    top_level: PartialResponseError,
    #[serde(default)]
    error: Option<PartialResponseError>,
}

#[derive(Deserialize, Debug, Clone, Default)]
struct PartialResponseError {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    param: Option<Value>,
}

impl GenericStreamErrorPayload {
    pub fn into_response_error(self) -> ResponseError {
        let nested = self.error.unwrap_or_default();
        ResponseError {
            code: self.top_level.code.or(nested.code),
            message: self
                .top_level
                .message
                .or(nested.message)
                .unwrap_or_default(),
            param: self.top_level.param.or(nested.param),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "response.created")]
    Created { response: ResponseSummary },
    #[serde(rename = "response.in_progress")]
    InProgress { response: ResponseSummary },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: usize,
        #[serde(default)]
        sequence_number: Option<u64>,
        item: ResponseOutputItem,
    },
    #[serde(rename = "response.output_item.done")]
    OutputItemDone {
        output_index: usize,
        #[serde(default)]
        sequence_number: Option<u64>,
        item: ResponseOutputItem,
    },
    #[serde(rename = "response.content_part.added")]
    ContentPartAdded {
        item_id: String,
        output_index: usize,
        content_index: usize,
        part: Value,
    },
    #[serde(rename = "response.content_part.done")]
    ContentPartDone {
        item_id: String,
        output_index: usize,
        content_index: usize,
        part: Value,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: usize,
        #[serde(default)]
        content_index: Option<usize>,
        delta: String,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: usize,
        #[serde(default)]
        content_index: Option<usize>,
        text: String,
    },
    #[serde(rename = "response.refusal.delta")]
    RefusalDelta {
        item_id: String,
        output_index: usize,
        content_index: usize,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.refusal.done")]
    RefusalDone {
        item_id: String,
        output_index: usize,
        content_index: usize,
        refusal: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.reasoning_summary_part.added")]
    ReasoningSummaryPartAdded {
        item_id: String,
        output_index: usize,
        summary_index: usize,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummaryTextDelta {
        item_id: String,
        output_index: usize,
        delta: String,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ReasoningSummaryTextDone {
        item_id: String,
        output_index: usize,
        text: String,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ReasoningSummaryPartDone {
        item_id: String,
        output_index: usize,
        summary_index: usize,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        item_id: String,
        output_index: usize,
        delta: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        item_id: String,
        output_index: usize,
        arguments: String,
        #[serde(default)]
        sequence_number: Option<u64>,
    },
    #[serde(rename = "response.completed")]
    Completed { response: ResponseSummary },
    #[serde(rename = "response.incomplete")]
    Incomplete { response: ResponseSummary },
    #[serde(rename = "response.failed")]
    Failed { response: ResponseSummary },
    #[serde(rename = "response.error")]
    Error { error: ResponseError },
    #[serde(rename = "error")]
    GenericError {
        #[serde(flatten)]
        error: GenericStreamErrorPayload,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ResponseSummary {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub incomplete_details: Option<ResponseIncompleteDetails>,
    #[serde(default)]
    pub error: Option<ResponseError>,
    #[serde(default)]
    pub usage: Option<ResponseUsage>,
    #[serde(default)]
    pub output: Vec<ResponseOutputItem>,
    #[serde(default)]
    pub service_tier: Option<crate::ServiceTier>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ResponseIncompleteDetails {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ResponseUsage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub input_tokens_details: ResponseInputTokensDetails,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens_details: ResponseOutputTokensDetails,
    #[serde(default)]
    pub total_tokens: Option<u64>,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ResponseInputTokensDetails {
    #[serde(default)]
    pub cached_tokens: u64,
}

#[derive(Deserialize, Debug, Default, Clone)]
pub struct ResponseOutputTokensDetails {
    #[serde(default)]
    pub reasoning_tokens: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseOutputItem {
    Message(ResponseOutputMessage),
    FunctionCall(ResponseFunctionToolCall),
    Reasoning(ResponseReasoningItem),
    Compaction(ResponseCompactionItem),
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseReasoningItem {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Vec<ReasoningSummaryPart>,
    #[serde(default)]
    pub content: Vec<Value>,
    #[serde(default)]
    pub encrypted_content: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningSummaryPart {
    SummaryText {
        text: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseOutputMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub content: Vec<Value>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseFunctionToolCall {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub arguments: String,
    #[serde(default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

#[path = "responses/streaming.rs"]
mod streaming;

pub use streaming::stream_response;
