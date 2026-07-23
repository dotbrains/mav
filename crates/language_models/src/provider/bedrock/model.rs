use super::*;

pub(super) struct BedrockModel {
    pub(super) id: LanguageModelId,
    pub(super) model: Model,
    pub(super) http_client: AwsHttpClient,
    pub(super) handle: tokio::runtime::Handle,
    pub(super) client: OnceCell<BedrockClient>,
    pub(super) state: Entity<State>,
    pub(super) request_limiter: RateLimiter,
}

impl BedrockModel {
    fn get_or_init_client(&self, cx: &AsyncApp) -> anyhow::Result<&BedrockClient> {
        self.client
            .get_or_try_init_blocking(|| {
                let (auth, endpoint, region) = cx.read_entity(&self.state, |state, _cx| {
                    let endpoint = state.settings.as_ref().and_then(|s| s.endpoint.clone());
                    let region = state.get_region();
                    (state.auth.clone(), endpoint, region)
                });

                let mut config_builder = aws_config::defaults(BehaviorVersion::latest())
                    .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
                    .http_client(self.http_client.clone())
                    .region(Region::new(region))
                    .timeout_config(TimeoutConfig::disabled());

                if let Some(endpoint_url) = endpoint
                    && !endpoint_url.is_empty()
                {
                    config_builder = config_builder.endpoint_url(endpoint_url);
                }

                match auth {
                    Some(BedrockAuth::Automatic) | None => {
                        // Use default AWS credential provider chain
                    }
                    Some(BedrockAuth::NamedProfile { profile_name })
                    | Some(BedrockAuth::SingleSignOn { profile_name }) => {
                        if !profile_name.is_empty() {
                            config_builder = config_builder.profile_name(profile_name);
                        }
                    }
                    Some(BedrockAuth::IamCredentials {
                        access_key_id,
                        secret_access_key,
                        session_token,
                    }) => {
                        let aws_creds = Credentials::new(
                            access_key_id,
                            secret_access_key,
                            session_token,
                            None,
                            "mav-bedrock-provider",
                        );
                        config_builder = config_builder.credentials_provider(aws_creds);
                    }
                    Some(BedrockAuth::ApiKey { api_key }) => {
                        config_builder = config_builder
                            .auth_scheme_preference(["httpBearerAuth".into()]) // https://github.com/smithy-lang/smithy-rs/pull/4241
                            .token_provider(Token::new(api_key, None));
                    }
                }

                let config = self.handle.block_on(config_builder.load());

                anyhow::Ok(BedrockClient::new(&config))
            })
            .context("initializing Bedrock client")?;

        self.client.get().context("Bedrock client not initialized")
    }

    fn stream_completion(
        &self,
        request: bedrock::Request,
        cx: &AsyncApp,
    ) -> BoxFuture<
        'static,
        Result<BoxStream<'static, Result<BedrockStreamingResponse, anyhow::Error>>, BedrockError>,
    > {
        let Ok(runtime_client) = self
            .get_or_init_client(cx)
            .cloned()
            .context("Bedrock client not initialized")
        else {
            return futures::future::ready(Err(BedrockError::Other(anyhow!("App state dropped"))))
                .boxed();
        };
        let extra_headers = self.state.read_with(cx, |_, cx| {
            AllLanguageModelSettings::get_global(cx)
                .bedrock
                .custom_headers
                .clone()
        });

        let task = Tokio::spawn(
            cx,
            bedrock::stream_completion(runtime_client, request, extra_headers),
        );
        async move { task.await.map_err(|e| BedrockError::Other(e.into()))? }.boxed()
    }
}

