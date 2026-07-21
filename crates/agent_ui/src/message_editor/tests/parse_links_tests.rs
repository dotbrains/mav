use super::*;

#[test]
fn test_parse_mention_links() {
    // Single file mention
    let text = "[@bundle-mac](file:///Users/test/mav/script/bundle-mac)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].0, 0..text.len());
    assert!(matches!(mentions[0].1, MentionUri::File { .. }));

    // Multiple mentions
    let text = "Check [@file1](file:///path/to/file1) and [@file2](file:///path/to/file2)!";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 2);

    // Text without mentions
    let text = "Just some regular text without mentions";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 0);

    // Malformed mentions (should be skipped)
    let text = "[@incomplete](invalid://uri) and [@missing](";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 0);

    // Mixed content with valid mention
    let text = "Before [@valid](file:///path/to/file) after";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].0.start, 7);

    // HTTP URL mention (Fetch)
    let text = "Check out [@docs](https://example.com/docs) for more info";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    assert!(matches!(mentions[0].1, MentionUri::Fetch { .. }));

    // Directory mention (trailing slash)
    let text = "[@src](file:///path/to/src/)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    assert!(matches!(mentions[0].1, MentionUri::Directory { .. }));

    // Multiple different mention types
    let text = "File [@f](file:///a) and URL [@u](https://b.com) and dir [@d](file:///c/)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 3);
    assert!(matches!(mentions[0].1, MentionUri::File { .. }));
    assert!(matches!(mentions[1].1, MentionUri::Fetch { .. }));
    assert!(matches!(mentions[2].1, MentionUri::Directory { .. }));

    // Adjacent mentions without separator
    let text = "[@a](file:///a)[@b](file:///b)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 2);

    // Regular markdown link (not a mention) should be ignored
    let text = "[regular link](https://example.com)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 0);

    // Incomplete mention link patterns
    let text = "[@name] without url and [@name( malformed";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 0);

    // Nested brackets in name portion
    let text = "[@name [with brackets]](file:///path/to/file)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].0, 0..text.len());

    // Deeply nested brackets
    let text = "[@outer [inner [deep]]](file:///path)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);

    // Unbalanced brackets should fail gracefully
    let text = "[@unbalanced [bracket](file:///path)";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 0);

    // Nested parentheses in URI (common in URLs with query params)
    let text = "[@wiki](https://en.wikipedia.org/wiki/Rust_(programming_language))";
    let mentions = parse_mention_links(text, PathStyle::local());
    assert_eq!(mentions.len(), 1);
    if let MentionUri::Fetch { url } = &mentions[0].1 {
        assert!(url.as_str().contains("Rust_(programming_language)"));
    } else {
        panic!("Expected Fetch URI");
    }
}
