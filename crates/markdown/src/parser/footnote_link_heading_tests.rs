use super::MarkdownEvent::*;
use super::MarkdownTag::*;
use super::*;

#[test]
fn test_footnotes() {
    let parsed = parse_markdown_with_options(
        "Text with a footnote[^1] and some more text.\n\n[^1]: This is the footnote content.",
        false,
        false,
        false,
    );
    assert_eq!(
        parsed.events,
        vec![
            (0..45, RootStart),
            (0..45, Start(Paragraph)),
            (0..20, Text),
            (20..24, FootnoteReference("1".into())),
            (24..44, Text),
            (0..45, End(MarkdownTagEnd::Paragraph)),
            (0..45, RootEnd(0)),
            (46..81, RootStart),
            (46..81, Start(FootnoteDefinition("1".into()))),
            (52..81, Start(Paragraph)),
            (52..81, Text),
            (52..81, End(MarkdownTagEnd::Paragraph)),
            (46..81, End(MarkdownTagEnd::FootnoteDefinition)),
            (46..81, RootEnd(1)),
        ]
    );
    assert_eq!(parsed.footnote_definitions.len(), 1);
    assert_eq!(parsed.footnote_definitions.get("1").copied(), Some(52));
}

#[test]
fn test_footnote_definitions_multiple() {
    let parsed = parse_markdown_with_options(
        "Text[^a] and[^b].\n\n[^a]: First.\n\n[^b]: Second.",
        false,
        false,
        false,
    );
    assert_eq!(parsed.footnote_definitions.len(), 2);
    assert!(parsed.footnote_definitions.contains_key("a"));
    assert!(parsed.footnote_definitions.contains_key("b"));
}

#[test]
fn test_links_split_across_fragments() {
    // This test verifies that links split across multiple text fragments due to escaping or other issues
    // are correctly detected and processed
    // Note: In real usage, pulldown_cmark creates separate text events for the escaped character
    // We're verifying our parser can handle this correctly
    assert_eq!(
        parse_markdown_with_options(
            "https:/\\/example.com is equivalent to https://example&#46;com!",
            false,
            false,
            false,
        )
        .events,
        vec![
            (0..62, RootStart),
            (0..62, Start(Paragraph)),
            (
                0..20,
                Start(Link {
                    link_type: LinkType::Autolink,
                    dest_url: "https://example.com".into(),
                    title: "".into(),
                    id: "".into()
                })
            ),
            (0..7, Text),
            (8..20, Text),
            (0..20, End(MarkdownTagEnd::Link)),
            (20..38, Text),
            (
                38..61,
                Start(Link {
                    link_type: LinkType::Autolink,
                    dest_url: "https://example.com".into(),
                    title: "".into(),
                    id: "".into()
                })
            ),
            (38..53, Text),
            (53..58, SubstitutedText(".".into())),
            (58..61, Text),
            (38..61, End(MarkdownTagEnd::Link)),
            (61..62, Text),
            (0..62, End(MarkdownTagEnd::Paragraph)),
            (0..62, RootEnd(0)),
        ],
    );

    assert_eq!(
        parse_markdown_with_options(
            "Visit https://example.com/cat\\/é&#8205;☕ for coffee!",
            false,
            false,
            false,
        )
        .events,
        [
            (0..55, RootStart),
            (0..55, Start(Paragraph)),
            (0..6, Text),
            (
                6..43,
                Start(Link {
                    link_type: LinkType::Autolink,
                    dest_url: "https://example.com/cat/é\u{200d}☕".into(),
                    title: "".into(),
                    id: "".into()
                })
            ),
            (6..29, Text),
            (30..33, Text),
            (33..40, SubstitutedText("\u{200d}".into())),
            (40..43, Text),
            (6..43, End(MarkdownTagEnd::Link)),
            (43..55, Text),
            (0..55, End(MarkdownTagEnd::Paragraph)),
            (0..55, RootEnd(0)),
        ]
    );
}

#[test]
fn test_heading_slugs() {
    let parsed = parse_markdown_with_options(
        "# Hello World\n\n## Code `block`\n\n### Third Level\n\n#### Fourth Level\n\n## Hello World",
        false,
        true,
        false,
    );
    assert_eq!(parsed.heading_slugs.len(), 5);
    assert!(parsed.heading_slugs.contains_key("hello-world"));
    assert!(parsed.heading_slugs.contains_key("code-block"));
    assert!(parsed.heading_slugs.contains_key("third-level"));
    assert!(parsed.heading_slugs.contains_key("fourth-level"));
    assert!(parsed.heading_slugs.contains_key("hello-world-1"));
}

#[test]
fn test_heading_source_index_for_slug() {
    let parsed = parse_markdown_with_options(
        "# Duplicate\n\nText\n\n## Duplicate\n\nMore text",
        false,
        true,
        false,
    );
    let first = parsed.heading_slugs.get("duplicate").copied();
    let second = parsed.heading_slugs.get("duplicate-1").copied();
    assert!(first.is_some());
    assert!(second.is_some());
    assert!(first.expect("first slug missing") < second.expect("second slug missing"));
}

#[test]
fn test_heading_slug_collision_with_dedup_suffix() {
    let parsed = parse_markdown_with_options("# Foo\n\n## Foo\n\n## Foo 1", false, true, false);
    assert_eq!(parsed.heading_slugs.len(), 3);
    assert!(parsed.heading_slugs.contains_key("foo"));
    assert!(parsed.heading_slugs.contains_key("foo-1"));
    assert!(parsed.heading_slugs.contains_key("foo-1-1"));
}
