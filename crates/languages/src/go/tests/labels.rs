use super::*;

#[gpui::test]
async fn test_go_label_for_completion() {
    let adapter = Arc::new(GoLspAdapter);
    let language = go_language();

    let theme = SyntaxTheme::new_test([
        ("type", Hsla::default()),
        ("keyword", Hsla::default()),
        ("function", Hsla::default()),
        ("number", Hsla::default()),
        ("property", Hsla::default()),
    ]);
    language.set_theme(&theme);

    let grammar = language.grammar().unwrap();
    let highlight_function = grammar.highlight_id_for_name("function").unwrap();
    let highlight_type = grammar.highlight_id_for_name("type").unwrap();
    let highlight_keyword = grammar.highlight_id_for_name("keyword").unwrap();
    let highlight_number = grammar.highlight_id_for_name("number").unwrap();
    let highlight_field = grammar.highlight_id_for_name("property").unwrap();

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FUNCTION),
                    label: "Hello".to_string(),
                    detail: Some("func(a B) c.D".to_string()),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "Hello(a B) c.D".to_string(),
            0..5,
            vec![
                (0..5, highlight_function),
                (8..9, highlight_type),
                (13..14, highlight_type),
            ]
        ))
    );

    // Nested methods
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::METHOD),
                    label: "one.two.Three".to_string(),
                    detail: Some("func() [3]interface{}".to_string()),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "one.two.Three() [3]interface{}".to_string(),
            0..13,
            vec![
                (8..13, highlight_function),
                (17..18, highlight_number),
                (19..28, highlight_keyword),
            ],
        ))
    );

    // Nested fields
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FIELD),
                    label: "two.Three".to_string(),
                    detail: Some("a.Bcd".to_string()),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "two.Three a.Bcd".to_string(),
            0..9,
            vec![(4..9, highlight_field), (12..15, highlight_type)],
        ))
    );
}
