use super::*;

#[gpui::test]
async fn test_chunks(cx: &mut gpui::TestAppContext) {
    let text = r#"
        fn outer() {}

        mod module {
            fn inner() {}
        }"#
    .unindent();

    let theme = SyntaxTheme::new_test(vec![("mod.body", Hsla::red()), ("fn.name", Hsla::blue())]);
    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Test".into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec![".test".to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_highlights_query(
            r#"
            (mod_item name: (identifier) body: _ @mod.body)
            (function_item name: (identifier) @fn.name)
            "#,
        )
        .unwrap(),
    );
    language.set_theme(&theme);

    cx.update(|cx| {
        init_test(cx, &|s| {
            s.project.all_languages.defaults.tab_size = Some(2.try_into().unwrap())
        })
    });

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    cx.condition(&buffer, |buf, _| !buf.is_parsing()).await;
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));

    let font_size = px(14.0);

    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer,
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
        cx.update(|cx| syntax_chunks(DisplayRow(0)..DisplayRow(5), &map, &theme, cx)),
        vec![
            ("fn ".to_string(), None),
            ("outer".to_string(), Some(Hsla::blue())),
            ("() {}\n\nmod module ".to_string(), None),
            ("{\n    fn ".to_string(), Some(Hsla::red())),
            ("inner".to_string(), Some(Hsla::blue())),
            ("() {}\n}".to_string(), Some(Hsla::red())),
        ]
    );
    assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(3)..DisplayRow(5), &map, &theme, cx)),
        vec![
            ("    fn ".to_string(), Some(Hsla::red())),
            ("inner".to_string(), Some(Hsla::blue())),
            ("() {}\n}".to_string(), Some(Hsla::red())),
        ]
    );

    map.update(cx, |map, cx| {
        map.fold(
            vec![Crease::simple(
                MultiBufferPoint::new(0, 6)..MultiBufferPoint::new(3, 2),
                FoldPlaceholder::test(),
            )],
            cx,
        )
    });
    assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(0)..DisplayRow(2), &map, &theme, cx)),
        vec![
            ("fn ".to_string(), None),
            ("out".to_string(), Some(Hsla::blue())),
            ("⋯".to_string(), None),
            ("  fn ".to_string(), Some(Hsla::red())),
            ("inner".to_string(), Some(Hsla::blue())),
            ("() {}\n}".to_string(), Some(Hsla::red())),
        ]
    );
}

#[gpui::test]
async fn test_chunks_with_syntax_highlighting_across_blocks(cx: &mut gpui::TestAppContext) {
    cx.background_executor
        .set_block_on_ticks(usize::MAX..=usize::MAX);

    let text = r#"
        const A: &str = "
            one
            two
            three
        ";
        const B: &str = "four";
    "#
    .unindent();

    let theme = SyntaxTheme::new_test(vec![
        ("string", Hsla::red()),
        ("punctuation", Hsla::blue()),
        ("keyword", Hsla::green()),
    ]);
    let language = Arc::new(
        Language::new(
            LanguageConfig {
                name: "Rust".into(),
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )
        .with_highlights_query(
            r#"
            (string_literal) @string
            "const" @keyword
            [":" ";"] @punctuation
            "#,
        )
        .unwrap(),
    );
    language.set_theme(&theme);

    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.new(|cx| Buffer::local(text, cx).with_language(language, cx));
    cx.condition(&buffer, |buf, _| !buf.is_parsing()).await;
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));

    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer,
            font("Courier"),
            px(16.0),
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    // Insert two blocks in the middle of a multi-line string literal.
    // The second block has zero height.
    map.update(cx, |map, cx| {
        map.insert_blocks(
            [
                BlockProperties {
                    placement: BlockPlacement::Below(
                        buffer_snapshot.anchor_before(Point::new(1, 0)),
                    ),
                    height: Some(1),
                    style: BlockStyle::Sticky,
                    render: Arc::new(|_| div().into_any()),
                    priority: 0,
                },
                BlockProperties {
                    placement: BlockPlacement::Below(
                        buffer_snapshot.anchor_before(Point::new(2, 0)),
                    ),
                    height: None,
                    style: BlockStyle::Sticky,
                    render: Arc::new(|_| div().into_any()),
                    priority: 0,
                },
            ],
            cx,
        )
    });

    pretty_assertions::assert_eq!(
        cx.update(|cx| syntax_chunks(DisplayRow(0)..DisplayRow(7), &map, &theme, cx)),
        [
            ("const".into(), Some(Hsla::green())),
            (" A".into(), None),
            (":".into(), Some(Hsla::blue())),
            (" &str = ".into(), None),
            ("\"\n    one\n".into(), Some(Hsla::red())),
            ("\n".into(), None),
            ("    two\n    three\n\"".into(), Some(Hsla::red())),
            (";".into(), Some(Hsla::blue())),
            ("\n".into(), None),
            ("const".into(), Some(Hsla::green())),
            (" B".into(), None),
            (":".into(), Some(Hsla::blue())),
            (" &str = ".into(), None),
            ("\"four\"".into(), Some(Hsla::red())),
            (";".into(), Some(Hsla::blue())),
            ("\n".into(), None),
        ]
    );
}

