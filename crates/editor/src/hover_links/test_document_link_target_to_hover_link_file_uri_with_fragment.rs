use super::*;

#[test]
fn test_document_link_target_to_hover_link_file_uri_with_fragment() {
    let server_id = LanguageServerId(0);
    let target = "file:///Users/me/work/local_test/document-links-test.json#9,16";
    match document_link_target_to_hover_link(target, server_id) {
        HoverLink::LspLocation(location, returned_id) => {
            assert_eq!(returned_id, server_id);
            assert_eq!(
                location.uri.as_str(),
                "file:///Users/me/work/local_test/document-links-test.json#9,16",
            );
            assert_eq!(
                location.range,
                lsp::Range {
                    start: lsp::Position {
                        line: 8,
                        character: 15,
                    },
                    end: lsp::Position {
                        line: 8,
                        character: 15,
                    },
                }
            );
        }
        other => panic!("expected LspLocation variant, got {other:?}"),
    }
}
