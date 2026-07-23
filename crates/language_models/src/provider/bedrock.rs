use std::pin::Pin;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use async_lock::OnceCell;
use aws_config::stalled_stream_protection::StalledStreamProtectionConfig;
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::{Credentials, Token};
use aws_http_client::AwsHttpClient;
use bedrock::BedrockSystemContentBlock;
use bedrock::bedrock_client::Client as BedrockClient;
use bedrock::bedrock_client::config::timeout::TimeoutConfig;
use bedrock::bedrock_client::types::{
    CachePointBlock, CachePointType, ContentBlockDelta, ContentBlockStart, ConverseStreamOutput,
    ReasoningContentBlockDelta, StopReason,
};
use bedrock::{
    BedrockAnyToolChoice, BedrockAutoToolChoice, BedrockBlob, BedrockError, BedrockImageBlock,
    BedrockImageFormat, BedrockImageSource, BedrockInnerContent, BedrockMessage, BedrockModelMode,
    BedrockStreamingResponse, BedrockThinkingBlock, BedrockThinkingTextBlock, BedrockTool,
    BedrockToolChoice, BedrockToolConfig, BedrockToolInputSchema, BedrockToolResultBlock,
    BedrockToolResultContentBlock, BedrockToolResultStatus, BedrockToolSpec, BedrockToolUseBlock,
    Model, value_to_aws_document,
};
use collections::{BTreeMap, HashMap};
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, Stream, StreamExt, future::BoxFuture, stream::BoxStream};
use gpui::{
    AnyView, App, AsyncApp, Context, Entity, FocusHandle, Subscription, Task, TaskExt, Window,
    actions,
};
use gpui_tokio::Tokio;
use http_client::HttpClient;
use language_model::{
    AuthenticateError, EnvVar, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelId, LanguageModelName, LanguageModelProvider,
    LanguageModelProviderId, LanguageModelProviderName, LanguageModelProviderState,
    LanguageModelRequest, LanguageModelToolChoice, LanguageModelToolResultContent,
    LanguageModelToolUse, MessageContent, RateLimiter, Role, TokenUsage, env_var,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use settings::{BedrockAvailableModel as AvailableModel, Settings, SettingsStore};
use std::sync::LazyLock;
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};
use ui::{ButtonLink, ConfiguredApiCard, Divider, List, ListBulletItem, prelude::*};
use ui_input::InputField;
use util::ResultExt;

use crate::AllLanguageModelSettings;
use http_client::CustomHeaders;
use language_model::util::{fix_streamed_json, parse_tool_arguments};

actions!(bedrock, [Tab, TabPrev]);

mod configuration_view;
use configuration_view::ConfigurationView;

mod model;
use model::BedrockModel;

mod provider;
pub(crate) use provider::BedrockLanguageModelProvider;

mod request;
use request::{deny_tool_use_events, into_bedrock};

mod stream_mapper;
use stream_mapper::map_to_language_model_completion_events;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("amazon-bedrock");
const PROVIDER_NAME: LanguageModelProviderName = LanguageModelProviderName::new("Amazon Bedrock");
pub(crate) const RESERVED_HEADER_NAMES: &[&str] = &[
    "host",
    "x-amz-date",
    "x-amz-security-token",
    "x-amz-content-sha256",
    "amz-sdk-invocation-id",
    "amz-sdk-request",
];

/// Credentials stored in the keychain for static authentication.
/// Region is handled separately since it's orthogonal to auth method.
#[derive(Default, Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct BedrockCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub bearer_token: Option<String>,
}

/// Resolved authentication configuration for Bedrock.
/// Settings take priority over UX-provided credentials.
#[derive(Clone, Debug, PartialEq)]
pub enum BedrockAuth {
    /// Use default AWS credential provider chain (IMDSv2, PodIdentity, env vars, etc.)
    Automatic,
    /// Use AWS named profile from ~/.aws/credentials or ~/.aws/config
    NamedProfile { profile_name: String },
    /// Use AWS SSO profile
    SingleSignOn { profile_name: String },
    /// Use IAM credentials (access key + secret + optional session token)
    IamCredentials {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
    },
    /// Use Bedrock API Key (bearer token authentication)
    ApiKey { api_key: String },
}

