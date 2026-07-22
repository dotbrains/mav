use super::*;

#[test]
fn test_oauth_callback_parse_query() {
    let callback = OAuthCallback::parse_query("code=test_auth_code&state=test_state").unwrap();
    assert_eq!(callback.code, "test_auth_code");
    assert_eq!(callback.state, "test_state");
}
#[test]
fn test_oauth_callback_parse_query_reversed_order() {
    let callback = OAuthCallback::parse_query("state=test_state&code=test_auth_code").unwrap();
    assert_eq!(callback.code, "test_auth_code");
    assert_eq!(callback.state, "test_state");
}

#[test]
fn test_oauth_callback_parse_query_with_extra_params() {
    let callback =
        OAuthCallback::parse_query("code=test_auth_code&state=test_state&extra=ignored").unwrap();
    assert_eq!(callback.code, "test_auth_code");
    assert_eq!(callback.state, "test_state");
}

#[test]
fn test_oauth_callback_parse_query_missing_code() {
    let result = OAuthCallback::parse_query("state=test_state");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("code"));
}

#[test]
fn test_oauth_callback_parse_query_missing_state() {
    let result = OAuthCallback::parse_query("code=test_auth_code");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("state"));
}

#[test]
fn test_oauth_callback_parse_query_empty_code() {
    let result = OAuthCallback::parse_query("code=&state=test_state");
    assert!(result.is_err());
}

#[test]
fn test_oauth_callback_parse_query_empty_state() {
    let result = OAuthCallback::parse_query("code=test_auth_code&state=");
    assert!(result.is_err());
}

#[test]
fn test_oauth_callback_parse_query_url_encoded_values() {
    let callback = OAuthCallback::parse_query("code=abc%20def&state=test%3Dstate").unwrap();
    assert_eq!(callback.code, "abc def");
    assert_eq!(callback.state, "test=state");
}

#[test]
fn test_oauth_callback_parse_query_error_response() {
    let result = OAuthCallback::parse_query(
        "error=access_denied&error_description=User%20denied%20access&state=abc",
    );
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("access_denied"),
        "unexpected error: {}",
        err_msg
    );
    assert!(
        err_msg.contains("User denied access"),
        "unexpected error: {}",
        err_msg
    );
}

#[test]
fn test_oauth_callback_parse_query_error_without_description() {
    let result = OAuthCallback::parse_query("error=server_error&state=abc");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("server_error"),
        "unexpected error: {}",
        err_msg
    );
    assert!(
        err_msg.contains("no description"),
        "unexpected error: {}",
        err_msg
    );
}
