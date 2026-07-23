use super::*;

pub struct OllamaLanguageModel {
    pub(super) id: LanguageModelId,
    pub(super) model: ollama::Model,
    pub(super) http_client: Arc<dyn HttpClient>,
    pub(super) request_limiter: RateLimiter,
    pub(super) state: Entity<State>,
    pub(super) disabled: Option<DisabledReason>,
}

impl OllamaLanguageModel {
    fn to_ollama_request(&self, request: LanguageModelRequest) -> ChatRequest {
        let supports_vision = self.model.supports_vision.unwrap_or(false);

        let mut messages = Vec::with_capacity(request.messages.len());

        for mut msg in request.messages.into_iter() {
            let images = if supports_vision {
                msg.content
                    .iter()
                    .filter_map(|content| match content {
                        MessageContent::Image(image) => Some(image.source.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<String>>()
            } else {
                vec![]
            };

            match msg.role {
                Role::User => {
                    for tool_result in msg
                        .content
                        .extract_if(.., |x| matches!(x, MessageContent::ToolResult(..)))
                    {
                        match tool_result {
                            MessageContent::ToolResult(tool_result) => {
                                messages.push(ChatMessage::Tool {
                                    tool_name: tool_result.tool_name.to_string(),
                                    content: tool_result.text_contents(),
                                })
                            }
                            _ => unreachable!("Only tool result should be extracted"),
                        }
                    }
                    if !msg.content.is_empty() {
                        messages.push(ChatMessage::User {
                            content: msg.string_contents(),
                            images: if images.is_empty() {
                                None
                            } else {
                                Some(images)
                            },
                        })
                    }
                }
                Role::Assistant => {
                    let mut text_content = String::new();
                    let mut thinking = None;
                    let mut tool_calls = Vec::new();
                    for content in msg.content.into_iter() {
                        match content {
                            MessageContent::Text(text) => {
                                text_content.push_str(&text);
                            }
                            MessageContent::Thinking { text, .. } if !text.is_empty() => {
                                thinking = Some(text)
                            }
                            MessageContent::ToolUse(tool_use) => {
                                tool_calls.push(OllamaToolCall {
                                    id: tool_use.id.to_string(),
                                    function: OllamaFunctionCall {
                                        name: tool_use.name.to_string(),
                                        arguments: tool_use.input,
                                    },
                                });
                            }
                            _ => (),
                        }
                    }
                    messages.push(ChatMessage::Assistant {
                        content: text_content,
                        tool_calls: Some(tool_calls),
                        images: if images.is_empty() {
                            None
                        } else {
                            Some(images)
                        },
                        thinking,
                    })
                }
                Role::System => messages.push(ChatMessage::System {
                    content: msg.string_contents(),
                }),
            }
        }
        ChatRequest {
            model: self.model.name.clone(),
            messages,
            keep_alive: self.model.keep_alive.clone().unwrap_or_default(),
            stream: true,
            options: Some(ChatOptions {
                num_ctx: Some(self.model.max_tokens),
                // Only send stop tokens if explicitly provided. When empty/None,
                // Ollama will use the model's default stop tokens from its Modelfile.
                // Sending an empty array would override and disable the defaults.
                stop: if request.stop.is_empty() {
                    None
                } else {
                    Some(request.stop)
                },
                temperature: request.temperature.or(Some(1.0)),
                ..Default::default()
            }),
            think: self
                .model
                .supports_thinking
                .map(|supports_thinking| supports_thinking && request.thinking_allowed),
            tools: if self.model.supports_tools.unwrap_or(false) {
                request.tools.into_iter().map(tool_into_ollama).collect()
            } else {
                vec![]
            },
        }
    }
}

impl LanguageModel for OllamaLanguageModel {
    fn id(&self) -> LanguageModelId {
        self.id.clone()
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
        self.model.supports_tools.unwrap_or(false)
    }

    fn supports_images(&self) -> bool {
        self.model.supports_vision.unwrap_or(false)
    }

    fn supports_thinking(&self) -> bool {
        self.model.supports_thinking.unwrap_or(false)
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto => false,
            LanguageModelToolChoice::Any => false,
            LanguageModelToolChoice::None => false,
        }
    }

    fn telemetry_id(&self) -> String {
        format!("ollama/{}", self.model.id())
    }

    fn is_disabled(&self) -> Option<DisabledReason> {
        self.disabled.clone()
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
        let request = self.to_ollama_request(request);

        let http_client = self.http_client.clone();
        let (api_key, api_url, extra_headers) = self.state.read_with(cx, |state, cx| {
            let api_url = OllamaLanguageModelProvider::api_url(cx);
            let extra_headers = OllamaLanguageModelProvider::settings(cx)
                .custom_headers
                .clone();
            (state.api_key_state.key(&api_url), api_url, extra_headers)
        });

        let future = self.request_limiter.stream(async move {
            let stream = stream_chat_completion(
                http_client.as_ref(),
                &api_url,
                api_key.as_deref(),
                request,
                &extra_headers,
            )
            .await?;
            let stream = map_to_language_model_completion_events(stream);
            Ok(stream)
        });

        future.map_ok(|f| f.boxed()).boxed()
    }
}

fn map_to_language_model_completion_events(
    stream: Pin<Box<dyn Stream<Item = anyhow::Result<ChatResponseDelta>> + Send>>,
) -> impl Stream<Item = Result<LanguageModelCompletionEvent, LanguageModelCompletionError>> {
    struct State {
        stream: Pin<Box<dyn Stream<Item = anyhow::Result<ChatResponseDelta>> + Send>>,
        used_tools: bool,
    }

    // We need to create a ToolUse and Stop event from a single
    // response from the original stream
    let stream = stream::unfold(
        State {
            stream,
            used_tools: false,
        },
        async move |mut state| {
            let response = state.stream.next().await?;

            let delta = match response {
                Ok(delta) => delta,
                Err(e) => {
                    let event = Err(LanguageModelCompletionError::from(anyhow!(e)));
                    return Some((vec![event], state));
                }
            };

            let mut events = Vec::new();

            match delta.message {
                ChatMessage::User { content, images: _ } => {
                    events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                }
                ChatMessage::System { content } => {
                    events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                }
                ChatMessage::Tool { content, .. } => {
                    events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                }
                ChatMessage::Assistant {
                    content,
                    tool_calls,
                    images: _,
                    thinking,
                } => {
                    if let Some(text) = thinking {
                        events.push(Ok(LanguageModelCompletionEvent::Thinking {
                            text,
                            signature: None,
                        }));
                    }

                    if let Some(tool_call) = tool_calls.and_then(|v| v.into_iter().next()) {
                        let OllamaToolCall { id, function } = tool_call;
                        let event = LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
                            id: LanguageModelToolUseId::from(id),
                            name: Arc::from(function.name),
                            raw_input: function.arguments.to_string(),
                            input: function.arguments,
                            is_input_complete: true,
                            thought_signature: None,
                        });
                        events.push(Ok(event));
                        state.used_tools = true;
                    } else if !content.is_empty() {
                        events.push(Ok(LanguageModelCompletionEvent::Text(content)));
                    }
                }
            };

            if delta.done {
                events.push(Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                    input_tokens: delta.prompt_eval_count.unwrap_or(0),
                    output_tokens: delta.eval_count.unwrap_or(0),
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                })));
                if state.used_tools {
                    state.used_tools = false;
                    events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::ToolUse)));
                } else {
                    events.push(Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)));
                }
            }

            Some((events, state))
        },
    );

    stream.flat_map(futures::stream::iter)
}
fn tool_into_ollama(tool: LanguageModelRequestTool) -> ollama::OllamaTool {
    ollama::OllamaTool::Function {
        function: OllamaFunctionTool {
            name: tool.name,
            description: Some(tool.description),
            parameters: Some(tool.input_schema),
        },
    }
}
