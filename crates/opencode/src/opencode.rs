use anyhow::{Result, anyhow};
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, Method, Request as HttpRequest, RequestBuilderExt,
};
use language_model_core::ReasoningEffort;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

pub const OPENCODE_API_URL: &str = "https://opencode.ai/zen";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ApiProtocol {
    #[default]
    Anthropic,
    OpenAiResponses,
    OpenAiChat,
    Google,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenCodeSubscription {
    Zen,
    Go,
    Free,
}

impl OpenCodeSubscription {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Zen => "Zen",
            Self::Go => "Go",
            Self::Free => "Free",
        }
    }

    pub fn id_prefix(&self) -> &'static str {
        match self {
            Self::Zen => "zen",
            Self::Go => "go",
            Self::Free => "free",
        }
    }

    pub fn api_path_suffix(&self) -> &'static str {
        match self {
            Self::Zen | Self::Free => "",
            Self::Go => "/go",
        }
    }
}

#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, EnumIter)]
pub enum Model {
    // -- Anthropic protocol models --
    #[serde(rename = "claude-opus-4-8")]
    ClaudeOpus4_8,
    #[serde(rename = "claude-opus-4-7")]
    ClaudeOpus4_7,
    #[serde(rename = "claude-opus-4-6")]
    ClaudeOpus4_6,
    #[serde(rename = "claude-opus-4-5")]
    ClaudeOpus4_5,
    #[serde(rename = "claude-opus-4-1")]
    ClaudeOpus4_1,
    #[default]
    #[serde(rename = "claude-sonnet-4-6")]
    ClaudeSonnet4_6,
    #[serde(rename = "claude-sonnet-4-5")]
    ClaudeSonnet4_5,
    #[serde(rename = "claude-sonnet-4")]
    ClaudeSonnet4,
    #[serde(rename = "claude-haiku-4-5")]
    ClaudeHaiku4_5,

    // -- OpenAI Responses API models --
    #[serde(rename = "gpt-5.5")]
    Gpt5_5,
    #[serde(rename = "gpt-5.5-pro")]
    Gpt5_5Pro,
    #[serde(rename = "gpt-5.4")]
    Gpt5_4,
    #[serde(rename = "gpt-5.4-pro")]
    Gpt5_4Pro,
    #[serde(rename = "gpt-5.4-mini")]
    Gpt5_4Mini,
    #[serde(rename = "gpt-5.4-nano")]
    Gpt5_4Nano,
    #[serde(rename = "gpt-5.3-codex")]
    Gpt5_3Codex,
    #[serde(rename = "gpt-5.3-codex-spark")]
    Gpt5_3Spark,
    #[serde(rename = "gpt-5.2")]
    Gpt5_2,
    #[serde(rename = "gpt-5.2-codex")]
    Gpt5_2Codex,
    #[serde(rename = "gpt-5.1")]
    Gpt5_1,
    #[serde(rename = "gpt-5.1-codex")]
    Gpt5_1Codex,
    #[serde(rename = "gpt-5.1-codex-max")]
    Gpt5_1CodexMax,
    #[serde(rename = "gpt-5.1-codex-mini")]
    Gpt5_1CodexMini,
    #[serde(rename = "gpt-5")]
    Gpt5,
    #[serde(rename = "gpt-5-codex")]
    Gpt5Codex,
    #[serde(rename = "gpt-5-nano")]
    Gpt5Nano,

    // -- Google protocol models --
    #[serde(rename = "gemini-3.1-pro")]
    Gemini3_1Pro,
    #[serde(rename = "gemini-3-flash")]
    Gemini3Flash,
    #[serde(rename = "gemini-3.5-flash")]
    Gemini3_5Flash,

    // -- OpenAI Chat Completions protocol models --
    #[serde(rename = "deepseek-v4-pro")]
    DeepSeekV4Pro,
    #[serde(rename = "deepseek-v4-flash")]
    DeepSeekV4Flash,
    #[serde(rename = "minimax-m2.5")]
    MiniMaxM2_5,
    #[serde(rename = "glm-5")]
    Glm5,
    #[serde(rename = "glm-5.1")]
    Glm5_1,
    #[serde(rename = "glm-5.2")]
    Glm5_2,
    #[serde(rename = "grok-build-0.1")]
    GrokBuild0_1,
    #[serde(rename = "kimi-k2.5")]
    KimiK2_5,
    #[serde(rename = "kimi-k2.6")]
    KimiK2_6,
    #[serde(rename = "kimi-k2.7-code")]
    KimiK2_7Code,
    #[serde(rename = "minimax-m2.7")]
    MiniMaxM2_7,
    #[serde(rename = "minimax-m3")]
    MiniMaxM3,
    #[serde(rename = "mimo-v2.5-pro")]
    MimoV2_5Pro,
    #[serde(rename = "mimo-v2.5")]
    MimoV2_5,
    #[serde(rename = "big-pickle")]
    BigPickle,
    #[serde(rename = "nemotron-3-ultra-free")]
    Nemotron3UltraFree,
    #[serde(rename = "qwen3.5-plus")]
    Qwen3_5Plus,
    #[serde(rename = "qwen3.6-plus")]
    Qwen3_6Plus,
    #[serde(rename = "qwen3.7-plus")]
    Qwen3_7Plus,
    #[serde(rename = "qwen3.7-max")]
    Qwen3_7Max,

    // -- Custom model --
    #[serde(rename = "custom")]
    Custom {
        name: String,
        display_name: Option<String>,
        max_tokens: u64,
        max_output_tokens: Option<u64>,
        protocol: ApiProtocol,
        reasoning_effort_levels: Option<Vec<ReasoningEffort>>,
        custom_model_api_url: Option<String>,
        interleaved_reasoning: bool,
    },
}

#[path = "opencode/model_capabilities.rs"]
mod model_capabilities;
#[path = "opencode/model_identity.rs"]
mod model_identity;

/// Stream generate content for Google models via OpenCode.
///
/// Unlike `google_ai::stream_generate_content()`, this uses:
/// - `/v1/models/{model}` path (not `/v1beta/models/{model}`)
/// - `Authorization: Bearer` header (not `key=` query param)
pub async fn stream_generate_content(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: &str,
    request: google_ai::GenerateContentRequest,
    extra_headers: &CustomHeaders,
) -> Result<BoxStream<'static, Result<google_ai::GenerateContentResponse>>> {
    let api_key = api_key.trim();

    let model_id = &request.model.model_id;

    let uri = format!("{api_url}/v1/models/{model_id}:streamGenerateContent?alt=sse");

    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {api_key}"))
        .extra_headers(extra_headers)
        .body(AsyncBody::from(serde_json::to_string(&request)?))?;
    let mut response = client.send(request).await?;
    if response.status().is_success() {
        let reader = BufReader::new(response.into_body());
        Ok(reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => {
                        if let Some(line) = line.strip_prefix("data: ") {
                            match serde_json::from_str(line) {
                                Ok(response) => Some(Ok(response)),
                                Err(error) => {
                                    Some(Err(anyhow!("Error parsing JSON: {error:?}\n{line:?}")))
                                }
                            }
                        } else {
                            None
                        }
                    }
                    Err(error) => Some(Err(anyhow!(error))),
                }
            })
            .boxed())
    } else {
        let mut text = String::new();
        response.body_mut().read_to_string(&mut text).await?;
        Err(anyhow!(
            "error during streamGenerateContent via OpenCode, status code: {:?}, body: {}",
            response.status(),
            text
        ))
    }
}
