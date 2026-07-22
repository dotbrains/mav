use super::*;

#[test]
fn test_dcr_registration_body_without_server_metadata() {
    // When server metadata is unavailable, include all supported grant types.
    let body = dcr_registration_body("http://127.0.0.1:12345/callback", None);
    assert_eq!(body["client_name"], "Mav");
    assert_eq!(body["redirect_uris"][0], "http://127.0.0.1:12345/callback");
    assert_eq!(body["grant_types"][0], "authorization_code");
    assert_eq!(body["grant_types"][1], "refresh_token");
    assert_eq!(body["response_types"][0], "code");
    assert_eq!(body["token_endpoint_auth_method"], "none");
}
#[test]
fn test_dcr_registration_body_mirrors_server_grant_types() {
    // When the server only supports authorization_code, omit refresh_token.
    let server_types = vec!["authorization_code".to_string()];
    let body = dcr_registration_body("http://127.0.0.1:12345/callback", Some(&server_types));
    assert_eq!(body["grant_types"][0], "authorization_code");
    assert!(body["grant_types"].as_array().unwrap().len() == 1);

    // When the server supports both, include both.
    let server_types = vec![
        "authorization_code".to_string(),
        "refresh_token".to_string(),
    ];
    let body = dcr_registration_body("http://127.0.0.1:12345/callback", Some(&server_types));
    assert_eq!(body["grant_types"][0], "authorization_code");
    assert_eq!(body["grant_types"][1], "refresh_token");
}

#[test]
fn test_perform_dcr() {
    gpui::block_on(async {
        let client = make_fake_http_client(|_req| {
            Box::pin(async move {
                json_response(
                    201,
                    r#"{
                            "client_id": "dynamic-client-001",
                            "client_secret": "dynamic-secret-001"
                        }"#,
                )
            })
        });

        let endpoint = Url::parse("https://auth.example.com/register").unwrap();
        let registration = perform_dcr(&client, &endpoint, "http://127.0.0.1:9999/callback", None)
            .await
            .unwrap();

        assert_eq!(registration.client_id, "dynamic-client-001");
        assert_eq!(
            registration.client_secret.as_deref(),
            Some("dynamic-secret-001")
        );
    });
}

#[test]
fn test_perform_dcr_failure() {
    gpui::block_on(async {
        let client = make_fake_http_client(|_req| {
            Box::pin(async move { json_response(403, r#"{"error": "registration_not_allowed"}"#) })
        });

        let endpoint = Url::parse("https://auth.example.com/register").unwrap();
        let result = perform_dcr(&client, &endpoint, "http://127.0.0.1:9999/callback", None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("403"));
    });
}

// -- OAuthCallback parse tests -------------------------------------------
