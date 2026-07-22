use super::*;

pub(crate) async fn get_models(
    models_url: Arc<str>,
    oauth_token: String,
    client: Arc<dyn HttpClient>,
) -> Result<Vec<Model>> {
    let all_models = request_models(models_url, oauth_token, client).await?;

    let mut models: Vec<Model> = all_models
        .into_iter()
        .filter(|model| {
            model.model_picker_enabled
                && model.capabilities.model_type.as_str() == "chat"
                && model
                    .policy
                    .as_ref()
                    .is_none_or(|policy| policy.state == "enabled")
        })
        .collect();

    if let Some(default_model_position) = models.iter().position(|model| model.is_chat_default) {
        let default_model = models.remove(default_model_position);
        models.insert(0, default_model);
    }

    Ok(models)
}

#[derive(Deserialize)]
struct GraphQLResponse {
    data: Option<GraphQLData>,
}

#[derive(Deserialize)]
struct GraphQLData {
    viewer: GraphQLViewer,
}

#[derive(Deserialize)]
struct GraphQLViewer {
    #[serde(rename = "copilotEndpoints")]
    copilot_endpoints: GraphQLCopilotEndpoints,
}

#[derive(Deserialize)]
struct GraphQLCopilotEndpoints {
    api: String,
}

pub(crate) async fn discover_api_endpoint(
    oauth_token: &str,
    configuration: &CopilotChatConfiguration,
    client: &Arc<dyn HttpClient>,
) -> Result<String> {
    let graphql_url = configuration.graphql_url();
    let query = serde_json::json!({
        "query": "query { viewer { copilotEndpoints { api } } }"
    });

    let request = HttpRequest::builder()
        .method(Method::POST)
        .uri(graphql_url.as_str())
        .header("Authorization", format!("Bearer {}", oauth_token))
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(serde_json::to_string(&query)?))?;

    let mut response = client.send(request).await?;

    anyhow::ensure!(
        response.status().is_success(),
        "GraphQL endpoint discovery failed: {}",
        response.status()
    );

    let mut body = Vec::new();
    response.body_mut().read_to_end(&mut body).await?;
    let body_str = std::str::from_utf8(&body)?;

    let parsed: GraphQLResponse = serde_json::from_str(body_str)
        .context("Failed to parse GraphQL response for Copilot endpoint discovery")?;

    let data = parsed
        .data
        .context("GraphQL response contained no data field")?;

    Ok(data.viewer.copilot_endpoints.api)
}

pub(crate) fn copilot_request_headers(
    builder: http_client::Builder,
    oauth_token: &str,
    is_user_initiated: Option<bool>,
    location: Option<ChatLocation>,
) -> http_client::Builder {
    builder
        .header("Authorization", format!("Bearer {}", oauth_token))
        .header("Content-Type", "application/json")
        .header(
            "Editor-Version",
            format!(
                "Mav/{}",
                option_env!("CARGO_PKG_VERSION").unwrap_or("unknown")
            ),
        )
        .header("X-GitHub-Api-Version", "2025-10-01")
        .when_some(is_user_initiated, |builder, is_user_initiated| {
            builder.header(
                "X-Initiator",
                if is_user_initiated { "user" } else { "agent" },
            )
        })
        .when_some(location, |builder, loc| {
            let interaction_type = loc.to_intent_string();
            builder
                .header("X-Interaction-Type", interaction_type)
                .header("OpenAI-Intent", interaction_type)
        })
}

async fn request_models(
    models_url: Arc<str>,
    oauth_token: String,
    client: Arc<dyn HttpClient>,
) -> Result<Vec<Model>> {
    let request_builder = copilot_request_headers(
        HttpRequest::builder()
            .method(Method::GET)
            .uri(models_url.as_ref()),
        &oauth_token,
        None,
        None,
    );

    let request = request_builder.body(AsyncBody::empty())?;

    let mut response = client.send(request).await?;

    anyhow::ensure!(
        response.status().is_success(),
        "Failed to request models: {}",
        response.status()
    );
    let mut body = Vec::new();
    response.body_mut().read_to_end(&mut body).await?;

    let body_str = std::str::from_utf8(&body)?;

    let models = serde_json::from_str::<ModelSchema>(body_str)?.data;

    Ok(models)
}
