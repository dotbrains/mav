use super::*;

#[test]
fn test_link_pattern_file_candidates() {
    // Full markdown link: [LinkTitle](link_file.txt)
    // Trimmed strips [ and ), regex extracts link destination, raw is fallback
    let candidates: Vec<String> = link_pattern_file_candidates("[LinkTitle](link_file.txt)")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(
        candidates,
        vec![
            "LinkTitle](link_file.txt",
            "link_file.txt",
            "[LinkTitle](link_file.txt)"
        ]
    );

    // Link title with spaces (token starts mid-link)
    let candidates: Vec<String> = link_pattern_file_candidates("LinkTitle](link_file.txt)")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(
        candidates,
        vec![
            "LinkTitle](link_file.txt",
            "link_file.txt",
            "LinkTitle](link_file.txt)"
        ]
    );

    // Link with escaped spaces
    let candidates: Vec<String> = link_pattern_file_candidates("LinkTitle](link\\ _file.txt)")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(
        candidates,
        vec![
            "LinkTitle](link\\ _file.txt",
            "link\\ _file.txt",
            "LinkTitle](link\\ _file.txt)"
        ]
    );

    // Bare parentheses: (link_file.txt)
    let candidates: Vec<String> = link_pattern_file_candidates("(link_file.txt)")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(candidates, vec!["link_file.txt", "(link_file.txt)"]);

    // Trailing paren only: link_file.txt)
    let candidates: Vec<String> = link_pattern_file_candidates("link_file.txt)")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(candidates, vec!["link_file.txt", "link_file.txt)"]);

    // Trailing backtick only: link_file.txt`
    let candidates: Vec<String> = link_pattern_file_candidates("link_file.txt`")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(candidates, vec!["link_file.txt", "link_file.txt`"]);

    // Wrapped in backticks: `link_file.txt`
    let candidates: Vec<String> = link_pattern_file_candidates("`link_file.txt`")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(candidates, vec!["link_file.txt", "`link_file.txt`"]);

    // Trailing period (sentence ending): link_file.txt.
    let candidates: Vec<String> = link_pattern_file_candidates("link_file.txt.")
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(candidates, vec!["link_file.txt", "link_file.txt."]);

    // Nested parens - regex finds first (...) capturing inner content
    let candidates: Vec<String> =
        link_pattern_file_candidates("LinkTitle](link_(link_file)file.txt)")
            .into_iter()
            .map(|(c, _)| c)
            .collect();
    assert_eq!(
        candidates,
        vec![
            "LinkTitle](link_(link_file)file.txt",
            "link_(link_file",
            "LinkTitle](link_(link_file)file.txt)"
        ]
    );
}
