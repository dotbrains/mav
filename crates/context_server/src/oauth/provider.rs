use super::*;

#[async_trait]
pub trait OAuthTokenProvider: Send + Sync {
    /// Returns the current access token, if one is available.
    fn access_token(&self) -> Option<String>;
    /// Attempts to refresh the access token. Returns `true` if a new token was
    /// obtained and the request should be retried.
    async fn try_refresh(&self) -> Result<bool>;
}

/// Concrete `OAuthTokenProvider` backed by a full persisted OAuth session and
/// an HTTP client for token refresh. The same provider type is used both after
/// an interactive authentication flow and when restoring a saved session from
/// the keychain on startup.
pub struct McpOAuthTokenProvider {
    session: SyncMutex<OAuthSession>,
    http_client: Arc<dyn HttpClient>,
    token_refresh_tx: Option<mpsc::UnboundedSender<OAuthSession>>,
}

impl McpOAuthTokenProvider {
    pub fn new(
        session: OAuthSession,
        http_client: Arc<dyn HttpClient>,
        token_refresh_tx: Option<mpsc::UnboundedSender<OAuthSession>>,
    ) -> Self {
        Self {
            session: SyncMutex::new(session),
            http_client,
            token_refresh_tx,
        }
    }

    fn access_token_is_expired(tokens: &OAuthTokens) -> bool {
        tokens.expires_at.is_some_and(|expires_at| {
            SystemTime::now()
                .checked_add(Duration::from_secs(30))
                .is_some_and(|now_with_buffer| expires_at <= now_with_buffer)
        })
    }
}

#[async_trait]
impl OAuthTokenProvider for McpOAuthTokenProvider {
    fn access_token(&self) -> Option<String> {
        let session = self.session.lock();
        if Self::access_token_is_expired(&session.tokens) {
            return None;
        }
        Some(session.tokens.access_token.clone())
    }

    async fn try_refresh(&self) -> Result<bool> {
        let (refresh_token, token_endpoint, resource, client_id, client_secret) = {
            let session = self.session.lock();
            match session.tokens.refresh_token.clone() {
                Some(refresh_token) => (
                    refresh_token,
                    session.token_endpoint.clone(),
                    session.resource.clone(),
                    session.client_registration.client_id.clone(),
                    session.client_registration.client_secret.clone(),
                ),
                None => return Ok(false),
            }
        };

        let resource_str = canonical_server_uri(&resource);

        match refresh_tokens(
            &self.http_client,
            &token_endpoint,
            &refresh_token,
            &client_id,
            &resource_str,
            client_secret.as_deref(),
        )
        .await
        {
            Ok(mut new_tokens) => {
                if new_tokens.refresh_token.is_none() {
                    new_tokens.refresh_token = Some(refresh_token);
                }

                {
                    let mut session = self.session.lock();
                    session.tokens = new_tokens;

                    if let Some(ref tx) = self.token_refresh_tx {
                        tx.unbounded_send(session.clone()).ok();
                    }
                }

                Ok(true)
            }
            Err(err) => {
                log::warn!("OAuth token refresh failed: {}", err);
                Ok(false)
            }
        }
    }
}
