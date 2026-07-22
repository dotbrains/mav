use std::pin::Pin;
use std::str::FromStr as _;
use std::sync::Arc;

use anthropic::AnthropicModelMode;
use anyhow::{Result, anyhow};
use collections::HashMap;
use copilot::{GlobalCopilotAuth, Status};
use copilot_chat::responses as copilot_responses;
use copilot_chat::{
    ChatLocation, ChatMessage, ChatMessageContent, ChatMessagePart, CopilotChat,
    CopilotChatConfiguration, Function, FunctionContent, ImageUrl, Model as CopilotChatModel,
    ModelVendor, Request as CopilotChatRequest, ResponseEvent, Tool, ToolCall, ToolCallContent,
    ToolChoice,
};
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, Stream, StreamExt};
use gpui::{AnyView, App, AsyncApp, Entity, Subscription, Task};
use http_client::StatusCode;
use language::language_settings::all_language_settings;
use language_model::{
    AuthenticateError, CompletionIntent, IconOrSvg, LanguageModel, LanguageModelCompletionError,
    LanguageModelCompletionEvent, LanguageModelCostInfo, LanguageModelEffortLevel, LanguageModelId,
    LanguageModelName, LanguageModelProvider, LanguageModelProviderId, LanguageModelProviderName,
    LanguageModelProviderState, LanguageModelRequest, LanguageModelRequestMessage,
    LanguageModelToolChoice, LanguageModelToolResultContent, LanguageModelToolSchemaFormat,
    LanguageModelToolUse, MessageContent, ProviderConfigurationView, RateLimiter, Role, StopReason,
    TokenUsage,
};
use settings::SettingsStore;
use ui::prelude::*;
use util::debug_panic;

use crate::provider::anthropic::{AnthropicEventMapper, AnthropicPromptCacheMode, into_anthropic};
use language_model::util::{fix_streamed_json, parse_tool_arguments};

mod request_conversion;
mod responses_mapper;
mod stream_mapper;

#[cfg(test)]
mod tests;

use request_conversion::{compute_thinking_budget, intent_to_chat_location};
use request_conversion::{into_copilot_chat, into_copilot_responses};
use responses_mapper::CopilotResponsesEventMapper;
use stream_mapper::map_to_language_model_completion_events;

const PROVIDER_ID: LanguageModelProviderId = LanguageModelProviderId::new("copilot_chat");
const PROVIDER_NAME: LanguageModelProviderName =
    LanguageModelProviderName::new("GitHub Copilot Chat");

pub struct CopilotChatLanguageModelProvider {
    state: Entity<State>,
}

pub struct State {
    _copilot_chat_subscription: Option<Subscription>,
    _settings_subscription: Subscription,
}

impl State {
    fn is_authenticated(&self, cx: &App) -> bool {
        CopilotChat::global(cx)
            .map(|m| m.read(cx).is_authenticated())
            .unwrap_or(false)
    }
}

impl CopilotChatLanguageModelProvider {
    pub fn new(cx: &mut App) -> Self {
        let state = cx.new(|cx| {
            let copilot_chat_subscription = CopilotChat::global(cx)
                .map(|copilot_chat| cx.observe(&copilot_chat, |_, _, cx| cx.notify()));
            State {
                _copilot_chat_subscription: copilot_chat_subscription,
                _settings_subscription: cx.observe_global::<SettingsStore>(|_, cx| {
                    if let Some(copilot_chat) = CopilotChat::global(cx) {
                        let language_settings = all_language_settings(None, cx);
                        let configuration = CopilotChatConfiguration {
                            enterprise_uri: language_settings
                                .edit_predictions
                                .copilot
                                .enterprise_uri
                                .clone(),
                        };
                        copilot_chat.update(cx, |chat, cx| {
                            chat.set_configuration(configuration, cx);
                        });
                    }
                    cx.notify();
                }),
            }
        });

        Self { state }
    }

    fn create_language_model(&self, model: CopilotChatModel) -> Arc<dyn LanguageModel> {
        Arc::new(CopilotChatLanguageModel {
            model,
            request_limiter: RateLimiter::new(4),
        })
    }
}

impl LanguageModelProviderState for CopilotChatLanguageModelProvider {
    type ObservableEntity = State;

    fn observable_entity(&self) -> Option<Entity<Self::ObservableEntity>> {
        Some(self.state.clone())
    }
}

impl LanguageModelProvider for CopilotChatLanguageModelProvider {
    fn id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn icon(&self) -> IconOrSvg {
        IconOrSvg::Icon(IconName::Copilot)
    }

