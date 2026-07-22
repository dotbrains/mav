use super::*;

#[test]
fn test_protected_resource_metadata_urls_with_path() {
    let server_url = Url::parse("https://api.example.com/v1/mcp").unwrap();
    let urls = protected_resource_metadata_urls(&server_url);
    assert_eq!(urls.len(), 2);
    assert_eq!(
        urls[0].as_str(),
        "https://api.example.com/.well-known/oauth-protected-resource/v1/mcp"
    );
    assert_eq!(
        urls[1].as_str(),
        "https://api.example.com/.well-known/oauth-protected-resource"
    );
}

#[test]
fn test_protected_resource_metadata_urls_without_path() {
    let server_url = Url::parse("https://mcp.example.com").unwrap();
    let urls = protected_resource_metadata_urls(&server_url);

    assert_eq!(urls.len(), 1);
    assert_eq!(
        urls[0].as_str(),
        "https://mcp.example.com/.well-known/oauth-protected-resource"
    );
}

#[test]
fn test_auth_server_metadata_urls_with_path() {
    let issuer = Url::parse("https://auth.example.com/tenant1").unwrap();
    let urls = auth_server_metadata_urls(&issuer);

    assert_eq!(urls.len(), 3);
    assert_eq!(
        urls[0].as_str(),
        "https://auth.example.com/.well-known/oauth-authorization-server/tenant1"
    );
    assert_eq!(
        urls[1].as_str(),
        "https://auth.example.com/.well-known/openid-configuration/tenant1"
    );
    assert_eq!(
        urls[2].as_str(),
        "https://auth.example.com/tenant1/.well-known/openid-configuration"
    );
}

#[test]
fn test_auth_server_metadata_urls_without_path() {
    let issuer = Url::parse("https://auth.example.com").unwrap();
    let urls = auth_server_metadata_urls(&issuer);

    assert_eq!(urls.len(), 2);
    assert_eq!(
        urls[0].as_str(),
        "https://auth.example.com/.well-known/oauth-authorization-server"
    );
    assert_eq!(
        urls[1].as_str(),
        "https://auth.example.com/.well-known/openid-configuration"
    );
}
