use super::*;

#[test]
fn test_mcp_oauth_provider_returns_none_when_token_expired() {
    let expired = SystemTime::now() - Duration::from_secs(60);
    let session = make_test_session("stale-token", Some("rt"), Some(expired));
    let provider = McpOAuthTokenProvider::new(
        session,
        make_fake_http_client(|_| Box::pin(async { unreachable!() })),
        None,
    );
    assert_eq!(provider.access_token(), None);
}

#[test]
fn test_mcp_oauth_provider_returns_token_when_not_expired() {
    let far_future = SystemTime::now() + Duration::from_secs(3600);
    let session = make_test_session("valid-token", Some("rt"), Some(far_future));
    let provider = McpOAuthTokenProvider::new(
        session,
        make_fake_http_client(|_| Box::pin(async { unreachable!() })),
        None,
    );

    assert_eq!(provider.access_token().as_deref(), Some("valid-token"));
}

#[test]
fn test_mcp_oauth_provider_returns_token_when_no_expiry() {
    let session = make_test_session("no-expiry-token", Some("rt"), None);
    let provider = McpOAuthTokenProvider::new(
        session,
        make_fake_http_client(|_| Box::pin(async { unreachable!() })),
        None,
    );

    assert_eq!(provider.access_token().as_deref(), Some("no-expiry-token"));
}

#[test]
fn test_mcp_oauth_provider_refresh_without_refresh_token_returns_false() {
    gpui::block_on(async {
        let session = make_test_session("token", None, None);
        let provider = McpOAuthTokenProvider::new(
            session,
            make_fake_http_client(|_| Box::pin(async { unreachable!("no HTTP call expected") })),
            None,
        );

        let refreshed = provider.try_refresh().await.unwrap();
        assert!(!refreshed);
    });
}

#[test]
fn test_mcp_oauth_provider_refresh_updates_session_and_notifies_channel() {
    gpui::block_on(async {
        let session = make_test_session("old-access", Some("my-refresh-token"), None);
        let (tx, mut rx) = futures::channel::mpsc::unbounded();

        let http_client = make_fake_http_client(|_req| {
            Box::pin(async {
                json_response(
                    200,
                    r#"{
                            "access_token": "new-access",
                            "refresh_token": "new-refresh",
                            "expires_in": 1800
                        }"#,
                )
            })
        });

        let provider = McpOAuthTokenProvider::new(session, http_client, Some(tx));

        let refreshed = provider.try_refresh().await.unwrap();
        assert!(refreshed);
        assert_eq!(provider.access_token().as_deref(), Some("new-access"));

        let notified_session = rx.try_recv().expect("channel should have a session");
        assert_eq!(notified_session.tokens.access_token, "new-access");
        assert_eq!(
            notified_session.tokens.refresh_token.as_deref(),
            Some("new-refresh")
        );
    });
}

#[test]
fn test_mcp_oauth_provider_refresh_preserves_old_refresh_token_when_server_omits_it() {
    gpui::block_on(async {
        let session = make_test_session("old-access", Some("original-refresh"), None);
        let (tx, mut rx) = futures::channel::mpsc::unbounded();

        let http_client = make_fake_http_client(|_req| {
            Box::pin(async {
                json_response(
                    200,
                    r#"{
                            "access_token": "new-access",
                            "expires_in": 900
                        }"#,
                )
            })
        });

        let provider = McpOAuthTokenProvider::new(session, http_client, Some(tx));

        let refreshed = provider.try_refresh().await.unwrap();
        assert!(refreshed);

        let notified_session = rx.try_recv().expect("channel should have a session");
        assert_eq!(notified_session.tokens.access_token, "new-access");
        assert_eq!(
            notified_session.tokens.refresh_token.as_deref(),
            Some("original-refresh"),
        );
    });
}

#[test]
fn test_mcp_oauth_provider_refresh_returns_false_on_http_error() {
    gpui::block_on(async {
        let session = make_test_session("old-access", Some("my-refresh"), None);

        let http_client = make_fake_http_client(|_req| {
            Box::pin(async { json_response(401, r#"{"error": "invalid_grant"}"#) })
        });

        let provider = McpOAuthTokenProvider::new(session, http_client, None);

        let refreshed = provider.try_refresh().await.unwrap();
        assert!(!refreshed);
        // The old token should still be in place.
        assert_eq!(provider.access_token().as_deref(), Some("old-access"));
    });
}