    fn default_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        let models = CopilotChat::global(cx).and_then(|m| m.read(cx).models())?;
        models
            .first()
            .map(|model| self.create_language_model(model.clone()))
    }

    fn default_fast_model(&self, cx: &App) -> Option<Arc<dyn LanguageModel>> {
        // The default model should be Copilot Chat's 'base model', which is likely a relatively fast
        // model (e.g. 4o) and a sensible choice when considering premium requests
        self.default_model(cx)
    }

    fn provided_models(&self, cx: &App) -> Vec<Arc<dyn LanguageModel>> {
        let Some(models) = CopilotChat::global(cx).and_then(|m| m.read(cx).models()) else {
            return Vec::new();
        };
        models
            .iter()
            .map(|model| self.create_language_model(model.clone()))
            .collect()
    }

    fn is_authenticated(&self, cx: &App) -> bool {
        self.state.read(cx).is_authenticated(cx)
    }

    fn authenticate(&self, cx: &mut App) -> Task<Result<(), AuthenticateError>> {
        if self.is_authenticated(cx) {
            return Task::ready(Ok(()));
        };

        let Some(copilot) = GlobalCopilotAuth::try_global(cx).cloned() else {
            return Task::ready(Err(anyhow!(concat!(
                "Copilot must be enabled for Copilot Chat to work. ",
                "Please enable Copilot and try again."
            ))
            .into()));
        };

        let err = match copilot.0.read(cx).status() {
            Status::Authorized => return Task::ready(Ok(())),
            Status::Disabled => anyhow!(
                "Copilot must be enabled for Copilot Chat to work. Please enable Copilot and try again."
            ),
            Status::Error(err) => anyhow!(format!(
                "Received the following error while signing into Copilot: {err}"
            )),
            Status::Starting { task: _ } => anyhow!(
                "Copilot is still starting, please wait for Copilot to start then try again"
            ),
            Status::Unauthorized => anyhow!(
                "Unable to authorize with Copilot. Please make sure that you have an active Copilot and Copilot Chat subscription."
            ),
            Status::SignedOut { .. } => {
                anyhow!("You have signed out of Copilot. Please sign in to Copilot and try again.")
            }
            Status::SigningIn { prompt: _ } => anyhow!("Still signing into Copilot..."),
        };

        Task::ready(Err(err.into()))
    }

    fn configuration_view(
        &self,
        _target_agent: language_model::ConfigurationViewTargetAgent,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyView {
        cx.new(|cx| {
            copilot_ui::ConfigurationView::new(
                |cx| {
                    CopilotChat::global(cx)
                        .map(|m| m.read(cx).is_authenticated())
                        .unwrap_or(false)
                },
                copilot_ui::ConfigurationMode::Chat,
                cx,
            )
        })
        .into()
    }

    fn configuration_view_v2(
        &self,
        target_agent: language_model::ConfigurationViewTargetAgent,
        window: &mut Window,
        cx: &mut App,
    ) -> ProviderConfigurationView {
        // GitHub Copilot's control is just a sign-in button, so render it inline
        // rather than behind a sub-page.
        ProviderConfigurationView::Inline(self.configuration_view(target_agent, window, cx))
    }

    fn reset_credentials(&self, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Err(anyhow!(
            "Signing out of GitHub Copilot Chat is currently not supported."
        )))
    }
}

pub struct CopilotChatLanguageModel {
    model: CopilotChatModel,
    request_limiter: RateLimiter,
}

impl LanguageModel for CopilotChatLanguageModel {
    fn id(&self) -> LanguageModelId {
        LanguageModelId::from(self.model.id().to_string())
    }

    fn name(&self) -> LanguageModelName {
        LanguageModelName::from(self.model.display_name().to_string())
    }

    fn provider_id(&self) -> LanguageModelProviderId {
        PROVIDER_ID
    }

    fn provider_name(&self) -> LanguageModelProviderName {
        PROVIDER_NAME
    }

    fn supports_tools(&self) -> bool {
        self.model.supports_tools()
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn supports_images(&self) -> bool {
        self.model.supports_vision()
    }

    fn supports_thinking(&self) -> bool {
        self.model.can_think()
    }

    fn supported_effort_levels(&self) -> Vec<LanguageModelEffortLevel> {
        let levels = self.model.reasoning_effort_levels();
        if levels.is_empty() {
            return vec![];
        }
        levels
            .iter()
            .map(|level| {
                let name = match level.as_str() {
                    "low" => "Low".into(),
                    "medium" => "Medium".into(),
                    "high" => "High".into(),
                    "xhigh" => "Extra High".into(),
                    _ => language_model::SharedString::from(level.clone()),
                };
                LanguageModelEffortLevel {
                    name,
                    value: language_model::SharedString::from(level.clone()),
                    is_default: level == "high",
                }
            })
            .collect()
    }

    fn tool_input_format(&self) -> LanguageModelToolSchemaFormat {
        match self.model.vendor() {
            ModelVendor::OpenAI | ModelVendor::Anthropic => {
                LanguageModelToolSchemaFormat::JsonSchema
            }
            ModelVendor::Google | ModelVendor::XAI | ModelVendor::Unknown => {
                LanguageModelToolSchemaFormat::JsonSchemaSubset
            }
        }
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto
            | LanguageModelToolChoice::Any
            | LanguageModelToolChoice::None => self.supports_tools(),
        }
    }