impl BedrockCredentials {
    /// Convert stored credentials to the appropriate auth variant.
    /// Prefers API key if present, otherwise uses IAM credentials.
    fn into_auth(self) -> Option<BedrockAuth> {
        if let Some(api_key) = self.bearer_token.filter(|t| !t.is_empty()) {
            Some(BedrockAuth::ApiKey { api_key })
        } else if !self.access_key_id.is_empty() && !self.secret_access_key.is_empty() {
            Some(BedrockAuth::IamCredentials {
                access_key_id: self.access_key_id,
                secret_access_key: self.secret_access_key,
                session_token: self.session_token.filter(|t| !t.is_empty()),
            })
        } else {
            None
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct AmazonBedrockSettings {
    pub available_models: Vec<AvailableModel>,
    pub custom_headers: CustomHeaders,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub profile_name: Option<String>,
    pub role_arn: Option<String>,
    pub authentication_method: Option<BedrockAuthMethod>,
    pub allow_global: Option<bool>,
    pub guardrail_identifier: Option<String>,
    pub guardrail_version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, EnumIter, IntoStaticStr, JsonSchema)]
pub enum BedrockAuthMethod {
    #[serde(rename = "named_profile")]
    NamedProfile,
    #[serde(rename = "sso")]
    SingleSignOn,
    #[serde(rename = "api_key")]
    ApiKey,
    /// IMDSv2, PodIdentity, env vars, etc.
    #[serde(rename = "default")]
    Automatic,
}

impl From<settings::BedrockAuthMethodContent> for BedrockAuthMethod {
    fn from(value: settings::BedrockAuthMethodContent) -> Self {
        match value {
            settings::BedrockAuthMethodContent::SingleSignOn => BedrockAuthMethod::SingleSignOn,
            settings::BedrockAuthMethodContent::Automatic => BedrockAuthMethod::Automatic,
            settings::BedrockAuthMethodContent::NamedProfile => BedrockAuthMethod::NamedProfile,
            settings::BedrockAuthMethodContent::ApiKey => BedrockAuthMethod::ApiKey,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ModelMode {
    #[default]
    Default,
    Thinking {
        /// The maximum number of tokens to use for reasoning. Must be lower than the model's `max_output_tokens`.
        budget_tokens: Option<u64>,
    },
    AdaptiveThinking {
        effort: bedrock::BedrockAdaptiveThinkingEffort,
    },
}

impl From<ModelMode> for BedrockModelMode {
    fn from(value: ModelMode) -> Self {
        match value {
            ModelMode::Default => BedrockModelMode::Default,
            ModelMode::Thinking { budget_tokens } => BedrockModelMode::Thinking { budget_tokens },
            ModelMode::AdaptiveThinking { effort } => BedrockModelMode::AdaptiveThinking { effort },
        }
    }
}

impl From<BedrockModelMode> for ModelMode {
    fn from(value: BedrockModelMode) -> Self {
        match value {
            BedrockModelMode::Default => ModelMode::Default,
            BedrockModelMode::Thinking { budget_tokens } => ModelMode::Thinking { budget_tokens },
            BedrockModelMode::AdaptiveThinking { effort } => ModelMode::AdaptiveThinking { effort },
        }
    }
}

/// The URL of the base AWS service.
///
/// Right now we're just using this as the key to store the AWS credentials
/// under in the keychain.
const AMAZON_AWS_URL: &str = "https://amazonaws.com";

// These environment variables all use a `MAV_` prefix because we don't want to overwrite the user's AWS credentials.
static MAV_BEDROCK_ACCESS_KEY_ID_VAR: LazyLock<EnvVar> = env_var!("MAV_ACCESS_KEY_ID");
static MAV_BEDROCK_SECRET_ACCESS_KEY_VAR: LazyLock<EnvVar> = env_var!("MAV_SECRET_ACCESS_KEY");
static MAV_BEDROCK_SESSION_TOKEN_VAR: LazyLock<EnvVar> = env_var!("MAV_SESSION_TOKEN");
static MAV_AWS_PROFILE_VAR: LazyLock<EnvVar> = env_var!("MAV_AWS_PROFILE");
static MAV_BEDROCK_REGION_VAR: LazyLock<EnvVar> = env_var!("MAV_AWS_REGION");
static MAV_AWS_ENDPOINT_VAR: LazyLock<EnvVar> = env_var!("MAV_AWS_ENDPOINT");
static MAV_BEDROCK_BEARER_TOKEN_VAR: LazyLock<EnvVar> = env_var!("MAV_BEDROCK_BEARER_TOKEN");

pub struct State {
    /// The resolved authentication method. Settings take priority over UX credentials.
    auth: Option<BedrockAuth>,
    /// Raw settings from settings.json
    settings: Option<AmazonBedrockSettings>,
    /// Whether credentials came from environment variables (only relevant for static credentials)
    credentials_from_env: bool,
    credentials_provider: Arc<dyn CredentialsProvider>,
    _subscription: Subscription,
}
