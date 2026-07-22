use super::*;

#[test]
fn test_fetch_protected_resource_metadata() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains(".well-known/oauth-protected-resource") {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"],
                                "scopes_supported": ["read", "write"]
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });
        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: None,
            scope: None,
            error: None,
            error_description: None,
        };

        let metadata = fetch_protected_resource_metadata(&client, &server_url, &www_auth)
            .await
            .unwrap();

        assert_eq!(metadata.resource.as_str(), "https://mcp.example.com/");
        assert_eq!(metadata.authorization_servers.len(), 1);
        assert_eq!(
            metadata.authorization_servers[0].as_str(),
            "https://auth.example.com/"
        );
        assert_eq!(
            metadata.scopes_supported,
            Some(vec!["read".to_string(), "write".to_string()])
        );
    });
}

#[test]
fn test_fetch_protected_resource_metadata_prefers_www_authenticate_url() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri == "https://mcp.example.com/custom-resource-metadata" {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"]
                            }"#,
                    )
                } else {
                    json_response(500, r#"{"error": "should not be called"}"#)
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: Some(
                Url::parse("https://mcp.example.com/custom-resource-metadata").unwrap(),
            ),
            scope: None,
            error: None,
            error_description: None,
        };

        let metadata = fetch_protected_resource_metadata(&client, &server_url, &www_auth)
            .await
            .unwrap();

        assert_eq!(metadata.authorization_servers.len(), 1);
    });
}

#[test]
fn test_fetch_protected_resource_metadata_falls_back_when_header_url_fails() {
    // Reproduces the Pydantic Logfire case: the server's WWW-Authenticate
    // header contains a resource_metadata URL with a doubled path (e.g.
    // /mcp/mcp), which returns HTML instead of JSON. The client should
    // fall back to the RFC 9728 well-known URL, which works correctly.
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri == "https://mcp.example.com/.well-known/oauth-protected-resource/api/mcp/mcp"
                {
                    // Buggy header URL returns HTML (like a SPA catch-all).
                    Ok(Response::builder()
                        .status(200)
                        .header("Content-Type", "text/html")
                        .body(AsyncBody::from(b"<!doctype html><html></html>".to_vec()))
                        .unwrap())
                } else if uri
                    == "https://mcp.example.com/.well-known/oauth-protected-resource/api/mcp"
                {
                    // Correct well-known URL returns valid metadata.
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com/api/mcp",
                                "authorization_servers": ["https://auth.example.com"]
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com/api/mcp").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: Some(
                // Buggy URL with doubled path component.
                Url::parse(
                    "https://mcp.example.com/.well-known/oauth-protected-resource/api/mcp/mcp",
                )
                .unwrap(),
            ),
            scope: None,
            error: None,
            error_description: None,
        };

        let metadata = fetch_protected_resource_metadata(&client, &server_url, &www_auth)
            .await
            .unwrap();

        assert_eq!(
            metadata.resource.as_str(),
            "https://mcp.example.com/api/mcp"
        );
        assert_eq!(
            metadata.authorization_servers[0].as_str(),
            "https://auth.example.com/"
        );
    });
}

#[test]
fn test_fetch_protected_resource_metadata_rejects_cross_origin_url() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                // The cross-origin URL should NOT be fetched; only the
                // well-known fallback at the server's own origin should be.
                if uri.contains("attacker.example.com") {
                    panic!("should not fetch cross-origin resource_metadata URL");
                } else if uri.contains(".well-known/oauth-protected-resource") {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"]
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: Some(
                Url::parse("https://attacker.example.com/fake-metadata").unwrap(),
            ),
            scope: None,
            error: None,
            error_description: None,
        };

        let metadata = fetch_protected_resource_metadata(&client, &server_url, &www_auth)
            .await
            .unwrap();

        // Should have used the fallback well-known URL, not the attacker's.
        assert_eq!(metadata.resource.as_str(), "https://mcp.example.com/");
    });
}

#[test]
fn test_fetch_auth_server_metadata() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains(".well-known/oauth-authorization-server") {
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://auth.example.com",
                                "authorization_endpoint": "https://auth.example.com/authorize",
                                "token_endpoint": "https://auth.example.com/token",
                                "registration_endpoint": "https://auth.example.com/register",
                                "code_challenge_methods_supported": ["S256"],
                                "client_id_metadata_document_supported": true
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let issuer = Url::parse("https://auth.example.com").unwrap();
        let metadata = fetch_auth_server_metadata(&client, &issuer).await.unwrap();

        assert_eq!(metadata.issuer.as_str(), "https://auth.example.com/");
        assert_eq!(
            metadata.authorization_endpoint.as_str(),
            "https://auth.example.com/authorize"
        );
        assert_eq!(
            metadata.token_endpoint.as_str(),
            "https://auth.example.com/token"
        );
        assert!(metadata.registration_endpoint.is_some());
        assert!(metadata.client_id_metadata_document_supported);
        assert_eq!(
            metadata.code_challenge_methods_supported,
            Some(vec!["S256".to_string()])
        );
    });
}

