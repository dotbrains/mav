use super::*;

#[derive(Clone, Copy, Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ChatLocation {
    #[default]
    Panel,
    Editor,
    EditingSession,
    Terminal,
    Agent,
    Other,
}

impl ChatLocation {
    pub fn to_intent_string(self) -> &'static str {
        match self {
            ChatLocation::Panel => "conversation-panel",
            ChatLocation::Editor => "conversation-inline",
            ChatLocation::EditingSession => "conversation-edits",
            ChatLocation::Terminal => "conversation-terminal",
            ChatLocation::Agent => "conversation-agent",
            ChatLocation::Other => "conversation-other",
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
pub enum ModelSupportedEndpoint {
    #[serde(rename = "/chat/completions")]
    ChatCompletions,
    #[serde(rename = "/responses")]
    Responses,
    #[serde(rename = "/v1/messages")]
    Messages,
    /// Unknown endpoint that we don't explicitly support yet
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize)]
pub(crate) struct ModelSchema {
    #[serde(deserialize_with = "deserialize_models_skip_errors")]
    pub(crate) data: Vec<Model>,
}

fn deserialize_models_skip_errors<'de, D>(deserializer: D) -> Result<Vec<Model>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw_values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let models = raw_values
        .into_iter()
        .filter_map(|value| match serde_json::from_value::<Model>(value) {
            Ok(model) => Some(model),
            Err(err) => {
                log::warn!("GitHub Copilot Chat model failed to deserialize: {:?}", err);
                None
            }
        })
        .collect();

    Ok(models)
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Model {
    pub(crate) billing: ModelBilling,
    pub(crate) capabilities: ModelCapabilities,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) policy: Option<ModelPolicy>,
    pub(crate) vendor: ModelVendor,
    pub(crate) is_chat_default: bool,
    // The model with this value true is selected by VSCode copilot if a premium request limit is
    // reached. Mav does not currently implement this behaviour
    pub(crate) is_chat_fallback: bool,
    pub(crate) model_picker_enabled: bool,
    #[serde(default)]
    pub(crate) supported_endpoints: Vec<ModelSupportedEndpoint>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ModelBilling {
    pub(crate) is_premium: bool,
    pub(crate) multiplier: f64,
    // List of plans a model is restricted to
    // Field is not present if a model is available for all plans
    #[serde(default)]
    pub(crate) restricted_to: Option<Vec<String>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ModelCapabilities {
    pub(crate) family: String,
    #[serde(default)]
    pub(crate) limits: ModelLimits,
    pub(crate) supports: ModelSupportedFeatures,
    #[serde(rename = "type")]
    pub(crate) model_type: String,
    #[serde(default)]
    pub(crate) tokenizer: Option<String>,
}

#[derive(Default, Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ModelLimits {
    #[serde(default)]
    pub(crate) max_context_window_tokens: usize,
    #[serde(default)]
    pub(crate) max_output_tokens: usize,
    #[serde(default)]
    pub(crate) max_prompt_tokens: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ModelPolicy {
    pub(crate) state: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct ModelSupportedFeatures {
    #[serde(default)]
    pub(crate) streaming: bool,
    #[serde(default)]
    pub(crate) tool_calls: bool,
    #[serde(default)]
    pub(crate) parallel_tool_calls: bool,
    #[serde(default)]
    pub(crate) vision: bool,
    #[serde(default)]
    pub(crate) thinking: bool,
    #[serde(default)]
    pub(crate) adaptive_thinking: bool,
    #[serde(default)]
    pub(crate) max_thinking_budget: Option<u32>,
    #[serde(default)]
    pub(crate) min_thinking_budget: Option<u32>,
    #[serde(default)]
    pub(crate) reasoning_effort: Vec<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub enum ModelVendor {
    // Azure OpenAI should have no functional difference from OpenAI in Copilot Chat
    #[serde(alias = "Azure OpenAI")]
    OpenAI,
    Google,
    Anthropic,
    #[serde(rename = "xAI")]
    XAI,
    /// Unknown vendor that we don't explicitly support yet
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type")]
pub enum ChatMessagePart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ImageUrl {
    pub url: String,
}

impl Model {
    pub fn uses_streaming(&self) -> bool {
        self.capabilities.supports.streaming
    }

    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn display_name(&self) -> &str {
        self.name.as_str()
    }

    pub fn max_token_count(&self) -> u64 {
        self.capabilities.limits.max_context_window_tokens as u64
    }

    pub fn max_output_tokens(&self) -> usize {
        self.capabilities.limits.max_output_tokens
    }

    pub fn supports_tools(&self) -> bool {
        self.capabilities.supports.tool_calls
    }

    pub fn vendor(&self) -> ModelVendor {
        self.vendor
    }

    pub fn supports_vision(&self) -> bool {
        self.capabilities.supports.vision
    }

    pub fn supports_parallel_tool_calls(&self) -> bool {
        self.capabilities.supports.parallel_tool_calls
    }

    pub fn tokenizer(&self) -> Option<&str> {
        self.capabilities.tokenizer.as_deref()
    }

    pub fn supports_response(&self) -> bool {
        self.supported_endpoints
            .contains(&ModelSupportedEndpoint::Responses)
    }

    pub fn supports_messages(&self) -> bool {
        self.supported_endpoints
            .contains(&ModelSupportedEndpoint::Messages)
    }

    pub fn supports_thinking(&self) -> bool {
        self.capabilities.supports.thinking
    }

    pub fn supports_adaptive_thinking(&self) -> bool {
        self.capabilities.supports.adaptive_thinking
    }

    pub fn can_think(&self) -> bool {
        self.supports_thinking()
            || self.supports_adaptive_thinking()
            || self.max_thinking_budget().is_some()
            || !self.reasoning_effort_levels().is_empty()
    }

    pub fn max_thinking_budget(&self) -> Option<u32> {
        self.capabilities.supports.max_thinking_budget
    }

    pub fn min_thinking_budget(&self) -> Option<u32> {
        self.capabilities.supports.min_thinking_budget
    }

    pub fn reasoning_effort_levels(&self) -> &[String] {
        &self.capabilities.supports.reasoning_effort
    }

    pub fn family(&self) -> &str {
        &self.capabilities.family
    }

    pub fn multiplier(&self) -> f64 {
        self.billing.multiplier
    }
}
