use super::*;

pub async fn stream_response(
    client: &dyn HttpClient,
    provider_name: &str,
    api_url: &str,
    api_key: &str,
    request: Request,
    extra_headers: &CustomHeaders,
) -> Result<BoxStream<'static, Result<StreamEvent>>, RequestError> {
    let uri = format!("{api_url}/responses");
    let is_streaming = request.stream;
    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .extra_headers(extra_headers)
        .body(AsyncBody::from(
            serde_json::to_string(&request).map_err(|e| RequestError::Other(e.into()))?,
        ))
        .map_err(|e| RequestError::Other(e.into()))?;

    let mut response = client.send(request).await?;
    if response.status().is_success() {
        if is_streaming {
            let reader = BufReader::new(response.into_body());
            Ok(reader
                .lines()
                .filter_map(|line| async move {
                    match line {
                        Ok(line) => {
                            let line = line
                                .strip_prefix("data: ")
                                .or_else(|| line.strip_prefix("data:"))?;
                            if line == "[DONE]" || line.is_empty() {
                                None
                            } else {
                                match serde_json::from_str::<StreamEvent>(line) {
                                    Ok(event) => Some(Ok(event)),
                                    Err(error) => {
                                        log::error!(
                                            "Failed to parse OpenAI responses stream event: `{}`\nResponse: `{}`",
                                            error,
                                            line,
                                        );
                                        Some(Err(anyhow!(error)))
                                    }
                                }
                            }
                        }
                        Err(error) => Some(Err(anyhow!(error))),
                    }
                })
                .boxed())
        } else {
            let mut body = String::new();
            response
                .body_mut()
                .read_to_string(&mut body)
                .await
                .map_err(|e| RequestError::Other(e.into()))?;

            match serde_json::from_str::<ResponseSummary>(&body) {
                Ok(response_summary) => {
                    let events = vec![
                        StreamEvent::Created {
                            response: response_summary.clone(),
                        },
                        StreamEvent::InProgress {
                            response: response_summary.clone(),
                        },
                    ];

                    let mut all_events = events;
                    for (output_index, item) in response_summary.output.iter().enumerate() {
                        all_events.push(StreamEvent::OutputItemAdded {
                            output_index,
                            sequence_number: None,
                            item: item.clone(),
                        });

                        match item {
                            ResponseOutputItem::Message(message) => {
                                for content_item in &message.content {
                                    if let Some(text) = content_item.get("text") {
                                        if let Some(text_str) = text.as_str() {
                                            if let Some(ref item_id) = message.id {
                                                all_events.push(StreamEvent::OutputTextDelta {
                                                    item_id: item_id.clone(),
                                                    output_index,
                                                    content_index: None,
                                                    delta: text_str.to_string(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            ResponseOutputItem::FunctionCall(function_call) => {
                                if let Some(ref item_id) = function_call.id {
                                    all_events.push(StreamEvent::FunctionCallArgumentsDone {
                                        item_id: item_id.clone(),
                                        output_index,
                                        arguments: function_call.arguments.clone(),
                                        sequence_number: None,
                                    });
                                }
                            }
                            ResponseOutputItem::Reasoning(reasoning) => {
                                if let Some(ref item_id) = reasoning.id {
                                    for part in &reasoning.summary {
                                        if let ReasoningSummaryPart::SummaryText { text } = part {
                                            all_events.push(
                                                StreamEvent::ReasoningSummaryTextDelta {
                                                    item_id: item_id.clone(),
                                                    output_index,
                                                    delta: text.clone(),
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                            // No synthesized deltas; the `OutputItemDone`
                            // event pushed below carries the full item.
                            ResponseOutputItem::Compaction(_) => {}
                            ResponseOutputItem::Unknown => {}
                        }

                        all_events.push(StreamEvent::OutputItemDone {
                            output_index,
                            sequence_number: None,
                            item: item.clone(),
                        });
                    }

                    let status = response_summary.status.clone();
                    all_events.push(match status.as_deref() {
                        Some("incomplete") => StreamEvent::Incomplete {
                            response: response_summary,
                        },
                        Some("failed") => StreamEvent::Failed {
                            response: response_summary,
                        },
                        _ => StreamEvent::Completed {
                            response: response_summary,
                        },
                    });

                    Ok(futures::stream::iter(all_events.into_iter().map(Ok)).boxed())
                }
                Err(error) => {
                    log::error!(
                        "Failed to parse OpenAI non-streaming response: `{}`\nResponse: `{}`",
                        error,
                        body,
                    );
                    Err(RequestError::Other(anyhow!(error)))
                }
            }
        }
    } else {
        let mut body = String::new();
        response
            .body_mut()
            .read_to_string(&mut body)
            .await
            .map_err(|e| RequestError::Other(e.into()))?;

        Err(RequestError::HttpResponseError {
            provider: provider_name.to_owned(),
            status_code: response.status(),
            body,
            headers: response.headers().clone(),
        })
    }
}
