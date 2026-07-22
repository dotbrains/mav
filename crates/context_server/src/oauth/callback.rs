use super::*;

pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}
impl std::fmt::Debug for OAuthCallback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthCallback")
            .field("code", &"[redacted]")
            .field("state", &"[redacted]")
            .finish()
    }
}

impl OAuthCallback {
    /// Parse the query string from a callback URL like
    /// `http://127.0.0.1:<port>/callback?code=...&state=...`.
    pub fn parse_query(query: &str) -> Result<Self> {
        let params = oauth_callback_server::OAuthCallbackParams::parse_query(query)?;
        Ok(Self {
            code: params.code,
            state: params.state,
        })
    }
}

/// Start a loopback HTTP server to receive the OAuth authorization callback.
///
/// Binds to an ephemeral loopback port for each flow.
///
/// Returns `(redirect_uri, callback_future)`. The caller should use the
/// redirect URI in the authorization request, open the browser, then await
/// the future to receive the callback.
///
/// The server accepts exactly one request on `/callback`, validates that it
/// contains `code` and `state` query parameters, responds with a minimal
/// HTML page telling the user they can close the tab, and shuts down.
///
/// The callback server shuts down when the returned future is dropped (e.g.
/// because the authentication task was cancelled), or after a timeout.
pub fn start_callback_server() -> Result<(String, BoxFuture<'static, Result<OAuthCallback>>)> {
    let (redirect_uri, rx) = oauth_callback_server::start_oauth_callback_server()?;
    let future = async move {
        match rx.await {
            Ok(Ok(params)) => Ok(OAuthCallback {
                code: params.code,
                state: params.state,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!(
                "OAuth callback server was shut down before receiving a response"
            )),
        }
    }
    .boxed();
    Ok((redirect_uri, future))
}
