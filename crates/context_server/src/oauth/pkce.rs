use super::*;

pub struct PkceChallenge {
    pub verifier: String,
    pub challenge: String,
}
impl std::fmt::Debug for PkceChallenge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkceChallenge")
            .field("verifier", &"[redacted]")
            .field("challenge", &self.challenge)
            .finish()
    }
}

/// Generate a PKCE code verifier and S256 challenge per RFC 7636.
///
/// The verifier is 43 base64url characters derived from 32 random bytes.
/// The challenge is `BASE64URL(SHA256(verifier))`.
pub fn generate_pkce_challenge() -> PkceChallenge {
    let mut random_bytes = [0u8; 32];
    rand::rng().fill(&mut random_bytes);
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let verifier = engine.encode(&random_bytes);

    let digest = Sha256::digest(verifier.as_bytes());
    let challenge = engine.encode(digest);

    PkceChallenge {
        verifier,
        challenge,
    }
}

// -- Authorization URL construction ------------------------------------------

/// Build the authorization URL for the OAuth Authorization Code + PKCE flow.
pub fn build_authorization_url(
    auth_server_metadata: &AuthServerMetadata,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    resource: &str,
    pkce: &PkceChallenge,
    state: &str,
) -> Url {
    let mut url = auth_server_metadata.authorization_endpoint.clone();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        if !scopes.is_empty() {
            query.append_pair("scope", &scopes.join(" "));
        }
        query.append_pair("resource", resource);
        query.append_pair("code_challenge", &pkce.challenge);
        query.append_pair("code_challenge_method", "S256");
        query.append_pair("state", state);
    }
    url
}
