pub mod responses;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Context as _;
use anyhow::{Result, anyhow};
use collections::HashSet;
use fs::Fs;
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use gpui::TaskExt;
use gpui::WeakEntity;
use gpui::{App, AsyncApp, Global, prelude::*};
use http_client::HttpRequestExt;
use http_client::{AsyncBody, HttpClient, Method, Request as HttpRequest};
use paths::home_dir;
use serde::{Deserialize, Serialize};

#[path = "copilot_chat/api.rs"]
mod api;
#[path = "copilot_chat/auth.rs"]
mod auth;
#[path = "copilot_chat/model.rs"]
mod model;
#[path = "copilot_chat/protocol.rs"]
mod protocol;
#[path = "copilot_chat/streams.rs"]
mod streams;

#[cfg(test)]
#[path = "copilot_chat/tests.rs"]
mod tests;

use api::{discover_api_endpoint, get_models};
use auth::read_oauth_token;
use streams::{stream_completion, stream_messages};

pub(crate) use api::copilot_request_headers;
pub(crate) use model::ModelSchema;
pub use model::{
    ChatLocation, ChatMessagePart, ImageUrl, Model, ModelSupportedEndpoint, ModelVendor, Role,
};
pub use protocol::{
    ChatMessage, ChatMessageContent, Function, FunctionChunk, FunctionContent, Request,
    ResponseChoice, ResponseDelta, ResponseEvent, Tool, ToolCall, ToolCallChunk, ToolCallContent,
    ToolChoice, Usage,
};

// The Copilot language server unofficially supports both token env vars:
// https://github.com/github/copilot-language-server-release/issues/3#issuecomment-2699433055
pub const COPILOT_OAUTH_ENV_VAR: &str = "GH_COPILOT_TOKEN";
pub const GITHUB_COPILOT_OAUTH_ENV_VAR: &str = "GITHUB_COPILOT_TOKEN";
const DEFAULT_COPILOT_API_ENDPOINT: &str = "https://api.githubcopilot.com";

#[derive(Default, Clone, Debug, PartialEq)]
pub struct CopilotChatConfiguration {
    pub enterprise_uri: Option<String>,
}

impl CopilotChatConfiguration {
    pub fn oauth_domain(&self) -> String {
        if let Some(enterprise_uri) = &self.enterprise_uri {
            Self::parse_domain(enterprise_uri)
        } else {
            "github.com".to_string()
        }
    }

    pub fn graphql_url(&self) -> String {
        if let Some(enterprise_uri) = &self.enterprise_uri {
            let domain = Self::parse_domain(enterprise_uri);
            format!("https://{}/api/graphql", domain)
        } else {
            "https://api.github.com/graphql".to_string()
        }
    }

    pub fn chat_completions_url(&self, api_endpoint: &str) -> String {
        format!("{}/chat/completions", api_endpoint)
    }

    pub fn responses_url(&self, api_endpoint: &str) -> String {
        format!("{}/responses", api_endpoint)
    }

    pub fn messages_url(&self, api_endpoint: &str) -> String {
        format!("{}/v1/messages", api_endpoint)
    }

    pub fn models_url(&self, api_endpoint: &str) -> String {
        format!("{}/models", api_endpoint)
    }

    fn parse_domain(enterprise_uri: &str) -> String {
        let uri = enterprise_uri.trim_end_matches('/');

        if let Some(domain) = uri.strip_prefix("https://") {
            domain.split('/').next().unwrap_or(domain).to_string()
        } else if let Some(domain) = uri.strip_prefix("http://") {
            domain.split('/').next().unwrap_or(domain).to_string()
        } else {
            uri.split('/').next().unwrap_or(uri).to_string()
        }
    }
}

struct GlobalCopilotChat(gpui::Entity<CopilotChat>);

impl Global for GlobalCopilotChat {}

pub struct CopilotChat {
    oauth_token: Option<String>,
    api_endpoint: Option<String>,
    configuration: CopilotChatConfiguration,
    models: Option<Vec<Model>>,
    client: Arc<dyn HttpClient>,
    fs: Arc<dyn Fs>,
}

pub fn init(
    fs: Arc<dyn Fs>,
    client: Arc<dyn HttpClient>,
    configuration: CopilotChatConfiguration,
    cx: &mut App,
) {
    let copilot_chat = cx.new(|cx| CopilotChat::new(fs, client, configuration, cx));
    cx.set_global(GlobalCopilotChat(copilot_chat));
}

