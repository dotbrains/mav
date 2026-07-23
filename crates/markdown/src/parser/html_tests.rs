use super::MarkdownEvent::*;
use super::MarkdownTag::*;
use super::*;

#[test]
fn test_gfm_alert_block_quote_kinds() {
    use pulldown_cmark::BlockQuoteKind;

    let markdown = "\n> [!NOTE]\n> A note.\n\n> [!TIP]\n> A tip.\n\n> [!IMPORTANT]\n> Important.\n\n> [!WARNING]\n> A warning.\n\n> [!CAUTION]\n> A caution.\n\n> Plain quote.\n";
    let parsed = parse_markdown_with_options(markdown, false, false, false);

    let block_quote_kinds: Vec<_> = parsed
        .events
        .iter()
        .filter_map(|(_, event)| match event {
            Start(BlockQuote(kind)) => Some(*kind),
            _ => None,
        })
        .collect();

    assert_eq!(
        block_quote_kinds,
        vec![
            Some(BlockQuoteKind::Note),
            Some(BlockQuoteKind::Tip),
            Some(BlockQuoteKind::Important),
            Some(BlockQuoteKind::Warning),
            Some(BlockQuoteKind::Caution),
            None,
        ]
    );
}

#[test]
fn test_br_tag_emits_hard_break() {
    for input in [
        "hello<br>world",
        "hello<br/>world",
        "hello<br />world",
        "hello<br >world",
        "hello<BR>world",
        "hello<br class=\"x\">world",
        "hello<br class=\"x\"/>world",
    ] {
        let parsed = parse_markdown_with_options(input, true, false, false);
        let has_hard_break = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::HardBreak));
        let has_empty_substituted_text = parsed.events.iter().any(
            |(_, event)| matches!(event, MarkdownEvent::SubstitutedText(text) if text.is_empty()),
        );
        assert!(has_hard_break, "<br> in \"{input}\" should emit HardBreak");
        assert!(
            !has_empty_substituted_text,
            "<br> in \"{input}\" should not produce empty SubstitutedText"
        );
    }
}

#[test]
fn test_br_tag_not_a_hard_break_without_parse_html() {
    for input in ["hello<br>world", "hello<br/>world", "hello<br />world"] {
        let parsed = parse_markdown_with_options(input, false, false, false);
        let has_hard_break = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::HardBreak));
        let has_inline_html = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::InlineHtml));
        assert!(
            !has_hard_break,
            "<br> in \"{input}\" should not emit HardBreak when parse_html is disabled"
        );
        assert!(
            has_inline_html,
            "<br> in \"{input}\" should be preserved as InlineHtml when parse_html is disabled"
        );
    }
}

#[test]
fn test_br_prefixed_tag_is_not_a_hard_break() {
    for input in ["a<break>b", "a<brick>b", "a<b>bold</b>c"] {
        let parsed = parse_markdown_with_options(input, true, false, false);
        let has_hard_break = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::HardBreak));
        assert!(
            !has_hard_break,
            "\"{input}\" should not be treated as a <br> hard break"
        );
    }
}

#[test]
fn test_unrecognized_inline_html_preserved_as_inline_html() {
    for input in ["a<span>b</span>c", "a<em>b</em>c", "a<strong>b</strong>c"] {
        let parsed = parse_markdown_with_options(input, false, false, false);
        let has_inline_html = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::InlineHtml));
        let has_hard_break = parsed
            .events
            .iter()
            .any(|(_, event)| matches!(event, MarkdownEvent::HardBreak));
        assert!(
            has_inline_html,
            "unrecognized inline HTML \"{input}\" should emit InlineHtml"
        );
        assert!(
            !has_hard_break,
            "unrecognized inline HTML \"{input}\" should not emit HardBreak"
        );
    }
}