#[test]
fn test_fetch_auth_server_metadata_falls_back_to_oidc() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("openid-configuration") {
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://auth.example.com",
                                "authorization_endpoint": "https://auth.example.com/authorize",
                                "token_endpoint": "https://auth.example.com/token",
                                "code_challenge_methods_supported": ["S256"]
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let issuer = Url::parse("https://auth.example.com").unwrap();
        let metadata = fetch_auth_server_metadata(&client, &issuer).await.unwrap();

        assert_eq!(
            metadata.authorization_endpoint.as_str(),
            "https://auth.example.com/authorize"
        );
        assert!(!metadata.client_id_metadata_document_supported);
    });
}

#[test]
fn test_fetch_auth_server_metadata_rejects_issuer_mismatch() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains(".well-known/oauth-authorization-server") {
                    // Response claims to be a different issuer.
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://evil.example.com",
                                "authorization_endpoint": "https://evil.example.com/authorize",
                                "token_endpoint": "https://evil.example.com/token",
                                "code_challenge_methods_supported": ["S256"]
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let issuer = Url::parse("https://auth.example.com").unwrap();
        let result = fetch_auth_server_metadata(&client, &issuer).await;

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("issuer mismatch"),
            "unexpected error: {}",
            err_msg
        );
    });
}

// -- Full discover integration tests -------------------------------------

#[test]
fn test_full_discover_with_cimd() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("oauth-protected-resource") {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"],
                                "scopes_supported": ["mcp:read"]
                            }"#,
                    )
                } else if uri.contains("oauth-authorization-server") {
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://auth.example.com",
                                "authorization_endpoint": "https://auth.example.com/authorize",
                                "token_endpoint": "https://auth.example.com/token",
                                "code_challenge_methods_supported": ["S256"],
                                "client_id_metadata_document_supported": true
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: None,
            scope: None,
            error: None,
            error_description: None,
        };

        let discovery = discover(&client, &server_url, &www_auth).await.unwrap();
        let registration =
            resolve_client_registration(&client, &discovery, "http://127.0.0.1:12345/callback")
                .await
                .unwrap();

        assert_eq!(registration.client_id, CIMD_URL);
        assert_eq!(registration.client_secret, None);
        assert_eq!(discovery.scopes, vec!["mcp:read"]);
    });
}

#[test]
fn test_full_discover_with_dcr_fallback() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("oauth-protected-resource") {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"]
                            }"#,
                    )
                } else if uri.contains("oauth-authorization-server") {
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://auth.example.com",
                                "authorization_endpoint": "https://auth.example.com/authorize",
                                "token_endpoint": "https://auth.example.com/token",
                                "registration_endpoint": "https://auth.example.com/register",
                                "code_challenge_methods_supported": ["S256"],
                                "client_id_metadata_document_supported": false
                            }"#,
                    )
                } else if uri.contains("/register") {
                    json_response(
                        201,
                        r#"{
                                "client_id": "dcr-minted-id-123",
                                "client_secret": "dcr-secret-456"
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: None,
            scope: Some(vec!["files:read".into()]),
            error: None,
            error_description: None,
        };

        let discovery = discover(&client, &server_url, &www_auth).await.unwrap();
        let registration =
            resolve_client_registration(&client, &discovery, "http://127.0.0.1:9999/callback")
                .await
                .unwrap();

        assert_eq!(registration.client_id, "dcr-minted-id-123");
        assert_eq!(
            registration.client_secret.as_deref(),
            Some("dcr-secret-456")
        );
        assert_eq!(discovery.scopes, vec!["files:read"]);
    });
}

#[test]
fn test_discover_fails_without_pkce_support() {
    gpui::block_on(async {
        let client = make_fake_http_client(|req| {
            Box::pin(async move {
                let uri = req.uri().to_string();
                if uri.contains("oauth-protected-resource") {
                    json_response(
                        200,
                        r#"{
                                "resource": "https://mcp.example.com",
                                "authorization_servers": ["https://auth.example.com"]
                            }"#,
                    )
                } else if uri.contains("oauth-authorization-server") {
                    json_response(
                        200,
                        r#"{
                                "issuer": "https://auth.example.com",
                                "authorization_endpoint": "https://auth.example.com/authorize",
                                "token_endpoint": "https://auth.example.com/token"
                            }"#,
                    )
                } else {
                    json_response(404, "{}")
                }
            })
        });

        let server_url = Url::parse("https://mcp.example.com").unwrap();
        let www_auth = WwwAuthenticate {
            resource_metadata: None,
            scope: None,
            error: None,
            error_description: None,
        };

        let result = discover(&client, &server_url, &www_auth).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("code_challenge_methods_supported"),
            "unexpected error: {}",
            err_msg
        );
    });
}

// -- Token exchange integration tests ------------------------------------
