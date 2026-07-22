use super::*;

pub fn protected_resource_metadata_urls(server_url: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    let base = format!("{}://{}", server_url.scheme(), server_url.authority());
    let path = server_url.path().trim_start_matches('/');
    if !path.is_empty() {
        if let Ok(url) = Url::parse(&format!(
            "{}/.well-known/oauth-protected-resource/{}",
            base, path
        )) {
            urls.push(url);
        }
    }

    if let Ok(url) = Url::parse(&format!("{}/.well-known/oauth-protected-resource", base)) {
        urls.push(url);
    }

    urls
}

/// Construct the well-known Authorization Server Metadata URIs for a given
/// issuer URL, per RFC 8414 Section 3.1 and Section 5 (OIDC compat).
///
/// Returns URIs in priority order, which differs depending on whether the
/// issuer URL has a path component.
pub fn auth_server_metadata_urls(issuer: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    let base = format!("{}://{}", issuer.scheme(), issuer.authority());
    let path = issuer.path().trim_matches('/');

    if !path.is_empty() {
        // Issuer with path: try path-inserted variants first.
        if let Ok(url) = Url::parse(&format!(
            "{}/.well-known/oauth-authorization-server/{}",
            base, path
        )) {
            urls.push(url);
        }
        if let Ok(url) = Url::parse(&format!(
            "{}/.well-known/openid-configuration/{}",
            base, path
        )) {
            urls.push(url);
        }
        if let Ok(url) = Url::parse(&format!(
            "{}/{}/.well-known/openid-configuration",
            base, path
        )) {
            urls.push(url);
        }
    } else {
        // No path: standard well-known locations.
        if let Ok(url) = Url::parse(&format!("{}/.well-known/oauth-authorization-server", base)) {
            urls.push(url);
        }
        if let Ok(url) = Url::parse(&format!("{}/.well-known/openid-configuration", base)) {
            urls.push(url);
        }
    }

    urls
}

// -- Canonical server URI (RFC 8707) -----------------------------------------

/// Derive the canonical resource URI for an MCP server URL, suitable for the
/// `resource` parameter in authorization and token requests per RFC 8707.
///
/// Lowercases the scheme and host, preserves the path (without trailing slash),
/// strips fragments and query strings.
pub fn canonical_server_uri(server_url: &Url) -> String {
    let mut uri = format!(
        "{}://{}",
        server_url.scheme().to_ascii_lowercase(),
        server_url.host_str().unwrap_or("").to_ascii_lowercase(),
    );
    if let Some(port) = server_url.port() {
        uri.push_str(&format!(":{}", port));
    }
    let path = server_url.path();
    if path != "/" {
        uri.push_str(path.trim_end_matches('/'));
    }
    uri
}

// -- Scope selection ---------------------------------------------------------

/// Select scopes following the MCP spec's Scope Selection Strategy:
/// 1. Use `scope` from the `WWW-Authenticate` challenge if present.
/// 2. Fall back to `scopes_supported` from Protected Resource Metadata.
/// 3. Return empty if neither is available.
pub fn select_scopes(
    www_authenticate: &WwwAuthenticate,
    resource_metadata: &ProtectedResourceMetadata,
) -> Vec<String> {
    if let Some(ref scopes) = www_authenticate.scope {
        if !scopes.is_empty() {
            return scopes.clone();
        }
    }
    resource_metadata
        .scopes_supported
        .clone()
        .unwrap_or_default()
}
