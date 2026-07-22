use super::*;

/// The registration approach to use, determined from auth server metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientRegistrationStrategy {
    /// The auth server supports CIMD. Use the CIMD URL as client_id directly.
    Cimd { client_id: String },
    /// The auth server has a registration endpoint. Caller must POST to it.
    Dcr { registration_endpoint: Url },
    /// No supported registration mechanism.
    Unavailable,
}
/// Determine how to register with the authorization server, following the
/// spec's recommended priority: CIMD first, DCR fallback.
pub fn determine_registration_strategy(
    auth_server_metadata: &AuthServerMetadata,
) -> ClientRegistrationStrategy {
    if auth_server_metadata.client_id_metadata_document_supported {
        ClientRegistrationStrategy::Cimd {
            client_id: CIMD_URL.to_string(),
        }
    } else if let Some(ref endpoint) = auth_server_metadata.registration_endpoint {
        ClientRegistrationStrategy::Dcr {
            registration_endpoint: endpoint.clone(),
        }
    } else {
        ClientRegistrationStrategy::Unavailable
    }
}
