use super::*;

#[gpui::test]
async fn test_edit_prediction_text(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Simple insertion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 6)..Point::new(0, 6), " beautiful".into())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Hello, beautiful world!");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 6..16);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Replacement
    assert_highlighted_edits(
        "This is a test.",
        vec![(Point::new(0, 0)..Point::new(0, 4), "That".into())],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "That is a test.");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 0..4);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Multiple edits
    assert_highlighted_edits(
        "Hello, world!",
        vec![
            (Point::new(0, 0)..Point::new(0, 5), "Greetings".into()),
            (Point::new(0, 12)..Point::new(0, 12), " and universe".into()),
        ],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Greetings, world and universe!");
            assert_eq!(highlighted_edits.highlights.len(), 2);
            assert_eq!(highlighted_edits.highlights[0].0, 0..9);
            assert_eq!(highlighted_edits.highlights[1].0, 16..29);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
            assert_eq!(
                highlighted_edits.highlights[1].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;

    // Multiple lines with edits
    assert_highlighted_edits(
        "First line\nSecond line\nThird line\nFourth line",
        vec![
            (Point::new(1, 7)..Point::new(1, 11), "modified".to_string()),
            (
                Point::new(2, 0)..Point::new(2, 10),
                "New third line".to_string(),
            ),
            (Point::new(3, 6)..Point::new(3, 6), " updated".to_string()),
        ],
        false,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(
                highlighted_edits.text,
                "Second modified\nNew third line\nFourth updated line"
            );
            assert_eq!(highlighted_edits.highlights.len(), 3);
            assert_eq!(highlighted_edits.highlights[0].0, 7..15); // "modified"
            assert_eq!(highlighted_edits.highlights[1].0, 16..30); // "New third line"
            assert_eq!(highlighted_edits.highlights[2].0, 37..45); // " updated"
            for highlight in &highlighted_edits.highlights {
                assert_eq!(
                    highlight.1.background_color,
                    Some(cx.theme().status().created_background)
                );
            }
        },
    )
    .await;
}

#[gpui::test]
async fn test_edit_prediction_text_with_deletions(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    // Deletion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 5)..Point::new(0, 11), "".to_string())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.text, "Hello, world!");
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 5..11);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().deleted_background)
            );
        },
    )
    .await;

    // Insertion
    assert_highlighted_edits(
        "Hello, world!",
        vec![(Point::new(0, 6)..Point::new(0, 6), " digital".to_string())],
        true,
        cx,
        &|highlighted_edits, cx| {
            assert_eq!(highlighted_edits.highlights.len(), 1);
            assert_eq!(highlighted_edits.highlights[0].0, 6..14);
            assert_eq!(
                highlighted_edits.highlights[0].1.background_color,
                Some(cx.theme().status().created_background)
            );
        },
    )
    .await;
}

async fn assert_highlighted_edits(
    text: &str,
    edits: Vec<(Range<Point>, String)>,
    include_deletions: bool,
    cx: &mut TestAppContext,
    assertion_fn: &dyn Fn(HighlightedText, &App),
) {
    let window = cx.add_window(|window, cx| {
        let buffer = MultiBuffer::build_simple(text, cx);
        Editor::new(EditorMode::full(), buffer, None, window, cx)
    });
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let (buffer, snapshot) = window
        .update(cx, |editor, _window, cx| {
            (
                editor.buffer().clone(),
                editor.buffer().read(cx).snapshot(cx),
            )
        })
        .unwrap();

    let edits = edits
        .into_iter()
        .map(|(range, edit)| {
            (
                snapshot.anchor_after(range.start)..snapshot.anchor_before(range.end),
                edit,
            )
        })
        .collect::<Vec<_>>();

    let text_anchor_edits = edits
        .clone()
        .into_iter()
        .map(|(range, edit)| {
            (
                range.start.expect_text_anchor()..range.end.expect_text_anchor(),
                edit.into(),
            )
        })
        .collect::<Vec<_>>();

    let edit_preview = window
        .update(cx, |_, _window, cx| {
            buffer
                .read(cx)
                .as_singleton()
                .unwrap()
                .read(cx)
                .preview_edits(text_anchor_edits.into(), cx)
        })
        .unwrap()
        .await;

    cx.update(|_window, cx| {
        let highlighted_edits = edit_prediction_edit_text(
            snapshot.as_singleton().unwrap(),
            &edits,
            &edit_preview,
            include_deletions,
            &snapshot,
            cx,
        );
        assertion_fn(highlighted_edits, cx)
    });
}
