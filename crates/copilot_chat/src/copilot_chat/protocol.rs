use super::*;

#[derive(Serialize, Deserialize)]
pub struct Request {
    pub n: usize,
    pub stream: bool,
    pub temperature: f32,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function { function: Function },
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoice {
    Auto,
    Required,
    None,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum ChatMessage {
    Assistant {
        content: ChatMessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_opaque: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_text: Option<String>,
    },
    User {
        content: ChatMessageContent,
    },
    System {
        content: String,
    },
    Tool {
        content: ChatMessageContent,
        tool_call_id: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessageContent {
    Plain(String),
    Multipart(Vec<ChatMessagePart>),
}

impl ChatMessageContent {
    pub fn empty() -> Self {
        ChatMessageContent::Multipart(vec![])
    }
}

impl From<Vec<ChatMessagePart>> for ChatMessageContent {
    fn from(mut parts: Vec<ChatMessagePart>) -> Self {
        if let [ChatMessagePart::Text { text }] = parts.as_mut_slice() {
            ChatMessageContent::Plain(std::mem::take(text))
        } else {
            ChatMessageContent::Multipart(parts)
        }
    }
}

impl From<String> for ChatMessageContent {
    fn from(text: String) -> Self {
        ChatMessageContent::Plain(text)
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ToolCall {
    pub id: String,
    #[serde(flatten)]
    pub content: ToolCallContent,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolCallContent {
    Function { function: FunctionContent },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct FunctionContent {
    pub name: String,
    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub struct ResponseEvent {
    pub choices: Vec<ResponseChoice>,
    pub id: String,
    pub usage: Option<Usage>,
}

#[derive(Deserialize, Debug)]
pub struct Usage {
    pub completion_tokens: u64,
    pub prompt_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize)]
pub struct ResponseChoice {
    pub index: Option<usize>,
    pub finish_reason: Option<String>,
    pub delta: Option<ResponseDelta>,
    pub message: Option<ResponseDelta>,
}

#[derive(Debug, Deserialize)]
pub struct ResponseDelta {
    pub content: Option<String>,
    pub role: Option<Role>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCallChunk>,
    pub reasoning_opaque: Option<String>,
    pub reasoning_text: Option<String>,
}
#[derive(Deserialize, Debug, Eq, PartialEq)]
pub struct ToolCallChunk {
    pub index: Option<usize>,
    pub id: Option<String>,
    pub function: Option<FunctionChunk>,
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
pub struct FunctionChunk {
    pub name: Option<String>,
    pub arguments: Option<String>,
    pub thought_signature: Option<String>,
}
