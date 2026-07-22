use super::*;

/// The JSON body returned by the token endpoint on success.
#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
}
impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenResponse")
            .field("access_token", &"[redacted]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_in", &self.expires_in)
            .field("token_type", &self.token_type)
            .finish()
    }
}

impl TokenResponse {
    /// Convert into `OAuthTokens`, computing `expires_at` from `expires_in`.
    pub fn into_tokens(self) -> OAuthTokens {
        let expires_at = self
            .expires_in
            .map(|secs| SystemTime::now() + Duration::from_secs(secs));
        OAuthTokens {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at,
        }
    }
}

/// An OAuth token error response (RFC 6749 Section 5.2).
#[derive(Debug, Deserialize, PartialEq)]
pub struct OAuthTokenError {
    pub error: String,
    #[serde(default)]
    pub error_description: Option<String>,
}

impl std::fmt::Display for OAuthTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OAuth token error: {}", self.error)?;
        if let Some(description) = &self.error_description {
            write!(f, " ({description})")?;
        }
        Ok(())
    }
}

impl std::error::Error for OAuthTokenError {}

/// Build the form-encoded body for an authorization code token exchange.
pub fn token_exchange_params(
    code: &str,
    client_id: &str,
    redirect_uri: &str,
    code_verifier: &str,
    resource: &str,
    client_secret: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("client_id", client_id.to_string()),
        ("code_verifier", code_verifier.to_string()),
        ("resource", resource.to_string()),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret.to_string()));
    }
    params
}

/// Build the form-encoded body for a token refresh request.
pub fn token_refresh_params(
    refresh_token: &str,
    client_id: &str,
    resource: &str,
    client_secret: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
        ("client_id", client_id.to_string()),
        ("resource", resource.to_string()),
    ];
    if let Some(secret) = client_secret {
        params.push(("client_secret", secret.to_string()));
    }
    params
}