    fn model_cost_info(&self) -> Option<LanguageModelCostInfo> {
        LanguageModelCostInfo::RequestCost {
            cost_per_request: self.model.multiplier(),
        }
        .into()
    }

    fn telemetry_id(&self) -> String {
        format!("copilot_chat/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    > {
        let is_user_initiated = request.intent.is_none_or(|intent| match intent {
            CompletionIntent::UserPrompt
            | CompletionIntent::ThreadContextSummarization
            | CompletionIntent::InlineAssist
            | CompletionIntent::TerminalInlineAssist
            | CompletionIntent::GenerateGitCommitMessage => true,

            CompletionIntent::Subagent
            | CompletionIntent::ToolResults
            | CompletionIntent::ThreadSummarization
            | CompletionIntent::CreateFile
            | CompletionIntent::EditFile => false,
        });

        if self.model.supports_messages() {
            let location = intent_to_chat_location(request.intent);
            let model = self.model.clone();
            let request_limiter = self.request_limiter.clone();
            let future = cx.spawn(async move |cx| {
                let effort = request
                    .thinking_effort
                    .as_ref()
                    .and_then(|e| anthropic::Effort::from_str(e).ok());

                let mut anthropic_request = into_anthropic(
                    request,
                    model.id().to_string(),
                    0.0,
                    model.max_output_tokens() as u64,
                    if model.supports_adaptive_thinking() {
                        AnthropicModelMode::Thinking {
                            budget_tokens: None,
                        }
                    } else if model.supports_thinking() {
                        AnthropicModelMode::Thinking {
                            budget_tokens: compute_thinking_budget(
                                model.min_thinking_budget(),
                                model.max_thinking_budget(),
                                model.max_output_tokens() as u32,
                            ),
                        }
                    } else {
                        AnthropicModelMode::Default
                    },
                    AnthropicPromptCacheMode::Legacy,
                );

                anthropic_request.temperature = None;

                // The Copilot proxy doesn't support eager_input_streaming on tools.
                for tool in &mut anthropic_request.tools {
                    tool.eager_input_streaming = false;
                }

                if model.supports_adaptive_thinking() {
                    if anthropic_request.thinking.is_some() {
                        anthropic_request.thinking = Some(anthropic::Thinking::Adaptive {
                            display: Some(anthropic::AdaptiveThinkingDisplay::Summarized),
                        });
                        anthropic_request.output_config =
                            effort.map(|effort| anthropic::OutputConfig {
                                effort: Some(effort),
                            });
                    }
                }

                let anthropic_beta =
                    if !model.supports_adaptive_thinking() && model.supports_thinking() {
                        Some("interleaved-thinking-2025-05-14".to_string())
                    } else {
                        None
                    };

                let body = serde_json::to_string(&anthropic::StreamingRequest {
                    base: anthropic_request,
                    stream: true,
                })
                .map_err(|e| anyhow::anyhow!(e))?;

                let stream = CopilotChat::stream_messages(
                    body,
                    location,
                    is_user_initiated,
                    anthropic_beta,
                    cx.clone(),
                );

                request_limiter
                    .stream(async move {
                        let events = stream.await?;
                        let mapper = AnthropicEventMapper::new(PROVIDER_NAME);
                        Ok(mapper.map_stream(events).boxed())
                    })
                    .await
            });
            return async move { Ok(future.await?.boxed()) }.boxed();
        }

        if self.model.supports_response() {
            let location = intent_to_chat_location(request.intent);
            let responses_request = into_copilot_responses(&self.model, request);
            let request_limiter = self.request_limiter.clone();
            let future = cx.spawn(async move |cx| {
                let request = CopilotChat::stream_response(
                    responses_request,
                    location,
                    is_user_initiated,
                    cx.clone(),
                );
                request_limiter
                    .stream(async move {
                        let stream = request.await?;
                        let mapper = CopilotResponsesEventMapper::new();
                        Ok(mapper.map_stream(stream).boxed())
                    })
                    .await
            });
            return async move { Ok(future.await?.boxed()) }.boxed();
        }

        let location = intent_to_chat_location(request.intent);
        let copilot_request = match into_copilot_chat(&self.model, request) {
            Ok(request) => request,
            Err(err) => return futures::future::ready(Err(err.into())).boxed(),
        };
        let is_streaming = copilot_request.stream;

        let request_limiter = self.request_limiter.clone();
        let future = cx.spawn(async move |cx| {
            let request = CopilotChat::stream_completion(
                copilot_request,
                location,
                is_user_initiated,
                cx.clone(),
            );
            request_limiter
                .stream(async move {
                    let response = request.await?;
                    Ok(map_to_language_model_completion_events(
                        response,
                        is_streaming,
                    ))
                })
                .await
        });
        async move { Ok(future.await?.boxed()) }.boxed()
    }
}
