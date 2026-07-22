#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;
    use http_client::FakeHttpClient;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeCredentialsProvider {
        api_key: Vec<u8>,
    }

    impl CredentialsProvider for FakeCredentialsProvider {
        fn read_credentials<'a>(
            &'a self,
            _url: &'a str,
            _cx: &'a AsyncApp,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Option<(String, Vec<u8>)>>> + 'a>,
        > {
            let api_key = self.api_key.clone();
            Box::pin(async move { Ok(Some(("Bearer".to_string(), api_key))) })
        }

        fn write_credentials<'a>(
            &'a self,
            _url: &'a str,
            _username: &'a str,
            _password: &'a [u8],
            _cx: &'a AsyncApp,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
            Box::pin(async { Ok(()) })
        }

        fn delete_credentials<'a>(
            &'a self,
            _url: &'a str,
            _cx: &'a AsyncApp,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + 'a>> {
            Box::pin(async { Ok(()) })
        }
    }

    fn entry(id: &str, n_ctx: Option<u64>, n_ctx_train: Option<u64>) -> ModelEntry {
        ModelEntry {
            id: id.to_string(),
            meta: Some(llama_cpp::ModelMeta { n_ctx, n_ctx_train }),
            architecture: None,
            status: None,
        }
    }

    #[test]
    fn display_name_strips_path_and_extension() {
        assert_eq!(
            display_name_for("../models/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
            "Qwen2.5-Coder-7B-Instruct-Q4_K_M"
        );
        assert_eq!(display_name_for("my-alias"), "my-alias");
    }

    #[test]
    fn telemetry_id_strips_local_model_paths() {
        assert_eq!(
            telemetry_id_for("/Users/alice/models/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
            "llama.cpp/Qwen2.5-Coder-7B-Instruct-Q4_K_M"
        );
        assert_eq!(
            telemetry_id_for(r"C:\Users\alice\models\Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
            "llama.cpp/Qwen2.5-Coder-7B-Instruct-Q4_K_M"
        );
        assert_eq!(telemetry_id_for("my-alias"), "llama.cpp/my-alias");
    }

    #[test]
    fn model_uses_props_then_meta_for_context() {
        let props = Props {
            default_generation_settings: Some(llama_cpp::GenerationSettings { n_ctx: Some(8192) }),
            modalities: Some(llama_cpp::Modalities { vision: true }),
            chat_template_caps: Some(llama_cpp::ChatTemplateCaps {
                supports_tool_calls: true,
                supports_preserve_reasoning: true,
                ..Default::default()
            }),
        };
        // /props wins when present.
        let model = model_from_entry(&entry("m", Some(4096), Some(131072)), Some(&props));
        assert_eq!(model.max_tokens, 8192);
        assert!(model.supports_tools);
        assert!(model.supports_images);
        assert!(model.supports_thinking);

        // Unprobed: falls back to the listing's runtime context, then trained
        // context. Tools are assumed supported until the model loads.
        let model = model_from_entry(&entry("m", Some(4096), Some(131072)), None);
        assert_eq!(model.max_tokens, 4096);
        assert!(model.supports_tools);
        assert!(!model.supports_thinking);

        let model = model_from_entry(&entry("m", None, Some(131072)), None);
        assert_eq!(model.max_tokens, 131072);

        // Nothing reported -> the optimistic unloaded-context assumption.
        let model = model_from_entry(&entry("m", None, None), None);
        assert_eq!(model.max_tokens, ASSUMED_UNLOADED_CONTEXT);
        assert!(model.supports_tools);
    }

    #[test]
    fn router_entry_detects_vision_from_modalities() {
        let router_entry = ModelEntry {
            id: "vlm".to_string(),
            meta: None,
            architecture: Some(llama_cpp::Architecture {
                input_modalities: vec!["text".to_string(), "image".to_string()],
            }),
            status: Some(llama_cpp::ModelStatus {
                value: "unloaded".to_string(),
            }),
        };
        let model = model_from_entry(&router_entry, None);
        assert!(model.supports_images);
        // Unprobed router models optimistically advertise tools until loaded.
        assert!(model.supports_tools);
    }

    #[test]
    fn settings_override_capabilities_and_context() {
        let mut models: HashMap<String, llama_cpp::Model> = HashMap::default();
        models.insert(
            "qwen".to_string(),
            llama_cpp::Model::new("qwen", Some("qwen"), Some(8192), false, false, false),
        );

        let available = vec![AvailableModel {
            name: "qwen".to_string(),
            display_name: Some("Qwen Coder".to_string()),
            max_tokens: 16384,
            supports_tools: Some(true),
            supports_images: None,
            supports_thinking: Some(true),
        }];

        merge_settings_into_models(&mut models, &available, None);

        let model = models.get("qwen").unwrap();
        assert_eq!(model.display_name.as_deref(), Some("Qwen Coder"));
        assert_eq!(model.max_tokens, 16384);
        assert!(model.supports_tools);
        assert!(model.supports_thinking);
        // Unspecified capability keeps the discovered value.
        assert!(!model.supports_images);
    }

    #[test]
    fn capability_cells_update_when_a_model_loads() {
        let cells: CapabilityCells = Arc::new(RwLock::new(HashMap::default()));
        let settings = LlamaCppSettings {
            auto_discover: true,
            ..Default::default()
        };

        // Cold: the optimistic unloaded-context assumption.
        let cold = vec![llama_cpp::Model::new(
            "m",
            Some("m"),
            Some(ASSUMED_UNLOADED_CONTEXT),
            true,
            false,
            false,
        )];
        sync_capability_cells(&cells, &compute_effective_models(&cold, &settings));
        assert_eq!(
            cells.read().unwrap().get("m").unwrap().max_tokens,
            ASSUMED_UNLOADED_CONTEXT
        );

        // The model loads and reports its real context. The shared map must
        // reflect the new value so a model reading it by name (an open
        // conversation) is no longer stuck on the cold-start assumption.
        let loaded = vec![llama_cpp::Model::new(
            "m",
            Some("m"),
            Some(262_144),
            true,
            false,
            true,
        )];
        sync_capability_cells(&cells, &compute_effective_models(&loaded, &settings));
        assert_eq!(cells.read().unwrap().get("m").unwrap().max_tokens, 262_144);
        assert!(cells.read().unwrap().get("m").unwrap().supports_thinking);
    }

    #[test]
    fn request_preserves_assistant_thinking_when_supported() {
        let request = build_llama_cpp_request(
            "test-model",
            false,
            LiveCapabilities {
                max_tokens: 8192,
                supports_tools: false,
                supports_thinking: true,
            },
            LanguageModelRequest {
                messages: vec![language_model::LanguageModelRequestMessage {
                    role: Role::Assistant,
                    content: vec![
                        MessageContent::Thinking {
                            text: "reasoning".to_string(),
                            signature: None,
                        },
                        MessageContent::Text("answer".to_string()),
                    ],
                    cache: false,
                    reasoning_details: None,
                }],
                ..Default::default()
            },
        );

        assert_eq!(request.messages.len(), 1);
        match &request.messages[0] {
            llama_cpp::ChatMessage::Assistant {
                content: Some(llama_cpp::MessageContent::Plain(content)),
                reasoning_content: Some(reasoning_content),
                tool_calls,
            } => {
                assert_eq!(content, "answer");
                assert_eq!(reasoning_content, "reasoning");
                assert!(tool_calls.is_empty());
            }
            message => panic!("unexpected message: {message:?}"),
        }
    }

    #[test]
    fn request_skips_assistant_thinking_when_unsupported() {
        let request = build_llama_cpp_request(
            "test-model",
            false,
            LiveCapabilities {
                max_tokens: 8192,
                supports_tools: false,
                supports_thinking: false,
            },
            LanguageModelRequest {
                messages: vec![language_model::LanguageModelRequestMessage {
                    role: Role::Assistant,
                    content: vec![
                        MessageContent::Thinking {
                            text: "reasoning".to_string(),
                            signature: None,
                        },
                        MessageContent::RedactedThinking("encrypted".to_string()),
                        MessageContent::Text("answer".to_string()),
                    ],
                    cache: false,
                    reasoning_details: None,
                }],
                ..Default::default()
            },
        );

        assert_eq!(request.messages.len(), 1);
        match &request.messages[0] {
            llama_cpp::ChatMessage::Assistant {
                content: Some(llama_cpp::MessageContent::Plain(content)),
                reasoning_content,
                tool_calls,
            } => {
                assert_eq!(content, "answer");
                assert_eq!(reasoning_content, &None);
                assert!(tool_calls.is_empty());
            }
            message => panic!("unexpected message: {message:?}"),
        }
    }

    #[test]
    fn request_preserves_thinking_for_assistant_tool_calls_when_supported() {
        let request = build_llama_cpp_request(
            "test-model",
            false,
            LiveCapabilities {
                max_tokens: 8192,
                supports_tools: true,
                supports_thinking: true,
            },
            LanguageModelRequest {
                messages: vec![language_model::LanguageModelRequestMessage {
                    role: Role::Assistant,
                    content: vec![
                        MessageContent::Thinking {
                            text: "reasoning".to_string(),
                            signature: None,
                        },
                        MessageContent::ToolUse(LanguageModelToolUse {
                            id: "call_1".into(),
                            name: "weather".into(),
                            raw_input: r#"{"city":"Oslo"}"#.to_string(),
                            input: serde_json::json!({ "city": "Oslo" }),
                            is_input_complete: true,
                            thought_signature: None,
                        }),
                    ],
                    cache: false,
                    reasoning_details: None,
                }],
                ..Default::default()
            },
        );

        assert_eq!(request.messages.len(), 1);
        match &request.messages[0] {
            llama_cpp::ChatMessage::Assistant {
                content: None,
                reasoning_content: Some(reasoning_content),
                tool_calls,
            } => {
                assert_eq!(reasoning_content, "reasoning");
                assert_eq!(tool_calls.len(), 1);
            }
            message => panic!("unexpected message: {message:?}"),
        }
    }

    #[test]
    fn usage_event_precedes_stop_event() {
        let mut mapper = LlamaCppEventMapper::new();
        let events = mapper.map_event(llama_cpp::ResponseStreamEvent {
            model: "test-model".to_string(),
            object: "chat.completion.chunk".to_string(),
            choices: vec![llama_cpp::ChoiceDelta {
                index: 0,
                delta: llama_cpp::ResponseMessageDelta {
                    content: None,
                    reasoning_content: None,
                    tool_calls: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(llama_cpp::Usage {
                prompt_tokens: 11,
                completion_tokens: 7,
                total_tokens: 18,
            }),
        });

        assert!(matches!(
            events.as_slice(),
            [
                Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                    input_tokens: 11,
                    output_tokens: 7,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                })),
                Ok(LanguageModelCompletionEvent::Stop(StopReason::EndTurn)),
            ]
        ));
    }

    #[test]
    fn usage_event_precedes_tool_use_stop_event() {
        let mut mapper = LlamaCppEventMapper::new();
        let events = mapper.map_event(llama_cpp::ResponseStreamEvent {
            model: "test-model".to_string(),
            object: "chat.completion.chunk".to_string(),
            choices: vec![llama_cpp::ChoiceDelta {
                index: 0,
                delta: llama_cpp::ResponseMessageDelta {
                    content: None,
                    reasoning_content: None,
                    tool_calls: Some(vec![llama_cpp::ToolCallChunk {
                        index: 0,
                        id: Some("tool-call-id".to_string()),
                        function: Some(llama_cpp::FunctionChunk {
                            name: Some("test_tool".to_string()),
                            arguments: Some(r#"{"value":1}"#.to_string()),
                        }),
                    }]),
                },
                finish_reason: Some("tool_calls".to_string()),
            }],
            usage: Some(llama_cpp::Usage {
                prompt_tokens: 13,
                completion_tokens: 5,
                total_tokens: 18,
            }),
        });

        assert!(matches!(
            events.as_slice(),
            [
                Ok(LanguageModelCompletionEvent::UsageUpdate(TokenUsage {
                    input_tokens: 13,
                    output_tokens: 5,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                })),
                Ok(LanguageModelCompletionEvent::ToolUse(LanguageModelToolUse {
                    id,
                    name,
                    ..
                })),
                Ok(LanguageModelCompletionEvent::Stop(StopReason::ToolUse)),
            ] if id.to_string() == "tool-call-id" && name.as_ref() == "test_tool"
        ));
    }

    #[gpui::test]
    async fn authenticate_fetches_models_after_loading_api_key(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        let model_request_authorizations = Arc::new(Mutex::new(Vec::new()));
        let model_request_count = Arc::new(AtomicUsize::new(0));
        let http_client = FakeHttpClient::create({
            let model_request_authorizations = model_request_authorizations.clone();
            let model_request_count = model_request_count.clone();
            move |request| {
                let model_request_authorizations = model_request_authorizations.clone();
                let model_request_count = model_request_count.clone();
                async move {
                    let path = request.uri().path();
                    let authorization = request
                        .headers()
                        .get("Authorization")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);

                    if path == "/v1/models" {
                        model_request_authorizations.lock().push(authorization);
                        let request_index = model_request_count.fetch_add(1, Ordering::SeqCst);
                        if request_index == 0 {
                            return Ok(http_client::Response::builder()
                                .status(503)
                                .body(http_client::AsyncBody::from("not ready"))?);
                        }

                        return Ok(http_client::Response::builder().status(200).body(
                            http_client::AsyncBody::from(
                                r#"{"data":[{"id":"test-model","meta":{"n_ctx":4096}}]}"#,
                            ),
                        )?);
                    }

                    if path == "/props" {
                        return Ok(http_client::Response::builder()
                            .status(200)
                            .body(http_client::AsyncBody::from("{}"))?);
                    }

                    Ok(http_client::Response::builder()
                        .status(404)
                        .body(http_client::AsyncBody::default())?)
                }
            }
        });
        let credentials_provider = Arc::new(FakeCredentialsProvider {
            api_key: b"loaded-key".to_vec(),
        });
        let provider = cx
            .update(|cx| LlamaCppLanguageModelProvider::new(http_client, credentials_provider, cx));

        cx.run_until_parked();

        let result = cx.update(|cx| provider.authenticate(cx)).await;
        assert!(
            result.is_ok(),
            "authenticate should discover models after loading credentials"
        );
        assert_eq!(
            &*model_request_authorizations.lock(),
            &[None, Some("Bearer loaded-key".to_string())]
        );
    }
}