#[gpui::test]
async fn test_chunks_with_diagnostics_across_blocks(cx: &mut gpui::TestAppContext) {
    cx.background_executor
        .set_block_on_ticks(usize::MAX..=usize::MAX);

    let text = r#"
        struct A {
            b: usize;
        }
        const c: usize = 1;
    "#
    .unindent();

    cx.update(|cx| init_test(cx, &|_| {}));

    let buffer = cx.new(|cx| Buffer::local(text, cx));

    buffer.update(cx, |buffer, cx| {
        buffer.update_diagnostics(
            LanguageServerId(0),
            DiagnosticSet::new(
                [DiagnosticEntry {
                    range: PointUtf16::new(0, 0)..PointUtf16::new(2, 1),
                    diagnostic: Diagnostic {
                        severity: lsp::DiagnosticSeverity::ERROR,
                        group_id: 1,
                        message: "hi".into(),
                        ..Default::default()
                    },
                }],
                buffer,
            ),
            cx,
        )
    });

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let buffer_snapshot = buffer.read_with(cx, |buffer, cx| buffer.snapshot(cx));

    let map = cx.new(|cx| {
        DisplayMap::new(
            buffer,
            font("Courier"),
            px(16.0),
            None,
            1,
            1,
            FoldPlaceholder::test(),
            DiagnosticSeverity::Warning,
            cx,
        )
    });

    let black = gpui::black().to_rgb();
    let red = gpui::red().to_rgb();

    // Insert a block in the middle of a multi-line diagnostic.
    map.update(cx, |map, cx| {
        map.highlight_text(
            HighlightKey::Editor,
            vec![
                buffer_snapshot.anchor_before(Point::new(3, 9))
                    ..buffer_snapshot.anchor_after(Point::new(3, 14)),
                buffer_snapshot.anchor_before(Point::new(3, 17))
                    ..buffer_snapshot.anchor_after(Point::new(3, 18)),
            ],
            red.into(),
            false,
            cx,
        );
        map.insert_blocks(
            [BlockProperties {
                placement: BlockPlacement::Below(buffer_snapshot.anchor_before(Point::new(1, 0))),
                height: Some(1),
                style: BlockStyle::Sticky,
                render: Arc::new(|_| div().into_any()),
                priority: 0,
            }],
            cx,
        )
    });

    let snapshot = map.update(cx, |map, cx| map.snapshot(cx));
    let mut chunks = Vec::<(String, Option<lsp::DiagnosticSeverity>, Rgba)>::new();
    for chunk in snapshot.chunks(
        DisplayRow(0)..DisplayRow(5),
        LanguageAwareStyling {
            tree_sitter: true,
            diagnostics: true,
        },
        Default::default(),
    ) {
        let color = chunk
            .highlight_style
            .and_then(|style| style.color)
            .map_or(black, |color| color.to_rgb());
        if let Some((last_chunk, last_severity, last_color)) = chunks.last_mut()
            && *last_severity == chunk.diagnostic_severity
            && *last_color == color
        {
            last_chunk.push_str(chunk.text);
            continue;
        }

        chunks.push((chunk.text.to_string(), chunk.diagnostic_severity, color));
    }

    assert_eq!(
        chunks,
        [
            (
                "struct A {\n    b: usize;\n".into(),
                Some(lsp::DiagnosticSeverity::ERROR),
                black
            ),
            ("\n".into(), None, black),
            ("}".into(), Some(lsp::DiagnosticSeverity::ERROR), black),
            ("\nconst c: ".into(), None, black),
            ("usize".into(), None, red),
            (" = ".into(), None, black),
            ("1".into(), None, red),
            (";\n".into(), None, black),
        ]
    );
}
