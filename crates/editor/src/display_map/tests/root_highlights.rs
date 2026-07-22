use super::super::*;

fn test_tabs_with_multibyte_chars(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    let text = "✅\t\tα\nβ\t\n🏀β\t\tγ";
    let buffer = MultiBuffer::build_simple(text, cx);
    let font_size = px(14.0);

    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });
    let map = map.update(cx, |map, cx| map.snapshot(cx));
    assert_eq!(map.text(), "✅       α\nβ   \n🏀β      γ");
    assert_eq!(
        map.text_chunks(DisplayRow(0)).collect::<String>(),
        "✅       α\nβ   \n🏀β      γ"
    );
    assert_eq!(
        map.text_chunks(DisplayRow(1)).collect::<String>(),
        "β   \n🏀β      γ"
    );
    assert_eq!(
        map.text_chunks(DisplayRow(2)).collect::<String>(),
        "🏀β      γ"
    );

    let point = MultiBufferPoint::new(0, "✅\t\t".len() as u32);
    let display_point = DisplayPoint::new(DisplayRow(0), "✅       ".len() as u32);
    assert_eq!(point.to_display_point(&map), display_point);
    assert_eq!(display_point.to_point(&map), point);

    let point = MultiBufferPoint::new(1, "β\t".len() as u32);
    let display_point = DisplayPoint::new(DisplayRow(1), "β   ".len() as u32);
    assert_eq!(point.to_display_point(&map), display_point);
    assert_eq!(display_point.to_point(&map), point,);

    let point = MultiBufferPoint::new(2, "🏀β\t\t".len() as u32);
    let display_point = DisplayPoint::new(DisplayRow(2), "🏀β      ".len() as u32);
    assert_eq!(point.to_display_point(&map), display_point);
    assert_eq!(display_point.to_point(&map), point,);

    // Display points inside of expanded tabs
    assert_eq!(
        DisplayPoint::new(DisplayRow(0), "✅      ".len() as u32).to_point(&map),
        MultiBufferPoint::new(0, "✅\t".len() as u32),
    );
    assert_eq!(
        DisplayPoint::new(DisplayRow(0), "✅ ".len() as u32).to_point(&map),
        MultiBufferPoint::new(0, "✅".len() as u32),
    );

    // Clipping display points inside of multi-byte characters
    assert_eq!(
        map.clip_point(
            DisplayPoint::new(DisplayRow(0), "✅".len() as u32 - 1),
            Left
        ),
        DisplayPoint::new(DisplayRow(0), 0)
    );
    assert_eq!(
        map.clip_point(
            DisplayPoint::new(DisplayRow(0), "✅".len() as u32 - 1),
            Bias::Right
        ),
        DisplayPoint::new(DisplayRow(0), "✅".len() as u32)
    );
}

#[gpui::test]
fn test_max_point(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    let buffer = MultiBuffer::build_simple("aaa\n\t\tbbb", cx);
    let font_size = px(14.0);
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });
    assert_eq!(
        map.update(cx, |map, cx| map.snapshot(cx)).max_point(),
        DisplayPoint::new(DisplayRow(1), 11)
    )
}

fn syntax_chunks(
    rows: Range<DisplayRow>,
    map: &Entity<DisplayMap>,
    theme: &SyntaxTheme,
    cx: &mut App,
) -> Vec<(String, Option<Hsla>)> {
    chunks(rows, map, theme, cx)
        .into_iter()
        .map(|(text, color, _)| (text, color))
        .collect()
}

fn chunks(
    rows: Range<DisplayRow>,
    map: &Entity<DisplayMap>,
    theme: &SyntaxTheme,
    cx: &mut App,
) -> Vec<(String, Option<Hsla>, Option<Hsla>)> {
    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    let mut chunks: Vec<(String, Option<Hsla>, Option<Hsla>)> = Vec::new();
    for chunk in snapshot.chunks(
        rows,
        LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: true,
        },
        HighlightStyles::default(),
    ) {
        let syntax_color = chunk
            .syntax_highlight_id
            .and_then(|id| theme.get(id)?.color);

        let highlight_color = chunk.highlight_style.and_then(|style| style.color);
        if let Some((last_chunk, last_syntax_color, last_highlight_color)) = chunks.last_mut()
            && syntax_color == *last_syntax_color
            && highlight_color == *last_highlight_color
        {
            last_chunk.push_str(chunk.text);
            continue;
        }
        chunks.push((chunk.text.to_string(), syntax_color, highlight_color));
    }
    chunks
}

fn init_test(cx: &mut App, f: &dyn Fn(&mut SettingsContent)) {
    let settings = SettingsStore::test(cx);
    cx.set_global(settings);
    crate::init(cx);
    theme_settings::init(LoadThemes::JustBase, cx);
    cx.update_global::<SettingsStore, _>(|store, cx| {
        store.update_user_settings(cx, f);
    });
}

