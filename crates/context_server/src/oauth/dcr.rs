const SUPPORTED_GRANT_TYPES: &[&str] = &["authorization_code", "refresh_token"];
pub fn dcr_registration_body(
    redirect_uri: &str,
    server_grant_types: Option<&[String]>,
) -> serde_json::Value {
    // Use the intersection of what we support and what the server advertises.
    // When the server doesn't advertise grant_types_supported, send all of
    // ours — the server will reject what it doesn't like.
    let grant_types: Vec<&str> = match server_grant_types {
        Some(server) => SUPPORTED_GRANT_TYPES
            .iter()
            .copied()
            .filter(|gt| server.iter().any(|s| s == *gt))
            .collect(),
        None => SUPPORTED_GRANT_TYPES.to_vec(),
    };

    serde_json::json!({
        "client_name": "Mav",
        "redirect_uris": [redirect_uri],
        "grant_types": grant_types,
        "response_types": ["code"],
        "token_endpoint_auth_method": "none"
    })
}
