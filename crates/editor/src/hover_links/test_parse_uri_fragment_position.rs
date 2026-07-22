use super::*;

#[test]
fn test_parse_uri_fragment_position() {
    // json-language-server style: 1-based `line,column`.
    assert_eq!(
        parse_uri_fragment_position("9,16"),
        Some(lsp::Position {
            line: 8,
            character: 15,
        })
    );
    assert_eq!(
        parse_uri_fragment_position("33,33"),
        Some(lsp::Position {
            line: 32,
            character: 32,
        })
    );

    // GitHub-style `L<line>` and `L<line>:<col>`.
    assert_eq!(
        parse_uri_fragment_position("L42"),
        Some(lsp::Position {
            line: 41,
            character: 0,
        })
    );
    assert_eq!(
        parse_uri_fragment_position("L42:7"),
        Some(lsp::Position {
            line: 41,
            character: 6,
        })
    );

    // Bare line number, no column.
    assert_eq!(
        parse_uri_fragment_position("5"),
        Some(lsp::Position {
            line: 4,
            character: 0,
        })
    );

    // Garbage / unparseable / 0-based fragments are rejected.
    assert_eq!(parse_uri_fragment_position(""), None);
    assert_eq!(parse_uri_fragment_position("section-name"), None);
    assert_eq!(parse_uri_fragment_position("0,0"), None);
}
