use super::*;

/// Parsed from the MCP server's WWW-Authenticate header or well-known endpoint
/// per RFC 9728 (OAuth 2.0 Protected Resource Metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: Url,
    pub authorization_servers: Vec<Url>,
    pub scopes_supported: Option<Vec<String>>,
}
/// Parsed from the authorization server's .well-known endpoint
/// per RFC 8414 (OAuth 2.0 Authorization Server Metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthServerMetadata {
    pub issuer: Url,
    pub authorization_endpoint: Url,
    pub token_endpoint: Url,
    pub registration_endpoint: Option<Url>,
    pub scopes_supported: Option<Vec<String>>,
    pub grant_types_supported: Option<Vec<String>>,
    pub code_challenge_methods_supported: Option<Vec<String>>,
    pub client_id_metadata_document_supported: bool,
}

/// The result of client registration — either CIMD or DCR.
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthClientRegistration {
    pub client_id: String,
    /// Only present for DCR-minted registrations.
    pub client_secret: Option<String>,
}

impl std::fmt::Debug for OAuthClientRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthClientRegistration")
            .field("client_id", &self.client_id)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

/// Access and refresh tokens obtained from the token endpoint.
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<SystemTime>,
}

impl std::fmt::Debug for OAuthTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthTokens")
            .field("access_token", &"[redacted]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Everything discovered before the browser flow starts. Client registration is
/// resolved separately, once the real redirect URI is known.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthDiscovery {
    pub resource_metadata: ProtectedResourceMetadata,
    pub auth_server_metadata: AuthServerMetadata,
    pub scopes: Vec<String>,
}

/// The persisted OAuth session for a context server.
///
/// Stored in the keychain so startup can restore a refresh-capable provider
/// without another browser flow. Deliberately excludes the full discovery
/// metadata to keep the serialized size well within keychain item limits.
#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthSession {
    pub token_endpoint: Url,
    pub resource: Url,
    pub client_registration: OAuthClientRegistration,
    pub tokens: OAuthTokens,
}

impl std::fmt::Debug for OAuthSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuthSession")
            .field("token_endpoint", &self.token_endpoint)
            .field("resource", &self.resource)
            .field("client_registration", &self.client_registration)
            .field("tokens", &self.tokens)
            .finish()
    }
}

/// Error codes defined by RFC 6750 Section 3.1 for Bearer token authentication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BearerError {
    /// The request is missing a required parameter, includes an unsupported
    /// parameter or parameter value, or is otherwise malformed.
    InvalidRequest,
    /// The access token provided is expired, revoked, malformed, or invalid.
    InvalidToken,
    /// The request requires higher privileges than provided by the access token.
    InsufficientScope,
    /// An unrecognized error code (extension or future spec addition).
    Other,
}

impl BearerError {
    pub(super) fn parse(value: &str) -> Self {
        match value {
            "invalid_request" => BearerError::InvalidRequest,
            "invalid_token" => BearerError::InvalidToken,
            "insufficient_scope" => BearerError::InsufficientScope,
            _ => BearerError::Other,
        }
    }
}
