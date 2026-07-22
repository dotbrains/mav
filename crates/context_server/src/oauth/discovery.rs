use super::*;

/// Fetch Protected Resource Metadata from the MCP server.
///
/// Tries the `resource_metadata` URL from the `WWW-Authenticate` header first,
/// then falls back to well-known URIs constructed from `server_url`.
pub async fn fetch_protected_resource_metadata(
    http_client: &Arc<dyn HttpClient>,
    server_url: &Url,
    www_authenticate: &WwwAuthenticate,
) -> Result<ProtectedResourceMetadata> {
    let candidate_urls = match &www_authenticate.resource_metadata {
        Some(url) if url.origin() == server_url.origin() => {
            // Try the header-provided URL first (per MCP spec: "use the resource
            // metadata URL from the parsed WWW-Authenticate headers when present"),
            // then fall back to RFC 9728 well-known URIs in case the header URL is
            // wrong (e.g. a buggy server that doubles the path component).
            let mut urls = vec![url.clone()];
            for fallback in protected_resource_metadata_urls(server_url) {
                if !urls.contains(&fallback) {
                    urls.push(fallback);
                }
            }
            urls
        }
        Some(url) => {
            log::warn!(
                "Ignoring cross-origin resource_metadata URL {} \
                 (server origin: {})",
                url,
                server_url.origin().unicode_serialization()
            );
            protected_resource_metadata_urls(server_url)
        }
        None => protected_resource_metadata_urls(server_url),
    };
    for url in &candidate_urls {
        match fetch_json::<ProtectedResourceMetadataResponse>(http_client, url).await {
            Ok(response) => {
                if response.authorization_servers.is_empty() {
                    bail!(
                        "Protected Resource Metadata at {} has no authorization_servers",
                        url
                    );
                }
                return Ok(ProtectedResourceMetadata {
                    resource: response.resource.unwrap_or_else(|| server_url.clone()),
                    authorization_servers: response.authorization_servers,
                    scopes_supported: response.scopes_supported,
                });
            }
            Err(err) => {
                log::debug!(
                    "Failed to fetch Protected Resource Metadata from {}: {}",
                    url,
                    err
                );
            }
        }
    }

    bail!(
        "Could not fetch Protected Resource Metadata for {}",
        server_url
    )
}

/// Fetch Authorization Server Metadata, trying RFC 8414 and OIDC Discovery
/// endpoints in the priority order specified by the MCP spec.
pub async fn fetch_auth_server_metadata(
    http_client: &Arc<dyn HttpClient>,
    issuer: &Url,
) -> Result<AuthServerMetadata> {
    let candidate_urls = auth_server_metadata_urls(issuer);

    for url in &candidate_urls {
        match fetch_json::<AuthServerMetadataResponse>(http_client, url).await {
            Ok(response) => {
                let reported_issuer = response.issuer.unwrap_or_else(|| issuer.clone());

                if reported_issuer != *issuer {
                    bail!(
                        "Auth server metadata issuer mismatch: expected {}, got {}",
                        issuer,
                        reported_issuer
                    );
                }

                return Ok(AuthServerMetadata {
                    issuer: reported_issuer,
                    grant_types_supported: response.grant_types_supported,
                    authorization_endpoint: response
                        .authorization_endpoint
                        .ok_or_else(|| anyhow!("missing authorization_endpoint"))?,
                    token_endpoint: response
                        .token_endpoint
                        .ok_or_else(|| anyhow!("missing token_endpoint"))?,
                    registration_endpoint: response.registration_endpoint,
                    scopes_supported: response.scopes_supported,
                    code_challenge_methods_supported: response.code_challenge_methods_supported,
                    client_id_metadata_document_supported: response
                        .client_id_metadata_document_supported
                        .unwrap_or(false),
                });
            }
            Err(err) => {
                log::debug!("Failed to fetch Auth Server Metadata from {}: {}", url, err);
            }
        }
    }

    bail!(
        "Could not fetch Authorization Server Metadata for {}",
        issuer
    )
}

/// Run the full discovery flow: fetch resource metadata, then auth server
/// metadata, then select scopes. Client registration is resolved separately,
/// once the real redirect URI is known.
pub async fn discover(
    http_client: &Arc<dyn HttpClient>,
    server_url: &Url,
    www_authenticate: &WwwAuthenticate,
) -> Result<OAuthDiscovery> {
    let resource_metadata =
        fetch_protected_resource_metadata(http_client, server_url, www_authenticate).await?;

    let auth_server_url = resource_metadata
        .authorization_servers
        .first()
        .ok_or_else(|| anyhow!("no authorization servers in resource metadata"))?;

    let auth_server_metadata = fetch_auth_server_metadata(http_client, auth_server_url).await?;

    // Verify PKCE S256 support (spec requirement).
    match &auth_server_metadata.code_challenge_methods_supported {
        Some(methods) if methods.iter().any(|m| m == "S256") => {}
        Some(_) => bail!("authorization server does not support S256 PKCE"),
        None => bail!("authorization server does not advertise code_challenge_methods_supported"),
    }

    let scopes = select_scopes(www_authenticate, &resource_metadata);

    Ok(OAuthDiscovery {
        resource_metadata,
        auth_server_metadata,
        scopes,
    })
}

/// Resolve the OAuth client registration for an authorization flow.
///
/// CIMD uses the static client metadata document directly. For DCR, a fresh
/// registration is performed each time because the loopback redirect URI
/// includes an ephemeral port that changes every flow.
pub async fn resolve_client_registration(
    http_client: &Arc<dyn HttpClient>,
    discovery: &OAuthDiscovery,
    redirect_uri: &str,
) -> Result<OAuthClientRegistration> {
    match determine_registration_strategy(&discovery.auth_server_metadata) {
        ClientRegistrationStrategy::Cimd { client_id } => Ok(OAuthClientRegistration {
            client_id,
            client_secret: None,
        }),
        ClientRegistrationStrategy::Dcr {
            registration_endpoint,
        } => {
            perform_dcr(
                http_client,
                &registration_endpoint,
                redirect_uri,
                discovery
                    .auth_server_metadata
                    .grant_types_supported
                    .as_deref(),
            )
            .await
        }
        ClientRegistrationStrategy::Unavailable => {
            bail!("authorization server supports neither CIMD nor DCR")
        }
    }
}
