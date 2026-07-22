use super::*;

#[gpui::test]
fn test_inlay_utf8_boundary_panic_fix(cx: &mut App) {
    init_test(cx);

    // This test verifies that we handle UTF-8 character boundaries correctly
    // when splitting inlay text for highlighting. Previously, this would panic
    // when trying to split at byte 13, which is in the middle of the '…' character.
    //
    // See https://github.com/mav-industries/mav/issues/33641
    let buffer = MultiBuffer::build_simple("fn main() {}\n", cx);
    let (mut inlay_map, _) = InlayMap::new(buffer.read(cx).snapshot(cx));

    // Create an inlay with text that contains a multi-byte character
    // The string "SortingDirec…" contains an ellipsis character '…' which is 3 bytes (E2 80 A6)
    let inlay_text = "SortingDirec…";
    let position = buffer.read(cx).snapshot(cx).anchor_before(Point::new(0, 5));

    let inlay = Inlay {
        id: InlayId::Hint(0),
        position,
        content: InlayContent::Text(text::Rope::from(inlay_text)),
    };

    let (inlay_snapshot, _) = inlay_map.splice(&[], vec![inlay]);

    // Create highlights that request a split at byte 13, which is in the middle
    // of the '…' character (bytes 12..15). We include the full character.
    let inlay_highlights = create_inlay_highlights(InlayId::Hint(0), 0..13, position);

    let highlights = crate::display_map::Highlights {
        text_highlights: None,
        inlay_highlights: Some(&inlay_highlights),
        semantic_token_highlights: None,
        styles: crate::display_map::HighlightStyles::default(),
    };

    // Collect chunks - this previously would panic
    let chunks: Vec<_> = inlay_snapshot
        .chunks(
            InlayOffset(MultiBufferOffset(0))..inlay_snapshot.len(),
            LanguageAwareStyling {
                tree_sitter: false,
                diagnostics: false,
            },
            highlights,
        )
        .collect();

    // Verify the chunks are correct
    let full_text: String = chunks.iter().map(|c| c.chunk.text).collect();
    assert_eq!(full_text, "fn maSortingDirec…in() {}\n");

    // Verify the highlighted portion includes the complete ellipsis character
    let highlighted_chunks: Vec<_> = chunks
        .iter()
        .filter(|c| c.chunk.highlight_style.is_some() && c.chunk.is_inlay)
        .collect();

    assert_eq!(highlighted_chunks.len(), 1);
    assert_eq!(highlighted_chunks[0].chunk.text, "SortingDirec…");
}

#[gpui::test]
fn test_inlay_utf8_boundaries(cx: &mut App) {
    init_test(cx);

    struct TestCase {
        inlay_text: &'static str,
        highlight_range: Range<usize>,
        expected_highlighted: &'static str,
        description: &'static str,
    }

    let test_cases = vec![
        TestCase {
            inlay_text: "Hello👋World",
            highlight_range: 0..7,
            expected_highlighted: "Hello👋",
            description: "Emoji boundary - rounds up to include full emoji",
        },
        TestCase {
            inlay_text: "Test→End",
            highlight_range: 0..5,
            expected_highlighted: "Test→",
            description: "Arrow boundary - rounds up to include full arrow",
        },
        TestCase {
            inlay_text: "café",
            highlight_range: 0..4,
            expected_highlighted: "café",
            description: "Accented char boundary - rounds up to include full é",
        },
        TestCase {
            inlay_text: "🎨🎭🎪",
            highlight_range: 0..5,
            expected_highlighted: "🎨🎭",
            description: "Multiple emojis - partial highlight",
        },
        TestCase {
            inlay_text: "普通话",
            highlight_range: 0..4,
            expected_highlighted: "普通",
            description: "Chinese characters - partial highlight",
        },
        TestCase {
            inlay_text: "Hello",
            highlight_range: 0..2,
            expected_highlighted: "He",
            description: "ASCII only - no adjustment needed",
        },
        TestCase {
            inlay_text: "👋",
            highlight_range: 0..1,
            expected_highlighted: "👋",
            description: "Single emoji - partial byte range includes whole char",
        },
        TestCase {
            inlay_text: "Test",
            highlight_range: 0..0,
            expected_highlighted: "",
            description: "Empty range",
        },
        TestCase {
            inlay_text: "🎨ABC",
            highlight_range: 2..5,
            expected_highlighted: "A",
            description: "Range starting mid-emoji skips the emoji",
        },
    ];

    for test_case in test_cases {
        let buffer = MultiBuffer::build_simple("test", cx);
        let (mut inlay_map, _) = InlayMap::new(buffer.read(cx).snapshot(cx));
        let position = buffer.read(cx).snapshot(cx).anchor_before(Point::new(0, 2));

        let inlay = Inlay {
            id: InlayId::Hint(0),
            position,
            content: InlayContent::Text(text::Rope::from(test_case.inlay_text)),
        };

        let (inlay_snapshot, _) = inlay_map.splice(&[], vec![inlay]);
        let inlay_highlights = create_inlay_highlights(
            InlayId::Hint(0),
            test_case.highlight_range.clone(),
            position,
        );

        let highlights = crate::display_map::Highlights {
            text_highlights: None,
            inlay_highlights: Some(&inlay_highlights),
            semantic_token_highlights: None,
            styles: crate::display_map::HighlightStyles::default(),
        };

        let chunks: Vec<_> = inlay_snapshot
            .chunks(
                InlayOffset(MultiBufferOffset(0))..inlay_snapshot.len(),
                LanguageAwareStyling {
                    tree_sitter: false,
                    diagnostics: false,
                },
                highlights,
            )
            .collect();

        // Verify we got chunks and they total to the expected text
        let full_text: String = chunks.iter().map(|c| c.chunk.text).collect();
        assert_eq!(
            full_text,
            format!("te{}st", test_case.inlay_text),
            "Full text mismatch for case: {}",
            test_case.description
        );

        // Verify that the highlighted portion matches expectations
        let highlighted_text: String = chunks
            .iter()
            .filter(|c| c.chunk.highlight_style.is_some() && c.chunk.is_inlay)
            .map(|c| c.chunk.text)
            .collect();
        assert_eq!(
            highlighted_text, test_case.expected_highlighted,
            "Highlighted text mismatch for case: {} (text: '{}', range: {:?})",
            test_case.description, test_case.inlay_text, test_case.highlight_range
        );
    }
}
