use anyhow::{Context as _, Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use credentials_provider::CredentialsProvider;
use futures::{FutureExt, StreamExt, future::BoxFuture, future::Shared};
use gpui::{AnyView, App, AsyncApp, Context, Entity, SharedString, Task, Window};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, Method, Request as HttpRequest,
    http::{HeaderName, HeaderValue},
};
use language_model::{
    AuthenticateError, FastModeConfirmation, IconOrSvg, LanguageModel,
    LanguageModelCompletionError, LanguageModelCompletionEvent, LanguageModelEffortLevel,
    LanguageModelId, LanguageModelName, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelProviderName, LanguageModelProviderState, LanguageModelRequest,
    LanguageModelToolChoice, RateLimiter,
};
use open_ai::{ReasoningEffort, responses::stream_response};
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use ui::{ConfiguredApiCard, prelude::*};
use url::form_urlencoded;
use util::ResultExt as _;

use crate::provider::open_ai::{OpenAiResponseEventMapper, into_open_ai_response};

mod auth;
use auth::get_fresh_credentials;

mod configuration_view;
use configuration_view::ConfigurationView;

mod model;
use model::{ChatGptModel, OpenAiSubscribedLanguageModel};

mod oauth;
use oauth::{do_oauth_flow, now_ms, refresh_token};

mod provider;
pub(crate) use provider::OpenAiSubscribedProvider;

mod sign_in;
use sign_in::{do_sign_in, do_sign_out};

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("openai-subscribed");
const PROVIDER_NAME: LanguageModelProviderName =
    LanguageModelProviderName::new("ChatGPT Subscription");

const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const OPENAI_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

const CREDENTIALS_KEY: &str = "https://chatgpt.com/backend-api/codex";
const TOKEN_REFRESH_BUFFER_MS: u64 = 5 * 60 * 1000;

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CodexCredentials {
    access_token: String,
    refresh_token: String,
    expires_at_ms: u64,
    account_id: Option<String>,
    email: Option<String>,
}

impl CodexCredentials {
    fn is_expired(&self) -> bool {
        let now = now_ms();
        now + TOKEN_REFRESH_BUFFER_MS >= self.expires_at_ms
    }
}

pub struct State {
    credentials: Option<CodexCredentials>,
    sign_in_task: Option<Task<Result<()>>>,
    refresh_task: Option<Shared<Task<Result<CodexCredentials, Arc<anyhow::Error>>>>>,
    load_task: Option<Shared<Task<Result<(), Arc<anyhow::Error>>>>>,
    credentials_provider: Arc<dyn CredentialsProvider>,
    auth_generation: u64,
    last_auth_error: Option<SharedString>,
}

#[derive(Debug)]
enum RefreshError {
    Fatal(anyhow::Error),
    Transient(anyhow::Error),
}

impl std::fmt::Display for RefreshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefreshError::Fatal(e) => write!(f, "{e}"),
            RefreshError::Transient(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(test)]
mod tests;
