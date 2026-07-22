use super::*;

#[gpui::test]
async fn test_rust_label_for_symbol() {
    let adapter = Arc::new(RustLspAdapter);
    let language = language("rust", tree_sitter_rust::LANGUAGE.into());
    let grammar = language.grammar().unwrap();
    let theme = SyntaxTheme::new_test([
        ("type", Hsla::default()),
        ("keyword", Hsla::default()),
        ("function", Hsla::default()),
        ("property", Hsla::default()),
    ]);

    language.set_theme(&theme);

    let highlight_function = grammar.highlight_id_for_name("function").unwrap();
    let highlight_type = grammar.highlight_id_for_name("type").unwrap();
    let highlight_keyword = grammar.highlight_id_for_name("keyword").unwrap();

    assert_eq!(
        adapter
            .label_for_symbol(
                &language::Symbol {
                    name: "hello".to_string(),
                    kind: lsp::SymbolKind::FUNCTION,
                    container_name: None,
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "fn hello".to_string(),
            3..8,
            vec![(0..2, highlight_keyword), (3..8, highlight_function)],
        ))
    );

    assert_eq!(
        adapter
            .label_for_symbol(
                &language::Symbol {
                    name: "World".to_string(),
                    kind: lsp::SymbolKind::TYPE_PARAMETER,
                    container_name: None,
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "type World".to_string(),
            5..10,
            vec![(0..4, highlight_keyword), (5..10, highlight_type)],
        ))
    );

    assert_eq!(
        adapter
            .label_for_symbol(
                &language::Symbol {
                    name: "mav".to_string(),
                    kind: lsp::SymbolKind::PACKAGE,
                    container_name: None,
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "extern crate mav".to_string(),
            13..16,
            vec![(0..6, highlight_keyword), (7..12, highlight_keyword),],
        ))
    );

    assert_eq!(
        adapter
            .label_for_symbol(
                &language::Symbol {
                    name: "Variant".to_string(),
                    kind: lsp::SymbolKind::ENUM_MEMBER,
                    container_name: None,
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "Variant".to_string(),
            0..7,
            vec![(0..7, highlight_type)],
        ))
    );
}

#[gpui::test]
async fn test_rust_autoindent(cx: &mut TestAppContext) {
    // cx.executor().set_block_on_ticks(usize::MAX..=usize::MAX);
    cx.update(|cx| {
        let test_settings = SettingsStore::test(cx);
        cx.set_global(test_settings);
        cx.update_global::<SettingsStore, _>(|store, cx| {
            store.update_user_settings(cx, |s| {
                s.project.all_languages.defaults.tab_size = NonZeroU32::new(2);
            });
        });
    });

    let language = crate::language("rust", tree_sitter_rust::LANGUAGE.into());

    cx.new(|cx| {
        let mut buffer = Buffer::local("", cx).with_language(language, cx);

        // indent between braces
        buffer.set_text("fn a() {}", cx);
        let ix = buffer.len() - 1;
        buffer.edit([(ix..ix, "\n\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n  \n}");

        // indent between braces, even after empty lines
        buffer.set_text("fn a() {\n\n\n}", cx);
        let ix = buffer.len() - 2;
        buffer.edit([(ix..ix, "\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n\n\n  \n}");

        // indent a line that continues a field expression
        buffer.set_text("fn a() {\n  \n}", cx);
        let ix = buffer.len() - 2;
        buffer.edit([(ix..ix, "b\n.c")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n  b\n    .c\n}");

        // indent further lines that continue the field expression, even after empty lines
        let ix = buffer.len() - 2;
        buffer.edit([(ix..ix, "\n\n.d")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "fn a() {\n  b\n    .c\n    \n    .d\n}");

        // dedent the line after the field expression
        let ix = buffer.len() - 2;
        buffer.edit([(ix..ix, ";\ne")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(
            buffer.text(),
            "fn a() {\n  b\n    .c\n    \n    .d;\n  e\n}"
        );

        // indent inside a struct within a call
        buffer.set_text("const a: B = c(D {});", cx);
        let ix = buffer.len() - 3;
        buffer.edit([(ix..ix, "\n\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "const a: B = c(D {\n  \n});");

        // indent further inside a nested call
        let ix = buffer.len() - 4;
        buffer.edit([(ix..ix, "e: f(\n\n)")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(buffer.text(), "const a: B = c(D {\n  e: f(\n    \n  )\n});");

        // keep that indent after an empty line
        let ix = buffer.len() - 8;
        buffer.edit([(ix..ix, "\n")], Some(AutoindentMode::EachLine), cx);
        assert_eq!(
            buffer.text(),
            "const a: B = c(D {\n  e: f(\n    \n    \n  )\n});"
        );

        buffer
    });
}
