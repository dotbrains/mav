use super::*;

/// Exchange an authorization code for tokens at the token endpoint.
pub async fn exchange_code(
    http_client: &Arc<dyn HttpClient>,
    auth_server_metadata: &AuthServerMetadata,
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
    resource: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens> {
    let params = token_exchange_params(
        code,
        client_id,
        redirect_uri,
        code_verifier,
        resource,
        client_secret,
    );
    post_token_request(http_client, &auth_server_metadata.token_endpoint, &params).await
}
/// Refresh tokens using a refresh token.
pub async fn refresh_tokens(
    http_client: &Arc<dyn HttpClient>,
    token_endpoint: &Url,
    refresh_token: &str,
    client_id: &str,
    resource: &str,
    client_secret: Option<&str>,
) -> Result<OAuthTokens> {
    let params = token_refresh_params(refresh_token, client_id, resource, client_secret);
    post_token_request(http_client, token_endpoint, &params).await
}

/// POST form-encoded parameters to a token endpoint and parse the response.
async fn post_token_request(
    http_client: &Arc<dyn HttpClient>,
    token_endpoint: &Url,
    params: &[(&str, String)],
) -> Result<OAuthTokens> {
    validate_oauth_url(token_endpoint)?;

    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
        .finish();

    let request = Request::builder()
        .method(http_client::http::Method::POST)
        .uri(token_endpoint.as_str())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(AsyncBody::from(body.into_bytes()))?;

    let mut response = http_client.send(request).await?;

    if !response.status().is_success() {
        let mut error_body = String::new();
        response.body_mut().read_to_string(&mut error_body).await?;
        let status = response.status();
        // Try to parse as an OAuth error response (RFC 6749 Section 5.2).
        if let Ok(token_error) = serde_json::from_str::<OAuthTokenError>(&error_body) {
            return Err(token_error.into());
        }
        bail!("token request failed with status {status}: {error_body}");
    }

    let mut response_body = String::new();
    response
        .body_mut()
        .read_to_string(&mut response_body)
        .await?;

    let token_response: TokenResponse =
        serde_json::from_str(&response_body).context("failed to parse token response")?;

    Ok(token_response.into_tokens())
}

// -- Loopback HTTP callback server -------------------------------------------
