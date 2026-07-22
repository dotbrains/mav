use super::*;

/// Perform Dynamic Client Registration with the authorization server.
pub async fn perform_dcr(
    http_client: &Arc<dyn HttpClient>,
    registration_endpoint: &Url,
    redirect_uri: &str,
    server_grant_types: Option<&[String]>,
) -> Result<OAuthClientRegistration> {
    validate_oauth_url(registration_endpoint)?;

    let body = dcr_registration_body(redirect_uri, server_grant_types);
    let body_bytes = serde_json::to_vec(&body)?;

    let request = Request::builder()
        .method(http_client::http::Method::POST)
        .uri(registration_endpoint.as_str())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(AsyncBody::from(body_bytes))?;

    let mut response = http_client.send(request).await?;

    if !response.status().is_success() {
        let mut error_body = String::new();
        response.body_mut().read_to_string(&mut error_body).await?;
        bail!(
            "DCR failed with status {}: {}",
            response.status(),
            error_body
        );
    }

    let mut response_body = String::new();
    response
        .body_mut()
        .read_to_string(&mut response_body)
        .await?;

    let dcr_response: DcrResponse =
        serde_json::from_str(&response_body).context("failed to parse DCR response")?;

    Ok(OAuthClientRegistration {
        client_id: dcr_response.client_id,
        client_secret: dcr_response.client_secret,
    })
}

// -- Token exchange and refresh (async) --------------------------------------
