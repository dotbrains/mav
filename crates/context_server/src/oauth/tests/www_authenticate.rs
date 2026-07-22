use super::*;

#[test]
fn test_parse_www_authenticate_with_resource_metadata_and_scope() {
    let header = r#"Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", scope="files:read user:profile""#;
    let result = parse_www_authenticate(header).unwrap();
    assert_eq!(
        result.resource_metadata.as_ref().map(|u| u.as_str()),
        Some("https://mcp.example.com/.well-known/oauth-protected-resource")
    );
    assert_eq!(
        result.scope,
        Some(vec!["files:read".to_string(), "user:profile".to_string()])
    );
    assert_eq!(result.error, None);
}

#[test]
fn test_parse_www_authenticate_resource_metadata_only() {
    let header = r#"Bearer resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource""#;
    let result = parse_www_authenticate(header).unwrap();

    assert_eq!(
        result.resource_metadata.as_ref().map(|u| u.as_str()),
        Some("https://mcp.example.com/.well-known/oauth-protected-resource")
    );
    assert_eq!(result.scope, None);
}

#[test]
fn test_parse_www_authenticate_bare_bearer() {
    let result = parse_www_authenticate("Bearer").unwrap();
    assert_eq!(result.resource_metadata, None);
    assert_eq!(result.scope, None);
}

#[test]
fn test_parse_www_authenticate_with_error() {
    let header = r#"Bearer error="insufficient_scope", scope="files:read files:write", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", error_description="Additional file write permission required""#;
    let result = parse_www_authenticate(header).unwrap();

    assert_eq!(result.error, Some(BearerError::InsufficientScope));
    assert_eq!(
        result.error_description.as_deref(),
        Some("Additional file write permission required")
    );
    assert_eq!(
        result.scope,
        Some(vec!["files:read".to_string(), "files:write".to_string()])
    );
    assert!(result.resource_metadata.is_some());
}

#[test]
fn test_parse_www_authenticate_invalid_token_error() {
    let header = r#"Bearer error="invalid_token", error_description="The access token expired""#;
    let result = parse_www_authenticate(header).unwrap();
    assert_eq!(result.error, Some(BearerError::InvalidToken));
}

#[test]
fn test_parse_www_authenticate_invalid_request_error() {
    let header = r#"Bearer error="invalid_request""#;
    let result = parse_www_authenticate(header).unwrap();
    assert_eq!(result.error, Some(BearerError::InvalidRequest));
}

#[test]
fn test_parse_www_authenticate_unknown_error() {
    let header = r#"Bearer error="some_future_error""#;
    let result = parse_www_authenticate(header).unwrap();
    assert_eq!(result.error, Some(BearerError::Other));
}

#[test]
fn test_parse_www_authenticate_rejects_non_bearer() {
    let result = parse_www_authenticate("Basic realm=\"example\"");
    assert!(result.is_err());
}

#[test]
fn test_parse_www_authenticate_case_insensitive_scheme() {
    let header =
        r#"bearer resource_metadata="https://example.com/.well-known/oauth-protected-resource""#;
    let result = parse_www_authenticate(header).unwrap();
    assert!(result.resource_metadata.is_some());
}

#[test]
fn test_parse_www_authenticate_multiline_style() {
    // Some servers emit the header spread across multiple lines joined by
    // whitespace, as shown in the spec examples.
    let header = "Bearer resource_metadata=\"https://mcp.example.com/.well-known/oauth-protected-resource\",\n                         scope=\"files:read\"";
    let result = parse_www_authenticate(header).unwrap();
    assert!(result.resource_metadata.is_some());
    assert_eq!(result.scope, Some(vec!["files:read".to_string()]));
}
