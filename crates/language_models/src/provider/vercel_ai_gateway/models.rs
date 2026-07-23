use super::*;

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ApiModel>,
}

#[derive(Deserialize)]
struct ApiModel {
    id: String,
    name: Option<String>,
    context_window: Option<u64>,
    max_tokens: Option<u64>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    supported_parameters: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    architecture: Option<ApiModelArchitecture>,
}

#[derive(Deserialize)]
struct ApiModelArchitecture {
    #[serde(default)]
    input_modalities: Vec<String>,
}

pub(super) async fn list_models(
    client: &dyn HttpClient,
    api_url: &str,
    api_key: Option<&str>,
    extra_headers: &CustomHeaders,
) -> Result<Vec<AvailableModel>, LanguageModelCompletionError> {
    let uri = format!("{api_url}/models?include_mappings=true");
    let mut request_builder = HttpRequest::builder()
        .method(Method::GET)
        .uri(uri)
        .header("Accept", "application/json");
    if let Some(api_key) = api_key {
        request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
    }
    let request = request_builder
        .extra_headers(extra_headers)
        .body(AsyncBody::default())
        .map_err(|error| LanguageModelCompletionError::BuildRequestBody {
            provider: PROVIDER_NAME,
            error,
        })?;
    let mut response =
        client
            .send(request)
            .await
            .map_err(|error| LanguageModelCompletionError::HttpSend {
                provider: PROVIDER_NAME,
                error,
            })?;

    let mut body = String::new();
    response
        .body_mut()
        .read_to_string(&mut body)
        .await
        .map_err(|error| LanguageModelCompletionError::ApiReadResponseError {
            provider: PROVIDER_NAME,
            error,
        })?;

    if !response.status().is_success() {
        return Err(LanguageModelCompletionError::from_http_status(
            PROVIDER_NAME,
            response.status(),
            extract_error_message(&body),
            None,
        ));
    }

    let response: ModelsResponse = serde_json::from_str(&body).map_err(|error| {
        LanguageModelCompletionError::DeserializeResponse {
            provider: PROVIDER_NAME,
            error,
        }
    })?;

    let mut models = Vec::new();
    for model in response.data {
        if let Some(model_type) = model.r#type.as_deref()
            && model_type != "language"
        {
            continue;
        }
        let supports_tools = model
            .supported_parameters
            .iter()
            .any(|parameter| parameter == "tools")
            || has_tag(&model.tags, "tool-use")
            || has_tag(&model.tags, "tools");
        let supports_images = model.architecture.is_some_and(|architecture| {
            architecture
                .input_modalities
                .iter()
                .any(|modality| modality == "image")
        }) || has_tag(&model.tags, "vision")
            || has_tag(&model.tags, "image-input");
        let parallel_tool_calls = model
            .supported_parameters
            .iter()
            .any(|parameter| parameter == "parallel_tool_calls");
        let prompt_cache_key = model
            .supported_parameters
            .iter()
            .any(|parameter| parameter == "prompt_cache_key" || parameter == "cache_control");
        models.push(AvailableModel {
            name: model.id.clone(),
            display_name: model.name.or(Some(model.id)),
            max_tokens: model.context_window.or(model.max_tokens).unwrap_or(128_000),
            max_output_tokens: model.max_tokens,
            max_completion_tokens: None,
            capabilities: ModelCapabilities {
                tools: supports_tools,
                images: supports_images,
                parallel_tool_calls,
                prompt_cache_key,
                chat_completions: true,
                interleaved_reasoning: false,
                max_tokens_parameter: false,
            },
        });
    }

    Ok(models)
}

pub(super) fn map_open_ai_error(error: open_ai::RequestError) -> LanguageModelCompletionError {
    match error {
        open_ai::RequestError::HttpResponseError {
            status_code,
            body,
            headers,
            ..
        } => {
            let retry_after = headers
                .get(http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()?.parse::<u64>().ok())
                .map(std::time::Duration::from_secs);

            LanguageModelCompletionError::from_http_status(
                PROVIDER_NAME,
                status_code,
                extract_error_message(&body),
                retry_after,
            )
        }
        open_ai::RequestError::Other(error) => LanguageModelCompletionError::Other(error),
    }
}

fn extract_error_message(body: &str) -> String {
    let json = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(json) => json,
        Err(_) => return body.to_string(),
    };

    let message = json
        .get("error")
        .and_then(|value| {
            value
                .get("message")
                .and_then(serde_json::Value::as_str)
                .or_else(|| value.as_str())
        })
        .or_else(|| json.get("message").and_then(serde_json::Value::as_str))
        .map(ToString::to_string)
        .unwrap_or_else(|| body.to_string());

    clean_error_message(&message)
}

fn clean_error_message(message: &str) -> String {
    let lower = message.to_lowercase();

    if lower.contains("vercel_oidc_token") && lower.contains("oidc token") {
        return "Authentication failed for Vercel AI Gateway. Use a Vercel AI Gateway key (vck_...).\nCreate or manage keys in Vercel AI Gateway console.\nIf this persists, regenerate the key and update it in Vercel AI Gateway provider settings in Mav.".to_string();
    }

    if lower.contains("invalid api key") || lower.contains("invalid_api_key") {
        return "Authentication failed for Vercel AI Gateway. Check that your Vercel AI Gateway key starts with vck_ and is active.".to_string();
    }

    message.to_string()
}

fn has_tag(tags: &[String], expected: &str) -> bool {
    tags.iter()
        .any(|tag| tag.trim().eq_ignore_ascii_case(expected))
}