pub fn copilot_chat_config_dir() -> &'static PathBuf {
    static COPILOT_CHAT_CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

    COPILOT_CHAT_CONFIG_DIR.get_or_init(|| {
        let config_dir = if cfg!(target_os = "windows") {
            dirs::data_local_dir().expect("failed to determine LocalAppData directory")
        } else {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir().join(".config"))
        };

        config_dir.join("github-copilot")
    })
}

/// Legacy JSON token-storage paths used by older Copilot SDK builds.
/// TODO(copilot): once Copilot SDK supports `auth.db`, remove these paths.
fn copilot_chat_config_paths() -> [PathBuf; 2] {
    let base_dir = copilot_chat_config_dir();
    [base_dir.join("hosts.json"), base_dir.join("apps.json")]
}

fn oauth_token_from_env() -> Option<String> {
    std::env::var(COPILOT_OAUTH_ENV_VAR)
        .ok()
        .or_else(|| std::env::var(GITHUB_COPILOT_OAUTH_ENV_VAR).ok())
}

impl CopilotChat {
    pub fn global(cx: &App) -> Option<gpui::Entity<Self>> {
        cx.try_global::<GlobalCopilotChat>()
            .map(|model| model.0.clone())
    }

    fn new(
        fs: Arc<dyn Fs>,
        client: Arc<dyn HttpClient>,
        configuration: CopilotChatConfiguration,
        cx: &mut Context<Self>,
    ) -> Self {
        // Initial async scan of token sources. Live reload is driven by the
        // Copilot LSP's auth status notifications instead of watching files,
        // because SQLite WAL writes can make directory watchers racy.
        cx.spawn({
            let fs = fs.clone();
            async move |this, cx| {
                let oauth_domain =
                    this.read_with(cx, |this, _| this.configuration.oauth_domain())?;
                let config_paths: HashSet<PathBuf> =
                    copilot_chat_config_paths().into_iter().collect();
                let auth_db_path = copilot_chat_config_dir().join("auth.db");

                let oauth_token =
                    read_oauth_token(&fs, &config_paths, &oauth_domain, &auth_db_path, cx).await;

                if oauth_token.is_some() {
                    this.update(cx, |this, cx| {
                        this.oauth_token = oauth_token;
                        cx.notify();
                    })?;
                    Self::update_models(&this, cx).await?;
                }
                anyhow::Ok(())
            }
        })
        .detach_and_log_err(cx);

        // Initial state uses env var because it's cheap. The others do IO, so
        // are on the background.
        let this = Self {
            oauth_token: oauth_token_from_env(),
            api_endpoint: None,
            models: None,
            configuration,
            client,
            fs,
        };

        if this.oauth_token.is_some() {
            cx.spawn(async move |this, cx| Self::update_models(&this, cx).await)
                .detach_and_log_err(cx);
        }

        this
    }

    async fn update_models(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> Result<()> {
        let (oauth_token, client, configuration) = this.read_with(cx, |this, _| {
            (
                this.oauth_token.clone(),
                this.client.clone(),
                this.configuration.clone(),
            )
        })?;

        let oauth_token = oauth_token
            .ok_or_else(|| anyhow!("OAuth token is missing while updating Copilot Chat models"))?;

        let api_endpoint =
            Self::resolve_api_endpoint(&this, &oauth_token, &configuration, &client, cx).await?;

        let models_url = configuration.models_url(&api_endpoint);
        let models = get_models(models_url.into(), oauth_token, client.clone()).await?;

        this.update(cx, |this, cx| {
            this.models = Some(models);
            cx.notify();
        })?;
        anyhow::Ok(())
    }

    pub fn is_authenticated(&self) -> bool {
        self.oauth_token.is_some()
    }

    pub fn models(&self) -> Option<&[Model]> {
        self.models.as_deref()
    }

    pub async fn stream_completion(
        request: Request,
        location: ChatLocation,
        is_user_initiated: bool,
        mut cx: AsyncApp,
    ) -> Result<BoxStream<'static, Result<ResponseEvent>>> {
        let (client, oauth_token, api_endpoint, configuration) =
            Self::get_auth_details(&mut cx).await?;

        let api_url = configuration.chat_completions_url(&api_endpoint);
        stream_completion(
            client.clone(),
            oauth_token,
            api_url.into(),
            request,
            is_user_initiated,
            location,
        )
        .await
    }

