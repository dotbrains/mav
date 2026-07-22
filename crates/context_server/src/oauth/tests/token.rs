use super::*;

#[test]
fn test_token_exchange_params() {
    let params = token_exchange_params(
        "auth_code_abc",
        "client_xyz",
        "http://127.0.0.1:5555/callback",
        "verifier_123",
        "https://mcp.example.com",
        None,
    );
    let map: std::collections::HashMap<&str, &str> =
        params.iter().map(|(k, v)| (*k, v.as_str())).collect();
    assert_eq!(map["grant_type"], "authorization_code");
    assert_eq!(map["code"], "auth_code_abc");
    assert_eq!(map["redirect_uri"], "http://127.0.0.1:5555/callback");
    assert_eq!(map["client_id"], "client_xyz");
    assert_eq!(map["code_verifier"], "verifier_123");
    assert_eq!(map["resource"], "https://mcp.example.com");
}

#[test]
fn test_token_refresh_params() {
    let params = token_refresh_params(
        "refresh_token_abc",
        "client_xyz",
        "https://mcp.example.com",
        None,
    );
    let map: std::collections::HashMap<&str, &str> =
        params.iter().map(|(k, v)| (*k, v.as_str())).collect();

    assert_eq!(map["grant_type"], "refresh_token");
    assert_eq!(map["refresh_token"], "refresh_token_abc");
    assert_eq!(map["client_id"], "client_xyz");
    assert_eq!(map["resource"], "https://mcp.example.com");
}

// -- Token response tests ------------------------------------------------

#[test]
fn test_token_response_into_tokens_with_expiry() {
    let response: TokenResponse = serde_json::from_str(
            r#"{"access_token": "at_123", "refresh_token": "rt_456", "expires_in": 3600, "token_type": "Bearer"}"#,
        )
        .unwrap();

    let tokens = response.into_tokens();
    assert_eq!(tokens.access_token, "at_123");
    assert_eq!(tokens.refresh_token.as_deref(), Some("rt_456"));
    assert!(tokens.expires_at.is_some());
}

#[test]
fn test_token_response_into_tokens_minimal() {
    let response: TokenResponse = serde_json::from_str(r#"{"access_token": "at_789"}"#).unwrap();

    let tokens = response.into_tokens();
    assert_eq!(tokens.access_token, "at_789");
    assert_eq!(tokens.refresh_token, None);
    assert_eq!(tokens.expires_at, None);
}

// -- DCR body test -------------------------------------------------------

#[test]
fn test_exchange_code_success() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("/token") {
                    json_response(
                        200,
                        r#"{
                                "access_token": "new_access_token",
                                "refresh_token": "new_refresh_token",
                                "expires_in": 3600,
                                "token_type": "Bearer"
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

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

        let tokens = exchange_code(
            &client,
            &metadata,
            "auth_code_123",
            CIMD_URL,
            "http://127.0.0.1:9999/callback",
            "verifier_abc",
            "https://mcp.example.com",
            None,
        )
        .await
        .unwrap();

        assert_eq!(tokens.access_token, "new_access_token");
        assert_eq!(tokens.refresh_token.as_deref(), Some("new_refresh_token"));
        assert!(tokens.expires_at.is_some());
    });
}

#[test]
fn test_refresh_tokens_success() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("/token") {
                    json_response(
                        200,
                        r#"{
                                "access_token": "refreshed_token",
                                "expires_in": 1800,
                                "token_type": "Bearer"
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let token_endpoint = Url::parse("https://auth.example.com/token").unwrap();

        let tokens = refresh_tokens(
            &client,
            &token_endpoint,
            "old_refresh_token",
            CIMD_URL,
            "https://mcp.example.com",
            None,
        )
        .await
        .unwrap();

        assert_eq!(tokens.access_token, "refreshed_token");
        assert_eq!(tokens.refresh_token, None);
        assert!(tokens.expires_at.is_some());
    });
}

#[test]
fn test_exchange_code_failure() {
    gpui::block_on(async {
        let client = make_fake_http_client(|_req| {
            Box::pin(async move { json_response(400, r#"{"error": "invalid_grant"}"#) })
        });

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

        let result = exchange_code(
            &client,
            &metadata,
            "bad_code",
            "client",
            "http://127.0.0.1:1/callback",
            "verifier",
            "https://mcp.example.com",
            None,
        )
        .await;

        let err = result.unwrap_err();
        let token_error = err
            .downcast_ref::<OAuthTokenError>()
            .expect("expected OAuthTokenError");
        assert_eq!(
            *token_error,
            OAuthTokenError {
                error: "invalid_grant".into(),
                error_description: None,
            }
        );
    });
}

// -- DCR integration tests -----------------------------------------------