impl LanguageModel for BedrockModel {
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
        self.model.supports_tool_use()
    }

    fn supports_images(&self) -> bool {
        self.model.supports_images()
    }

    fn supports_thinking(&self) -> bool {
        self.model.supports_thinking()
    }

    fn supported_effort_levels(&self) -> Vec<language_model::LanguageModelEffortLevel> {
        if self.model.supports_adaptive_thinking() {
            vec![
                language_model::LanguageModelEffortLevel {
                    name: "Low".into(),
                    value: "low".into(),
                    is_default: false,
                },
                language_model::LanguageModelEffortLevel {
                    name: "Medium".into(),
                    value: "medium".into(),
                    is_default: false,
                },
                language_model::LanguageModelEffortLevel {
                    name: "High".into(),
                    value: "high".into(),
                    is_default: true,
                },
                language_model::LanguageModelEffortLevel {
                    name: "XHigh".into(),
                    value: "xhigh".into(),
                    is_default: false,
                },
                language_model::LanguageModelEffortLevel {
                    name: "Max".into(),
                    value: "max".into(),
                    is_default: false,
                },
            ]
            .into_iter()
            .filter(|effort_level| {
                effort_level.value != "xhigh" || self.model.supports_xhigh_adaptive_thinking()
            })
            .collect()
        } else {
            Vec::new()
        }
    }

    fn supports_tool_choice(&self, choice: LanguageModelToolChoice) -> bool {
        match choice {
            LanguageModelToolChoice::Auto | LanguageModelToolChoice::Any => {
                self.model.supports_tool_use()
            }
            // Add support for None - we'll filter tool calls at response
            LanguageModelToolChoice::None => self.model.supports_tool_use(),
        }
    }

    fn supports_streaming_tools(&self) -> bool {
        true
    }

    fn telemetry_id(&self) -> String {
        format!("bedrock/{}", self.model.id())
    }

    fn max_token_count(&self) -> u64 {
        self.model.max_token_count()
    }

    fn max_output_tokens(&self) -> Option<u64> {
        Some(self.model.max_output_tokens())
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
        let (region, allow_global, guardrail_identifier, guardrail_version) =
            cx.read_entity(&self.state, |state, _cx| {
                let (gid, gv) = state.get_guardrail_config();
                (state.get_region(), state.get_allow_global(), gid, gv)
            });

        let model_id = match self.model.cross_region_inference_id(&region, allow_global) {
            Ok(s) => s,
            Err(e) => {
                return async move { Err(e.into()) }.boxed();
            }
        };

        let deny_tool_calls = request.tool_choice == Some(LanguageModelToolChoice::None);

        let request = match into_bedrock(
            request,
            model_id,
            self.model.default_temperature(),
            self.model.max_output_tokens(),
            self.model.thinking_mode(),
            self.model.supports_caching(),
            self.model.supports_tool_use(),
            guardrail_identifier,
            guardrail_version,
        ) {
            Ok(request) => request,
            Err(err) => return futures::future::ready(Err(err.into())).boxed(),
        };

        let request = self.stream_completion(request, cx);
        let display_name = self.model.display_name().to_string();
        let future = self.request_limiter.stream(async move {
            let response = request.await.map_err(|err| match err {
                BedrockError::Validation(ref msg) => {
                    if msg.contains("model identifier is invalid") {
                        LanguageModelCompletionError::Other(anyhow!(
                            "{display_name} is not available in {region}. \
                                 Try switching to a region where this model is supported."
                        ))
                    } else {
                        LanguageModelCompletionError::BadRequestFormat {
                            provider: PROVIDER_NAME,
                            message: msg.clone(),
                        }
                    }
                }
                BedrockError::RateLimited => LanguageModelCompletionError::RateLimitExceeded {
                    provider: PROVIDER_NAME,
                    retry_after: None,
                },
                BedrockError::ServiceUnavailable => {
                    LanguageModelCompletionError::ServerOverloaded {
                        provider: PROVIDER_NAME,
                        retry_after: None,
                    }
                }
                BedrockError::AccessDenied(msg) => LanguageModelCompletionError::PermissionError {
                    provider: PROVIDER_NAME,
                    message: msg,
                },
                BedrockError::InternalServer(msg) => {
                    LanguageModelCompletionError::ApiInternalServerError {
                        provider: PROVIDER_NAME,
                        message: msg,
                    }
                }
                other => LanguageModelCompletionError::Other(anyhow!(other)),
            })?;
            let events = map_to_language_model_completion_events(response);

            if deny_tool_calls {
                Ok(deny_tool_use_events(events).boxed())
            } else {
                Ok(events.boxed())
            }
        });

        async move { Ok(future.await?.boxed()) }.boxed()
    }
}
