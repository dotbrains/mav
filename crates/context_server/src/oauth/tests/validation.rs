use super::*;

#[test]
fn test_require_https_or_loopback_accepts_https() {
    let url = Url::parse("https://auth.example.com/token").unwrap();
    assert!(require_https_or_loopback(&url).is_ok());
}
#[test]
fn test_require_https_or_loopback_rejects_http_remote() {
    let url = Url::parse("http://auth.example.com/token").unwrap();
    assert!(require_https_or_loopback(&url).is_err());
}

#[test]
fn test_require_https_or_loopback_accepts_http_127_0_0_1() {
    let url = Url::parse("http://127.0.0.1:8080/callback").unwrap();
    assert!(require_https_or_loopback(&url).is_ok());
}

#[test]
fn test_require_https_or_loopback_accepts_http_ipv6_loopback() {
    let url = Url::parse("http://[::1]:8080/callback").unwrap();
    assert!(require_https_or_loopback(&url).is_ok());
}

#[test]
fn test_require_https_or_loopback_accepts_http_localhost() {
    let url = Url::parse("http://localhost:8080/callback").unwrap();
    assert!(require_https_or_loopback(&url).is_ok());
}

#[test]
fn test_require_https_or_loopback_accepts_http_localhost_case_insensitive() {
    let url = Url::parse("http://LOCALHOST:8080/callback").unwrap();
    assert!(require_https_or_loopback(&url).is_ok());
}

#[test]
fn test_require_https_or_loopback_rejects_http_non_loopback_ip() {
    let url = Url::parse("http://192.168.1.1:8080/token").unwrap();
    assert!(require_https_or_loopback(&url).is_err());
}

#[test]
fn test_require_https_or_loopback_rejects_ftp() {
    let url = Url::parse("ftp://auth.example.com/token").unwrap();
    assert!(require_https_or_loopback(&url).is_err());
}

// -- validate_oauth_url (SSRF) tests ------------------------------------

#[test]
fn test_validate_oauth_url_accepts_https_public() {
    let url = Url::parse("https://auth.example.com/token").unwrap();
    assert!(validate_oauth_url(&url).is_ok());
}

#[test]
fn test_validate_oauth_url_rejects_private_ipv4_10() {
    let url = Url::parse("https://10.0.0.1/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_private_ipv4_172() {
    let url = Url::parse("https://172.16.0.1/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_private_ipv4_192() {
    let url = Url::parse("https://192.168.1.1/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_link_local() {
    let url = Url::parse("https://169.254.169.254/latest/meta-data/").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_ipv6_ula() {
    let url = Url::parse("https://[fd12:3456:789a::1]/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_ipv6_unspecified() {
    let url = Url::parse("https://[::]/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_ipv4_mapped_ipv6_private() {
    let url = Url::parse("https://[::ffff:10.0.0.1]/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_rejects_ipv4_mapped_ipv6_link_local() {
    let url = Url::parse("https://[::ffff:169.254.169.254]/token").unwrap();
    assert!(validate_oauth_url(&url).is_err());
}

#[test]
fn test_validate_oauth_url_allows_http_loopback() {
    // Loopback is permitted (it's our callback server).
    let url = Url::parse("http://127.0.0.1:8080/callback").unwrap();
    assert!(validate_oauth_url(&url).is_ok());
}

#[test]
fn test_validate_oauth_url_allows_https_public_ip() {
    let url = Url::parse("https://93.184.216.34/token").unwrap();
    assert!(validate_oauth_url(&url).is_ok());
}
