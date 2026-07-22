use super::*;
use http_client::Response;

mod authorization;
mod callback;
mod canonical_scope_registration;
mod dcr;
mod discovery;
mod metadata_urls;
mod provider;
mod token;
mod validation;
mod www_authenticate;

pub(super) fn make_fake_http_client(
    handler: impl Fn(
        http_client::Request<AsyncBody>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send>,
    > + Send
    + Sync
    + 'static,
) -> Arc<dyn HttpClient> {
    http_client::FakeHttpClient::create(handler) as Arc<dyn HttpClient>
}

pub(super) fn json_response(status: u16, body: &str) -> anyhow::Result<Response<AsyncBody>> {
    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(AsyncBody::from(body.as_bytes().to_vec()))
        .unwrap())
}

pub(super) fn make_test_session(
    access_token: &str,
    refresh_token: Option<&str>,
    expires_at: Option<SystemTime>,
) -> OAuthSession {
    OAuthSession {
        token_endpoint: Url::parse("https://auth.example.com/token").unwrap(),
        resource: Url::parse("https://mcp.example.com").unwrap(),
        client_registration: OAuthClientRegistration {
            client_id: "test-client".into(),
            client_secret: None,
        },
        tokens: OAuthTokens {
            access_token: access_token.into(),
            refresh_token: refresh_token.map(String::from),
            expires_at,
        },
    }
}
