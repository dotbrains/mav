use super::MarkdownEvent::*;
use super::MarkdownTag::*;
use super::*;

#[test]
fn test_html_comments() {
    assert_eq!(
        parse_markdown_with_options(
            "  <!--\nrdoc-file=string.c\n-->\nReturns",
            false,
            false,
            false
        ),
        ParsedMarkdownData {
            events: vec![
                (2..30, RootStart),
                (2..30, Start(HtmlBlock)),
                (2..2, SubstitutedText("  ".into())),
                (2..7, Html),
                (7..26, Html),
                (26..30, Html),
                (2..30, End(MarkdownTagEnd::HtmlBlock)),
                (2..30, RootEnd(0)),
                (30..37, RootStart),
                (30..37, Start(Paragraph)),
                (30..37, Text),
                (30..37, End(MarkdownTagEnd::Paragraph)),
                (30..37, RootEnd(1)),
            ],
            root_block_starts: vec![2, 30],
            ..Default::default()
        }
    )
}

#[test]
fn test_plain_urls_and_escaped_text() {
    assert_eq!(
        parse_markdown_with_options(
            "&nbsp;&nbsp; https://some.url some \\`&#9658;\\` text",
            false,
            false,
            false,
        ),
        ParsedMarkdownData {
            events: vec![
                (0..51, RootStart),
                (0..51, Start(Paragraph)),
                (0..6, SubstitutedText("\u{a0}".into())),
                (6..12, SubstitutedText("\u{a0}".into())),
                (12..13, Text),
                (
                    13..29,
                    Start(Link {
                        link_type: LinkType::Autolink,
                        dest_url: "https://some.url".into(),
                        title: "".into(),
                        id: "".into(),
                    })
                ),
                (13..29, Text),
                (13..29, End(MarkdownTagEnd::Link)),
                (29..35, Text),
                (36..37, Text), // Escaped backtick
                (37..44, SubstitutedText("►".into())),
                (45..46, Text), // Escaped backtick
                (46..51, Text),
                (0..51, End(MarkdownTagEnd::Paragraph)),
                (0..51, RootEnd(0)),
            ],
            root_block_starts: vec![0],
            ..Default::default()
        }
    );
}

#[test]
fn test_incomplete_link() {
    assert_eq!(
        parse_markdown_with_options(
            "You can use the [GitHub Search API](https://docs.github.com/en",
            false,
            false,
            false,
        )
        .events,
        vec![
            (0..62, RootStart),
            (0..62, Start(Paragraph)),
            (0..16, Text),
            (16..17, Text),
            (17..34, Text),
            (34..35, Text),
            (35..36, Text),
            (
                36..62,
                Start(Link {
                    link_type: LinkType::Autolink,
                    dest_url: "https://docs.github.com/en".into(),
                    title: "".into(),
                    id: "".into()
                })
            ),
            (36..62, Text),
            (36..62, End(MarkdownTagEnd::Link)),
            (0..62, End(MarkdownTagEnd::Paragraph)),
            (0..62, RootEnd(0)),
        ],
    );
}

#[test]
fn test_smart_punctuation() {
    assert_eq!(
        parse_markdown_with_options(
            "-- --- ... \"double quoted\" 'single quoted' ----------",
            false,
            false,
            false,
        ),
        ParsedMarkdownData {
            events: vec![
                (0..53, RootStart),
                (0..53, Start(Paragraph)),
                (0..2, SubstitutedText("–".into())),
                (2..3, Text),
                (3..6, SubstitutedText("—".into())),
                (6..7, Text),
                (7..10, SubstitutedText("…".into())),
                (10..11, Text),
                (11..12, SubstitutedText("\u{201c}".into())),
                (12..25, Text),
                (25..26, SubstitutedText("\u{201d}".into())),
                (26..27, Text),
                (27..28, SubstitutedText("\u{2018}".into())),
                (28..41, Text),
                (41..42, SubstitutedText("\u{2019}".into())),
                (42..43, Text),
                (43..53, SubstitutedText("–––––".into())),
                (0..53, End(MarkdownTagEnd::Paragraph)),
                (0..53, RootEnd(0)),
            ],
            root_block_starts: vec![0],
            ..Default::default()
        }
    )
}