#[gpui::test]
fn test_isomorphic_display_point_ranges_for_buffer_range(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.new(|cx| Buffer::local("let x = 5;\n", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));

    let font_size = px(14.0);
    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer.clone(),
            font("Helvetica"),
            font_size,
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    // Without inlays, a buffer range maps to a single display range.
    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    let ranges = snapshot.isomorphic_display_point_ranges_for_buffer_range(
        MultiBufferOffset(4)..MultiBufferOffset(9),
    );
    assert_eq!(ranges.len(), 1);
    // "x = 5" is columns 4..9 with no inlays shifting anything.
    assert_eq!(ranges[0].start, DisplayPoint::new(DisplayRow(0), 4));
    assert_eq!(ranges[0].end, DisplayPoint::new(DisplayRow(0), 9));

    // Insert a 4-char inlay hint ": i32" at buffer offset 5 (after "x").
    map.update(cx, |map, cx| {
        map.splice_inlays(
            &[],
            vec![Inlay::mock_hint(
                0,
                buffer_snapshot.anchor_after(MultiBufferOffset(5)),
                ": i32",
            )],
            cx,
        );
    });
    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    assert_eq!(snapshot.text(), "let x: i32 = 5;\n");

    // A buffer range [4..9] ("x = 5") now spans across the inlay.
    // It should be split into two display ranges that skip the inlay text.
    let ranges = snapshot.isomorphic_display_point_ranges_for_buffer_range(
        MultiBufferOffset(4)..MultiBufferOffset(9),
    );
    assert_eq!(
        ranges.len(),
        2,
        "expected the range to be split around the inlay, got: {:?}",
        ranges,
    );
    // First sub-range: buffer [4, 5) → "x" at display columns 4..5
    assert_eq!(ranges[0].start, DisplayPoint::new(DisplayRow(0), 4));
    assert_eq!(ranges[0].end, DisplayPoint::new(DisplayRow(0), 5));
    // Second sub-range: buffer [5, 9) → " = 5" at display columns 10..14
    // (shifted right by the 5-char ": i32" inlay)
    assert_eq!(ranges[1].start, DisplayPoint::new(DisplayRow(0), 10));
    assert_eq!(ranges[1].end, DisplayPoint::new(DisplayRow(0), 14));

    // A range entirely before the inlay is not split.
    let ranges = snapshot.isomorphic_display_point_ranges_for_buffer_range(
        MultiBufferOffset(0)..MultiBufferOffset(5),
    );
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start, DisplayPoint::new(DisplayRow(0), 0));
    assert_eq!(ranges[0].end, DisplayPoint::new(DisplayRow(0), 5));

    // A range entirely after the inlay is not split.
    let ranges = snapshot.isomorphic_display_point_ranges_for_buffer_range(
        MultiBufferOffset(5)..MultiBufferOffset(9),
    );
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start, DisplayPoint::new(DisplayRow(0), 10));
    assert_eq!(ranges[0].end, DisplayPoint::new(DisplayRow(0), 14));
}

#[test]
fn test_highlight_invisibles_preserves_compound_emojis() {
    let editor_style = EditorStyle::default();

    let pilot_emoji = "🧑\u{200d}✈\u{fe0f}";
    let chunk = HighlightedChunk {
        text: pilot_emoji,
        style: None,
        is_tab: false,
        is_inlay: false,
        replacement: None,
    };

    let chunks: Vec<_> = chunk
        .highlight_invisibles(&editor_style)
        .map(|chunk| chunk.text.to_string())
        .collect();

    assert_eq!(
        chunks.concat(),
        pilot_emoji,
        "all text bytes must be preserved"
    );
    assert_eq!(
        chunks.len(),
        1,
        "compound emoji should not be split into multiple chunks, got: {:?}",
        chunks,
    );
}

/// Regression test: Creating a DisplayMap when the MultiBuffer has pending
/// unsynced changes should not cause a desync between the subscription edits
/// and the InlayMap's buffer state.
///
/// The bug occurred because:
/// 1. DisplayMap::new created a subscription first
/// 2. Then called snapshot() which synced and published edits
/// 3. InlayMap was created with the post-sync snapshot
/// 4. But the subscription captured the sync edits, leading to double-application
#[gpui::test]
fn test_display_map_subscription_ordering(cx: &mut gpui::App) {
    init_test(cx, &|_| {});

    // Create a buffer with some initial text
    let buffer = cx.new(|cx| Buffer::local("initial", cx));
    let multibuffer = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));

    // Edit the buffer. This sets buffer_changed_since_sync = true.
    // Importantly, do NOT call multibuffer.snapshot() yet.
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "prefix ")], None, cx);
    });

    // Create the DisplayMap. In the buggy code, this would:
    // 1. Create subscription (empty)
    // 2. Call snapshot() which syncs and publishes edits E1
    // 3. Create InlayMap with post-E1 snapshot
    // 4. Subscription now has E1, but InlayMap is already at post-E1 state
    let map = cx.new(|cx| {
        DisplayMap::new(
            multibuffer.clone(),
            font("Helvetica"),
            px(14.0),
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    // Verify initial state is correct
    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    assert_eq!(snapshot.text(), "prefix initial");

    // Make another edit
    buffer.update(cx, |buffer, cx| {
        buffer.edit([(7..7, "more ")], None, cx);
    });

    // This would crash in the buggy code because:
    // - InlayMap expects edits from V1 to V2
    // - But subscription has E1 ∘ E2 (from V0 to V2)
    // - The calculation `buffer_edit.new.end + (cursor.end().0 - buffer_edit.old.end)`
    //   would produce an offset exceeding the buffer length
    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    assert_eq!(snapshot.text(), "prefix more initial");
}
