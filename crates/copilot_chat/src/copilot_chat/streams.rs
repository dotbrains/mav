use super::*;

pub(crate) async fn stream_completion(
    client: Arc<dyn HttpClient>,
    oauth_token: String,
    completion_url: Arc<str>,
    request: Request,
    is_user_initiated: bool,
    location: ChatLocation,
) -> Result<BoxStream<'static, Result<ResponseEvent>>> {
    let is_vision_request = request.messages.iter().any(|message| match message {
    ChatMessage::User { content }
    | ChatMessage::Assistant { content, .. }
    | ChatMessage::Tool { content, .. } => {
        matches!(content, ChatMessageContent::Multipart(parts) if parts.iter().any(|part| matches!(part, ChatMessagePart::Image { .. })))
    }
    _ => false,
});

    let request_builder = copilot_request_headers(
        HttpRequest::builder()
            .method(Method::POST)
            .uri(completion_url.as_ref()),
        &oauth_token,
        Some(is_user_initiated),
        Some(location),
    )
    .when(is_vision_request, |builder| {
        builder.header("Copilot-Vision-Request", is_vision_request.to_string())
    });

    let is_streaming = request.stream;

    let json = serde_json::to_string(&request)?;
    let request = request_builder.body(AsyncBody::from(json))?;
    let mut response = client.send(request).await?;

    if !response.status().is_success() {
        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;
        let body_str = std::str::from_utf8(&body)?;
        anyhow::bail!(
            "Failed to connect to API: {} {}",
            response.status(),
            body_str
        );
    }

    if is_streaming {
        let reader = BufReader::new(response.into_body());
        Ok(reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => {
                        let line = line.strip_prefix("data: ")?;
                        if line.starts_with("[DONE]") {
                            return None;
                        }

                        match serde_json::from_str::<ResponseEvent>(line) {
                            Ok(response) => {
                                if response.choices.is_empty() {
                                    None
                                } else {
                                    Some(Ok(response))
                                }
                            }
                            Err(error) => Some(Err(anyhow!(error))),
                        }
                    }
                    Err(error) => Some(Err(anyhow!(error))),
                }
            })
            .boxed())
    } else {
        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;
        let body_str = std::str::from_utf8(&body)?;
        let response: ResponseEvent = serde_json::from_str(body_str)?;

        Ok(futures::stream::once(async move { Ok(response) }).boxed())
    }
}

pub(crate) async fn stream_messages(
    client: Arc<dyn HttpClient>,
    oauth_token: String,
    api_url: String,
    body: String,
    is_user_initiated: bool,
    location: ChatLocation,
    anthropic_beta: Option<String>,
) -> Result<BoxStream<'static, Result<anthropic::Event, anthropic::AnthropicError>>> {
    let mut request_builder = copilot_request_headers(
        HttpRequest::builder().method(Method::POST).uri(&api_url),
        &oauth_token,
        Some(is_user_initiated),
        Some(location),
    );

    if let Some(beta) = &anthropic_beta {
        request_builder = request_builder.header("anthropic-beta", beta.as_str());
    }

    let request = request_builder.body(AsyncBody::from(body))?;
    let mut response = client.send(request).await?;

    if !response.status().is_success() {
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        anyhow::bail!("Failed to connect to API: {} {}", response.status(), body);
    }

    let reader = BufReader::new(response.into_body());
    Ok(reader
    .lines()
    .filter_map(|line| async move {
        match line {
            Ok(line) => {
                let line = line
                    .strip_prefix("data: ")
                    .or_else(|| line.strip_prefix("data:"))?;
                if line.starts_with("[DONE]") || line.is_empty() {
                    return None;
                }
                match serde_json::from_str(line) {
                    Ok(event) => Some(Ok(event)),
                    Err(error) => {
                        log::error!(
                            "Failed to parse Copilot messages stream event: `{}`\nResponse: `{}`",
                            error,
                            line,
                        );
                        Some(Err(anthropic::AnthropicError::DeserializeResponse(error)))
                    }
                }
            }
            Err(error) => Some(Err(anthropic::AnthropicError::ReadResponse(error))),
        }
    })
    .boxed())
}
