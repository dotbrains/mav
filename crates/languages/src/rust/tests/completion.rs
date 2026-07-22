use super::*;

#[gpui::test]
async fn test_rust_label_for_completion() {
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
    let highlight_field = grammar.highlight_id_for_name("property").unwrap();

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FUNCTION),
                    label: "hello(…)".to_string(),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some("(use crate::foo)".into()),
                        description: Some("fn(&mut Option<T>) -> Vec<T>".to_string())
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "hello(&mut Option<T>) -> Vec<T> (use crate::foo)".to_string(),
            0..5,
            vec![
                (0..5, highlight_function),
                (7..10, highlight_keyword),
                (11..17, highlight_type),
                (18..19, highlight_type),
                (25..28, highlight_type),
                (29..30, highlight_type),
            ],
        ))
    );
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FUNCTION),
                    label: "hello(…)".to_string(),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some("(use crate::foo)".into()),
                        description: Some("async fn(&mut Option<T>) -> Vec<T>".to_string()),
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "hello(&mut Option<T>) -> Vec<T> (use crate::foo)".to_string(),
            0..5,
            vec![
                (0..5, highlight_function),
                (7..10, highlight_keyword),
                (11..17, highlight_type),
                (18..19, highlight_type),
                (25..28, highlight_type),
                (29..30, highlight_type),
            ],
        ))
    );
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FIELD),
                    label: "len".to_string(),
                    detail: Some("usize".to_string()),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "len: usize".to_string(),
            0..3,
            vec![(0..3, highlight_field), (5..10, highlight_type),],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FUNCTION),
                    label: "hello(…)".to_string(),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some("(use crate::foo)".to_string()),
                        description: Some("fn(&mut Option<T>) -> Vec<T>".to_string()),
                    }),

                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "hello(&mut Option<T>) -> Vec<T> (use crate::foo)".to_string(),
            0..5,
            vec![
                (0..5, highlight_function),
                (7..10, highlight_keyword),
                (11..17, highlight_type),
                (18..19, highlight_type),
                (25..28, highlight_type),
                (29..30, highlight_type),
            ],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FUNCTION),
                    label: "hello".to_string(),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: Some("(use crate::foo)".to_string()),
                        description: Some("fn(&mut Option<T>) -> Vec<T>".to_string()),
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "hello(&mut Option<T>) -> Vec<T> (use crate::foo)".to_string(),
            0..5,
            vec![
                (0..5, highlight_function),
                (7..10, highlight_keyword),
                (11..17, highlight_type),
                (18..19, highlight_type),
                (25..28, highlight_type),
                (29..30, highlight_type),
            ],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::METHOD),
                    label: "await.as_deref_mut()".to_string(),
                    filter_text: Some("as_deref_mut".to_string()),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: None,
                        description: Some("fn(&mut self) -> IterMut<'_, T>".to_string()),
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "await.as_deref_mut(&mut self) -> IterMut<'_, T>".to_string(),
            6..18,
            vec![
                (6..18, HighlightId::new(2)),
                (20..23, HighlightId::new(1)),
                (33..40, HighlightId::new(0)),
                (45..46, HighlightId::new(0))
            ],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::METHOD),
                    label: "as_deref_mut()".to_string(),
                    filter_text: Some("as_deref_mut".to_string()),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: None,
                        description: Some(
                            "pub fn as_deref_mut(&mut self) -> IterMut<'_, T>".to_string()
                        ),
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "pub fn as_deref_mut(&mut self) -> IterMut<'_, T>".to_string(),
            7..19,
            vec![
                (0..3, HighlightId::new(1)),
                (4..6, HighlightId::new(1)),
                (7..19, HighlightId::new(2)),
                (21..24, HighlightId::new(1)),
                (34..41, HighlightId::new(0)),
                (46..47, HighlightId::new(0))
            ],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::METHOD),
                    label: "sync_all(…)".to_string(),
                    filter_text: Some("sync_allfsync".to_string()),
                    label_details: Some(CompletionItemLabelDetails {
                        detail: None,
                        description: Some("pub fn sync_all(&self) -> io::Result<()>".to_string()),
                    }),
                    ..Default::default()
                },
                &language
            )
            .await,
        Some(CodeLabel::new(
            "pub fn sync_all(&self) -> io::Result<()>".to_string(),
            7..15,
            vec![
                (0..3, HighlightId::new(1)),
                (4..6, HighlightId::new(1)),
                (7..15, HighlightId::new(2)),
                (30..36, HighlightId::new(0))
            ],
        ))
    );

    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::FIELD),
                    label: "inner_value".to_string(),
                    filter_text: Some("value".to_string()),
                    detail: Some("String".to_string()),
                    ..Default::default()
                },
                &language,
            )
            .await,
        Some(CodeLabel::new(
            "inner_value: String".to_string(),
            6..11,
            vec![(0..11, HighlightId::new(3)), (13..19, HighlightId::new(0))],
        ))
    );

    // Snippet with insert tabstop (empty placeholder)
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::SNIPPET),
                    label: "println!".to_string(),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::default(),
                        new_text: "println!(\"$1\", $2)$0".to_string(),
                    })),
                    ..Default::default()
                },
                &language,
            )
            .await,
        Some(CodeLabel::new(
            "println!(\"…\", …)".to_string(),
            0..8,
            vec![
                (10..13, HighlightId::TABSTOP_INSERT_ID),
                (16..19, HighlightId::TABSTOP_INSERT_ID),
                (0..7, HighlightId::new(2)),
                (7..8, HighlightId::new(2)),
            ],
        ))
    );

    // Snippet with replace tabstop (placeholder with default text)
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::SNIPPET),
                    label: "vec!".to_string(),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::default(),
                        new_text: "vec![${1:elem}]$0".to_string(),
                    })),
                    ..Default::default()
                },
                &language,
            )
            .await,
        Some(CodeLabel::new(
            "vec![elem]".to_string(),
            0..4,
            vec![
                (5..9, HighlightId::TABSTOP_REPLACE_ID),
                (0..3, HighlightId::new(2)),
                (3..4, HighlightId::new(2)),
            ],
        ))
    );

    // Snippet with tabstop appearing more than once
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::SNIPPET),
                    label: "if let".to_string(),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::default(),
                        new_text: "if let ${1:pat} = $1 {\n    $0\n}".to_string(),
                    })),
                    ..Default::default()
                },
                &language,
            )
            .await,
        Some(CodeLabel::new(
            "if let pat = … {\n    \n}".to_string(),
            0..6,
            vec![
                (7..10, HighlightId::TABSTOP_REPLACE_ID),
                (13..16, HighlightId::TABSTOP_INSERT_ID),
                (0..2, HighlightId::new(1)),
                (3..6, HighlightId::new(1)),
            ],
        ))
    );

    // Snippet with tabstops not in left-to-right order
    assert_eq!(
        adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::SNIPPET),
                    label: "for".to_string(),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::default(),
                        new_text: "for ${2:item} in ${1:iter} {\n    $0\n}".to_string(),
                    })),
                    ..Default::default()
                },
                &language,
            )
            .await,
        Some(CodeLabel::new(
            "for item in iter {\n    \n}".to_string(),
            0..3,
            vec![
                (4..8, HighlightId::TABSTOP_REPLACE_ID),
                (12..16, HighlightId::TABSTOP_REPLACE_ID),
                (0..3, HighlightId::new(1)),
                (9..11, HighlightId::new(1)),
            ],
        ))
    );

    // Postfix completion without actual tabstops (only implicit final $0)
    // The label should use completion.label so it can be filtered by "ref"
    let ref_completion = adapter
        .label_for_completion(
            &lsp::CompletionItem {
                kind: Some(lsp::CompletionItemKind::SNIPPET),
                label: "ref".to_string(),
                filter_text: Some("ref".to_string()),
                label_details: Some(CompletionItemLabelDetails {
                    detail: None,
                    description: Some("&expr".to_string()),
                }),
                detail: Some("&expr".to_string()),
                insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                    range: lsp::Range::default(),
                    new_text: "&String::new()".to_string(),
                })),
                ..Default::default()
            },
            &language,
        )
        .await;
    assert!(
        ref_completion.is_some(),
        "ref postfix completion should have a label"
    );
    let ref_label = ref_completion.unwrap();
    let filter_text = &ref_label.text[ref_label.filter_range.clone()];
    assert!(
        filter_text.contains("ref"),
        "filter range text '{filter_text}' should contain 'ref' for filtering to work",
    );

    // Test for correct range calculation with mixed empty and non-empty tabstops.(See https://github.com/mav-industries/mav/issues/44825)
    let res = adapter
            .label_for_completion(
                &lsp::CompletionItem {
                    kind: Some(lsp::CompletionItemKind::STRUCT),
                    label: "Particles".to_string(),
                    insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::default(),
                        new_text: "Particles { pos_x: $1, pos_y: $2, vel_x: $3, vel_y: $4, acc_x: ${5:()}, acc_y: ${6:()}, mass: $7 }$0".to_string(),
                    })),
                    ..Default::default()
                },
                &language,
            )
            .await
            .unwrap();

    assert_eq!(
        res,
        CodeLabel::new(
            "Particles { pos_x: …, pos_y: …, vel_x: …, vel_y: …, acc_x: (), acc_y: (), mass: … }"
                .to_string(),
            0..9,
            vec![
                (19..22, HighlightId::TABSTOP_INSERT_ID),
                (31..34, HighlightId::TABSTOP_INSERT_ID),
                (43..46, HighlightId::TABSTOP_INSERT_ID),
                (55..58, HighlightId::TABSTOP_INSERT_ID),
                (67..69, HighlightId::TABSTOP_REPLACE_ID),
                (78..80, HighlightId::TABSTOP_REPLACE_ID),
                (88..91, HighlightId::TABSTOP_INSERT_ID),
                (0..9, highlight_type),
                (60..65, highlight_field),
                (71..76, highlight_field),
            ],
        )
    );
}
