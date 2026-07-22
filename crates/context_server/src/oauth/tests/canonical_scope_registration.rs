use super::*;

#[test]
fn test_canonical_server_uri_simple() {
    let url = Url::parse("https://mcp.example.com").unwrap();
    assert_eq!(canonical_server_uri(&url), "https://mcp.example.com");
}
#[test]
fn test_canonical_server_uri_with_path() {
    let url = Url::parse("https://mcp.example.com/v1/mcp").unwrap();
    assert_eq!(canonical_server_uri(&url), "https://mcp.example.com/v1/mcp");
}

#[test]
fn test_canonical_server_uri_strips_trailing_slash() {
    let url = Url::parse("https://mcp.example.com/").unwrap();
    assert_eq!(canonical_server_uri(&url), "https://mcp.example.com");
}

#[test]
fn test_canonical_server_uri_preserves_port() {
    let url = Url::parse("https://mcp.example.com:8443").unwrap();
    assert_eq!(canonical_server_uri(&url), "https://mcp.example.com:8443");
}

#[test]
fn test_canonical_server_uri_lowercases() {
    let url = Url::parse("HTTPS://MCP.Example.COM/Server/MCP").unwrap();
    assert_eq!(
        canonical_server_uri(&url),
        "https://mcp.example.com/Server/MCP"
    );
}

// -- Scope selection tests -----------------------------------------------

#[test]
fn test_select_scopes_prefers_www_authenticate() {
    let www_auth = WwwAuthenticate {
        resource_metadata: None,
        scope: Some(vec!["files:read".into()]),
        error: None,
        error_description: None,
    };
    let resource_meta = ProtectedResourceMetadata {
        resource: Url::parse("https://example.com").unwrap(),
        authorization_servers: vec![],
        scopes_supported: Some(vec!["files:read".into(), "files:write".into()]),
    };
    assert_eq!(select_scopes(&www_auth, &resource_meta), vec!["files:read"]);
}

#[test]
fn test_select_scopes_falls_back_to_resource_metadata() {
    let www_auth = WwwAuthenticate {
        resource_metadata: None,
        scope: None,
        error: None,
        error_description: None,
    };
    let resource_meta = ProtectedResourceMetadata {
        resource: Url::parse("https://example.com").unwrap(),
        authorization_servers: vec![],
        scopes_supported: Some(vec!["admin".into()]),
    };
    assert_eq!(select_scopes(&www_auth, &resource_meta), vec!["admin"]);
}

#[test]
fn test_select_scopes_empty_when_nothing_available() {
    let www_auth = WwwAuthenticate {
        resource_metadata: None,
        scope: None,
        error: None,
        error_description: None,
    };
    let resource_meta = ProtectedResourceMetadata {
        resource: Url::parse("https://example.com").unwrap(),
        authorization_servers: vec![],
        scopes_supported: None,
    };
    assert!(select_scopes(&www_auth, &resource_meta).is_empty());
}

// -- Client registration strategy tests ----------------------------------

#[test]
fn test_registration_strategy_prefers_cimd() {
    let metadata = AuthServerMetadata {
        issuer: Url::parse("https://auth.example.com").unwrap(),
        authorization_endpoint: Url::parse("https://auth.example.com/authorize").unwrap(),
        token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
        registration_endpoint: Some(Url::parse("https://auth.example.com/register").unwrap()),
        scopes_supported: None,
        code_challenge_methods_supported: Some(vec!["S256".into()]),
        client_id_metadata_document_supported: true,
        grant_types_supported: None,
    };
    assert_eq!(
        determine_registration_strategy(&metadata),
        ClientRegistrationStrategy::Cimd {
            client_id: CIMD_URL.to_string(),
        }
    );
}

#[test]
fn test_registration_strategy_falls_back_to_dcr() {
    let reg_endpoint = Url::parse("https://auth.example.com/register").unwrap();
    let metadata = AuthServerMetadata {
        issuer: Url::parse("https://auth.example.com").unwrap(),
        authorization_endpoint: Url::parse("https://auth.example.com/authorize").unwrap(),
        token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
        registration_endpoint: Some(reg_endpoint.clone()),
        scopes_supported: None,
        code_challenge_methods_supported: Some(vec!["S256".into()]),
        client_id_metadata_document_supported: false,
        grant_types_supported: None,
    };
    assert_eq!(
        determine_registration_strategy(&metadata),
        ClientRegistrationStrategy::Dcr {
            registration_endpoint: reg_endpoint,
        }
    );
}

#[test]
fn test_registration_strategy_unavailable() {
    let metadata = AuthServerMetadata {
        issuer: Url::parse("https://auth.example.com").unwrap(),
        authorization_endpoint: Url::parse("https://auth.example.com/authorize").unwrap(),
        token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
        registration_endpoint: None,
        scopes_supported: None,
        code_challenge_methods_supported: Some(vec!["S256".into()]),
        client_id_metadata_document_supported: false,
        grant_types_supported: None,
    };
    assert_eq!(
        determine_registration_strategy(&metadata),
        ClientRegistrationStrategy::Unavailable,
    );
}

// -- PKCE tests ----------------------------------------------------------
