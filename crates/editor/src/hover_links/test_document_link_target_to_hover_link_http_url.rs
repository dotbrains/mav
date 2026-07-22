use super::*;

#[test]
fn test_document_link_target_to_hover_link_http_url() {
    let server_id = LanguageServerId(0);
    let target = "https://opensource.org/licenses/MIT";
    match document_link_target_to_hover_link(target, server_id) {
        HoverLink::Url(url) => assert_eq!(url, target),
        other => panic!("expected Url variant, got {other:?}"),
    }
}
