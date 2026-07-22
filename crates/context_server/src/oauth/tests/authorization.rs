use super::*;

#[test]
fn test_pkce_challenge_verifier_length() {
    let pkce = generate_pkce_challenge();
    // 32 random bytes → 43 base64url chars (no padding).
    assert_eq!(pkce.verifier.len(), 43);
}
#[test]
fn test_pkce_challenge_is_valid_base64url() {
    let pkce = generate_pkce_challenge();
    for c in pkce.verifier.chars().chain(pkce.challenge.chars()) {
        assert!(
            c.is_ascii_alphanumeric() || c == '-' || c == '_',
            "invalid base64url character: {}",
            c
        );
    }
}

#[test]
fn test_pkce_challenge_is_s256_of_verifier() {
    let pkce = generate_pkce_challenge();
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let expected_digest = Sha256::digest(pkce.verifier.as_bytes());
    let expected_challenge = engine.encode(expected_digest);
    assert_eq!(pkce.challenge, expected_challenge);
}

#[test]
fn test_pkce_challenges_are_unique() {
    let a = generate_pkce_challenge();
    let b = generate_pkce_challenge();
    assert_ne!(a.verifier, b.verifier);
}

// -- Authorization URL tests ---------------------------------------------

#[test]
fn test_build_authorization_url() {
    let metadata = AuthServerMetadata {
        issuer: Url::parse("https://auth.example.com").unwrap(),
        authorization_endpoint: Url::parse("https://auth.example.com/authorize").unwrap(),
        token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
        registration_endpoint: None,
        scopes_supported: None,
        code_challenge_methods_supported: Some(vec!["S256".into()]),
        client_id_metadata_document_supported: true,
        grant_types_supported: None,
    };
    let pkce = PkceChallenge {
        verifier: "test_verifier".into(),
        challenge: "test_challenge".into(),
    };
    let url = build_authorization_url(
        &metadata,
        "https://mav.dev/oauth/client-metadata.json",
        "http://127.0.0.1:12345/callback",
        &["files:read".into(), "files:write".into()],
        "https://mcp.example.com",
        &pkce,
        "random_state_123",
    );

    let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert_eq!(pairs.get("response_type").unwrap(), "code");
    assert_eq!(
        pairs.get("client_id").unwrap(),
        "https://mav.dev/oauth/client-metadata.json"
    );
    assert_eq!(
        pairs.get("redirect_uri").unwrap(),
        "http://127.0.0.1:12345/callback"
    );
    assert_eq!(pairs.get("scope").unwrap(), "files:read files:write");
    assert_eq!(pairs.get("resource").unwrap(), "https://mcp.example.com");
    assert_eq!(pairs.get("code_challenge").unwrap(), "test_challenge");
    assert_eq!(pairs.get("code_challenge_method").unwrap(), "S256");
    assert_eq!(pairs.get("state").unwrap(), "random_state_123");
}

#[test]
fn test_build_authorization_url_omits_empty_scope() {
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
    let pkce = PkceChallenge {
        verifier: "v".into(),
        challenge: "c".into(),
    };
    let url = build_authorization_url(
        &metadata,
        "client_123",
        "http://127.0.0.1:9999/callback",
        &[],
        "https://mcp.example.com",
        &pkce,
        "state",
    );

    let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
    assert!(!pairs.contains_key("scope"));
}

// -- Token exchange / refresh param tests --------------------------------