    pub async fn stream_response(
        request: responses::Request,
        location: ChatLocation,
        is_user_initiated: bool,
        mut cx: AsyncApp,
    ) -> Result<BoxStream<'static, Result<responses::StreamEvent>>> {
        let (client, oauth_token, api_endpoint, configuration) =
            Self::get_auth_details(&mut cx).await?;

        let api_url = configuration.responses_url(&api_endpoint);
        responses::stream_response(
            client.clone(),
            oauth_token,
            api_url,
            request,
            is_user_initiated,
            location,
        )
        .await
    }

    pub async fn stream_messages(
        body: String,
        location: ChatLocation,
        is_user_initiated: bool,
        anthropic_beta: Option<String>,
        mut cx: AsyncApp,
    ) -> Result<BoxStream<'static, Result<anthropic::Event, anthropic::AnthropicError>>> {
        let (client, oauth_token, api_endpoint, configuration) =
            Self::get_auth_details(&mut cx).await?;

        let api_url = configuration.messages_url(&api_endpoint);
        stream_messages(
            client.clone(),
            oauth_token,
            api_url,
            body,
            is_user_initiated,
            location,
            anthropic_beta,
        )
        .await
    }

    async fn get_auth_details(
        cx: &mut AsyncApp,
    ) -> Result<(
        Arc<dyn HttpClient>,
        String,
        String,
        CopilotChatConfiguration,
    )> {
        let this = cx
            .update(|cx| Self::global(cx))
            .context("Copilot chat is not enabled")?;

        let (oauth_token, api_endpoint, client, configuration) = this.read_with(cx, |this, _| {
            (
                this.oauth_token.clone(),
                this.api_endpoint.clone(),
                this.client.clone(),
                this.configuration.clone(),
            )
        });

        let oauth_token = oauth_token.context("No OAuth token available")?;

        let api_endpoint = match api_endpoint {
            Some(endpoint) => endpoint,
            None => {
                let weak = this.downgrade();
                Self::resolve_api_endpoint(&weak, &oauth_token, &configuration, &client, cx).await?
            }
        };

        Ok((client, oauth_token, api_endpoint, configuration))
    }

    async fn resolve_api_endpoint(
        this: &WeakEntity<Self>,
        oauth_token: &str,
        configuration: &CopilotChatConfiguration,
        client: &Arc<dyn HttpClient>,
        cx: &mut AsyncApp,
    ) -> Result<String> {
        let api_endpoint = match discover_api_endpoint(oauth_token, configuration, client).await {
            Ok(endpoint) => endpoint,
            Err(error) => {
                log::warn!(
                    "Failed to discover Copilot API endpoint via GraphQL, \
                         falling back to {DEFAULT_COPILOT_API_ENDPOINT}: {error:#}"
                );
                DEFAULT_COPILOT_API_ENDPOINT.to_string()
            }
        };

        this.update(cx, |this, cx| {
            this.api_endpoint = Some(api_endpoint.clone());
            cx.notify();
        })?;

        Ok(api_endpoint)
    }

    pub fn set_configuration(
        &mut self,
        configuration: CopilotChatConfiguration,
        cx: &mut Context<Self>,
    ) {
        let same_configuration = self.configuration == configuration;
        self.configuration = configuration;
        if !same_configuration {
            self.api_endpoint = None;
            cx.spawn(async move |this, cx| {
                Self::update_models(&this, cx).await?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
        }
    }

    pub fn reload_auth(&mut self, cx: &mut Context<Self>) {
        let fs = self.fs.clone();
        let oauth_domain = self.configuration.oauth_domain();
        cx.spawn(async move |this, cx| {
            let config_paths: HashSet<PathBuf> = copilot_chat_config_paths().into_iter().collect();
            let auth_db_path = copilot_chat_config_dir().join("auth.db");

            let new_token =
                read_oauth_token(&fs, &config_paths, &oauth_domain, &auth_db_path, cx).await;

            let token_present = this.update(cx, |this, cx| {
                let changed = this.oauth_token != new_token;
                if changed {
                    this.oauth_token = new_token.clone();
                    if new_token.is_none() {
                        // Sign-out: drop derived state so a future sign-in
                        // re-discovers the endpoint and re-fetches models.
                        this.api_endpoint = None;
                        this.models = None;
                    }
                    cx.notify();
                }
                new_token.is_some()
            })?;

            if token_present {
                Self::update_models(&this, cx).await?;
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }
}
