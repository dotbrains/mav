use super::*;

impl CodegenAlternative {
    pub(super) fn handle_completion(
        &mut self,
        model: Arc<dyn LanguageModel>,
        completion_stream: Task<
            Result<
                BoxStream<
                    'static,
                    Result<LanguageModelCompletionEvent, LanguageModelCompletionError>,
                >,
                LanguageModelCompletionError,
            >,
        >,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        self.diff = Diff::default();
        self.status = CodegenStatus::Pending;

        cx.notify();
        // Leaving this in generation so that STOP equivalent events are respected even
        // while we're still pre-processing the completion event
        cx.spawn(async move |codegen, cx| {
            let finish_with_status = |status: CodegenStatus, cx: &mut AsyncApp| {
                let _ = codegen.update(cx, |this, cx| {
                    this.status = status;
                    cx.emit(CodegenEvent::Finished);
                    cx.notify();
                });
            };

            let mut completion_events = match completion_stream.await {
                Ok(events) => events,
                Err(err) => {
                    finish_with_status(CodegenStatus::Error(err.into()), cx);
                    return;
                }
            };

            enum ToolUseOutput {
                Rewrite {
                    text: String,
                    description: Option<String>,
                },
                Failure(String),
            }

            enum ModelUpdate {
                Description(String),
                Failure(String),
            }

            let chars_read_by_tool_id: Arc<Mutex<HashMap<LanguageModelToolUseId, usize>>> =
                Arc::new(Mutex::new(HashMap::default()));
            let process_tool_use = move |tool_use: LanguageModelToolUse| -> Option<ToolUseOutput> {
                let mut chars_read_by_tool_id = chars_read_by_tool_id.lock();
                match tool_use.name.as_ref() {
                    REWRITE_SECTION_TOOL_NAME => {
                        let Ok(input) =
                            serde_json::from_value::<RewriteSectionInput>(tool_use.input)
                        else {
                            return None;
                        };
                        let chars_read_so_far =
                            chars_read_by_tool_id.entry(tool_use.id).or_insert(0);
                        let Some(text_slice) = input.replacement_text.get(*chars_read_so_far..)
                        else {
                            return None;
                        };
                        let text = text_slice.to_string();
                        *chars_read_so_far = input.replacement_text.len();
                        Some(ToolUseOutput::Rewrite {
                            text,
                            description: None,
                        })
                    }
                    FAILURE_MESSAGE_TOOL_NAME => {
                        let Ok(mut input) =
                            serde_json::from_value::<FailureMessageInput>(tool_use.input)
                        else {
                            return None;
                        };
                        Some(ToolUseOutput::Failure(std::mem::take(&mut input.message)))
                    }
                    _ => None,
                }
            };

            let (message_tx, mut message_rx) = futures::channel::mpsc::unbounded::<ModelUpdate>();

            cx.spawn({
                let codegen = codegen.clone();
                async move |cx| {
                    while let Some(update) = message_rx.next().await {
                        let _ = codegen.update(cx, |this, _cx| match update {
                            ModelUpdate::Description(d) => this.description = Some(d),
                            ModelUpdate::Failure(f) => this.failure = Some(f),
                        });
                    }
                }
            })
            .detach();

            let mut message_id = None;
            let mut first_text = None;
            let last_token_usage = Arc::new(Mutex::new(TokenUsage::default()));
            let total_text = Arc::new(Mutex::new(String::new()));

            loop {
                if let Some(first_event) = completion_events.next().await {
                    match first_event {
                        Ok(LanguageModelCompletionEvent::StartMessage { message_id: id }) => {
                            message_id = Some(id);
                        }
                        Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) => {
                            if let Some(output) = process_tool_use(tool_use) {
                                let (text, update) = match output {
                                    ToolUseOutput::Rewrite { text, description } => {
                                        (Some(text), description.map(ModelUpdate::Description))
                                    }
                                    ToolUseOutput::Failure(message) => {
                                        (None, Some(ModelUpdate::Failure(message)))
                                    }
                                };
                                if let Some(update) = update {
                                    let _ = message_tx.unbounded_send(update);
                                }
                                first_text = text;
                                if first_text.is_some() {
                                    break;
                                }
                            }
                        }
                        Ok(LanguageModelCompletionEvent::UsageUpdate(token_usage)) => {
                            *last_token_usage.lock() = token_usage;
                        }
                        Ok(LanguageModelCompletionEvent::Text(text)) => {
                            let mut lock = total_text.lock();
                            lock.push_str(&text);
                        }
                        Ok(e) => {
                            log::warn!("Unexpected event: {:?}", e);
                            break;
                        }
                        Err(e) => {
                            finish_with_status(CodegenStatus::Error(e.into()), cx);
                            break;
                        }
                    }
                }
            }

            let Some(first_text) = first_text else {
                finish_with_status(CodegenStatus::Done, cx);
                return;
            };

            let move_last_token_usage = last_token_usage.clone();

            let text_stream = Box::pin(futures::stream::once(async { Ok(first_text) }).chain(
                completion_events.filter_map(move |e| {
                    let process_tool_use = process_tool_use.clone();
                    let last_token_usage = move_last_token_usage.clone();
                    let total_text = total_text.clone();
                    let mut message_tx = message_tx.clone();
                    async move {
                        match e {
                            Ok(LanguageModelCompletionEvent::ToolUse(tool_use)) => {
                                let Some(output) = process_tool_use(tool_use) else {
                                    return None;
                                };
                                let (text, update) = match output {
                                    ToolUseOutput::Rewrite { text, description } => {
                                        (Some(text), description.map(ModelUpdate::Description))
                                    }
                                    ToolUseOutput::Failure(message) => {
                                        (None, Some(ModelUpdate::Failure(message)))
                                    }
                                };
                                if let Some(update) = update {
                                    let _ = message_tx.send(update).await;
                                }
                                text.map(Ok)
                            }
                            Ok(LanguageModelCompletionEvent::UsageUpdate(token_usage)) => {
                                *last_token_usage.lock() = token_usage;
                                None
                            }
                            Ok(LanguageModelCompletionEvent::Text(text)) => {
                                let mut lock = total_text.lock();
                                lock.push_str(&text);
                                None
                            }
                            Ok(LanguageModelCompletionEvent::Stop(_reason)) => None,
                            e => {
                                log::error!("UNEXPECTED EVENT {:?}", e);
                                None
                            }
                        }
                    }
                }),
            ));

            let language_model_text_stream = LanguageModelTextStream {
                message_id: message_id,
                stream: text_stream,
                last_token_usage,
            };

            let Some(task) = codegen
                .update(cx, move |codegen, cx| {
                    codegen.handle_stream(
                        model,
                        /* strip_invalid_spans: */ false,
                        async { Ok(language_model_text_stream) },
                        cx,
                    )
                })
                .ok()
            else {
                return;
            };

            task.await;
        })
    }
}
