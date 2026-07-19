mod types;

use anyhow::{Context as _, Result, anyhow};
use futures::{AsyncBufReadExt, AsyncReadExt, StreamExt, io::BufReader, stream::BoxStream};
use http_client::{
    AsyncBody, CustomHeaders, HttpClient, Method, Request as HttpRequest, RequestBuilderExt, http,
};
use std::time::Duration;

pub use types::*;

pub const LMSTUDIO_API_URL: &str = "http://localhost:1234/api/v0";

pub async fn complete(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: Option<&str>,
    request: ChatCompletionRequest,
    extra_headers: &CustomHeaders,
) -> Result<ChatResponse> {
    let uri = format!("{api_url}/chat/completions");
    let mut request_builder = HttpRequest::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json");

    if let Some(api_key) = api_key {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
    }

    let serialized_request = serde_json::to_string(&request)?;
    let request = request_builder
        .extra_headers(extra_headers)
        .body(AsyncBody::from(serialized_request))?;

    let mut response = client.send(request).await?;
    if response.status().is_success() {
        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;
        let response_message: ChatResponse = serde_json::from_slice(&body)?;
        Ok(response_message)
    } else {
        let mut body = Vec::new();
        response.body_mut().read_to_end(&mut body).await?;
        let body_str = std::str::from_utf8(&body)?;
        anyhow::bail!(
            "Failed to connect to API: {} {}",
            response.status(),
            body_str
        );
    }
}

pub async fn stream_chat_completion(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: Option<&str>,
    request: ChatCompletionRequest,
    extra_headers: &CustomHeaders,
) -> Result<BoxStream<'static, Result<ResponseStreamEvent>>> {
    let uri = format!("{api_url}/chat/completions");
    let mut request_builder = http::Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("Content-Type", "application/json");

    if let Some(api_key) = api_key {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
    }

    let request = request_builder
        .extra_headers(extra_headers)
        .body(AsyncBody::from(serde_json::to_string(&request)?))?;
    let mut response = client.send(request).await?;
    if response.status().is_success() {
        let reader = BufReader::new(response.into_body());
        Ok(reader
            .lines()
            .filter_map(|line| async move {
                match line {
                    Ok(line) => {
                        let line = line.strip_prefix("data: ")?;
                        if line == "[DONE]" {
                            None
                        } else {
                            match serde_json::from_str(line) {
                                Ok(ResponseStreamResult::Ok(response)) => Some(Ok(response)),
                                Ok(ResponseStreamResult::Err { error, .. }) => {
                                    Some(Err(anyhow!(error.message)))
                                }
                                Err(error) => Some(Err(anyhow!(error))),
                            }
                        }
                    }
                    Err(error) => Some(Err(anyhow!(error))),
                }
            })
            .boxed())
    } else {
        let mut body = String::new();
        response.body_mut().read_to_string(&mut body).await?;
        anyhow::bail!(
            "Failed to connect to LM Studio API: {} {}",
            response.status(),
            body,
        );
    }
}

pub async fn get_models(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: Option<&str>,
    _: Option<Duration>,
    extra_headers: &CustomHeaders,
) -> Result<Vec<ModelEntry>> {
    let uri = format!("{api_url}/models");
    let mut request_builder = HttpRequest::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Accept", "application/json");

    if let Some(api_key) = api_key {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
    }

    let request = request_builder
        .extra_headers(extra_headers)
        .body(AsyncBody::default())?;

    let mut response = client.send(request).await?;

    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;

    anyhow::ensure!(
        response.status().is_success(),
        "Failed to connect to LM Studio API: {} {}",
        response.status(),
        body,
    );
    let response: ListModelsResponse =
        serde_json::from_str(&body).context("Unable to parse LM Studio models response")?;
    Ok(response.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_message_part_serialization() {
        let image_part = MessagePart::Image {
            image_url: ImageUrl {
                url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string(),
                detail: None,
            },
        };

        let json = serde_json::to_string(&image_part).unwrap();
        println!("Serialized image part: {}", json);

        // Verify the structure matches what LM Studio expects
        let expected_structure = r#"{"type":"image_url","image_url":{"url":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=="}}"#;
        assert_eq!(json, expected_structure);
    }

    #[test]
    fn test_text_message_part_serialization() {
        let text_part = MessagePart::Text {
            text: "Hello, world!".to_string(),
        };

        let json = serde_json::to_string(&text_part).unwrap();
        println!("Serialized text part: {}", json);

        let expected_structure = r#"{"type":"text","text":"Hello, world!"}"#;
        assert_eq!(json, expected_structure);
    }
}
