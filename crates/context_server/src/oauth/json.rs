use super::*;

pub(super) async fn fetch_json<T: serde::de::DeserializeOwned>(
    http_client: &Arc<dyn HttpClient>,
    url: &Url,
) -> Result<T> {
    validate_oauth_url(url)?;
    let request = Request::builder()
        .method(http_client::http::Method::GET)
        .uri(url.as_str())
        .header("Accept", "application/json")
        .body(AsyncBody::default())?;

    let mut response = http_client.send(request).await?;

    if !response.status().is_success() {
        bail!("HTTP {} fetching {}", response.status(), url);
    }

    let mut body = String::new();
    response.body_mut().read_to_string(&mut body).await?;
    serde_json::from_str(&body).with_context(|| format!("failed to parse JSON from {}", url))
}

// -- Serde response types for discovery --------------------------------------

#[derive(Debug, Deserialize)]
pub(super) struct ProtectedResourceMetadataResponse {
    #[serde(default)]
    pub(super) resource: Option<Url>,
    #[serde(default)]
    pub(super) authorization_servers: Vec<Url>,
    #[serde(default)]
    pub(super) scopes_supported: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct AuthServerMetadataResponse {
    #[serde(default)]
    pub(super) issuer: Option<Url>,
    #[serde(default)]
    pub(super) authorization_endpoint: Option<Url>,
    #[serde(default)]
    pub(super) token_endpoint: Option<Url>,
    #[serde(default)]
    pub(super) registration_endpoint: Option<Url>,
    #[serde(default)]
    pub(super) scopes_supported: Option<Vec<String>>,
    #[serde(default)]
    pub(super) grant_types_supported: Option<Vec<String>>,
    #[serde(default)]
    pub(super) code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    pub(super) client_id_metadata_document_supported: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DcrResponse {
    pub(super) client_id: String,
    #[serde(default)]
    pub(super) client_secret: Option<String>,
}
