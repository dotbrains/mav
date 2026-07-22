use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, rgba};
    use pretty_assertions::assert_matches;

    #[test]
    fn test_highlight_map() {
        let theme = SyntaxTheme::new(
            [
                ("function", rgba(0x100000ff)),
                ("function.method", rgba(0x200000ff)),
                ("function.async", rgba(0x300000ff)),
                ("variable.builtin.self.rust", rgba(0x400000ff)),
                ("variable.builtin", rgba(0x500000ff)),
                ("variable", rgba(0x600000ff)),
            ]
            .iter()
            .map(|(name, color)| (name.to_string(), (*color).into())),
        );

        let capture_names = &[
            "function.special",
            "function.async.rust",
            "variable.builtin.self",
        ];

        let map = build_highlight_map(capture_names, &theme);
        assert_eq!(
            theme.get_capture_name(map.get(0).unwrap()),
            Some("function")
        );
        assert_eq!(
            theme.get_capture_name(map.get(1).unwrap()),
            Some("function.async")
        );
        assert_eq!(
            theme.get_capture_name(map.get(2).unwrap()),
            Some("variable.builtin")
        );
    }

    #[test]
    fn test_with_parser_resets_after_cancellation() {
        use std::ops::ControlFlow;
        use tree_sitter::{Language as TsLanguage, ParseOptions};

        let rust_language: TsLanguage = tree_sitter_rust::LANGUAGE.into();

        // Drain the shared pool so this test sees a deterministic LIFO order:
        // the parser we push at the end of the first `with_parser` call is the
        // one we pop at the start of the second call.
        PARSERS.lock().clear();

        // Large enough that tree-sitter invokes the progress callback before
        // the parse completes; otherwise the cancellation never fires.
        let large_input = format!("fn a() {{ {} }}", "b(c, d); e(f, g); ".repeat(5000));
        let small_input = "fn z() {}";

        // Cancel a parse via the progress callback. Tree-sitter retains the
        // in-progress parse state on the parser (its `canceled_balancing` flag
        // and/or non-empty parse stack), and the next call to
        // `parse_with_options` will *resume* that parse unless the parser is
        // reset first.
        let cancelled = with_parser(|parser| {
            parser.set_language(&rust_language).unwrap();
            let bytes = large_input.as_bytes();
            let mut break_immediately = |_: &_| ControlFlow::Break(());
            parser.parse_with_options(
                &mut |offset, _| {
                    if offset < bytes.len() {
                        &bytes[offset..]
                    } else {
                        &[]
                    }
                },
                None,
                Some(ParseOptions {
                    progress_callback: Some(&mut break_immediately),
                }),
            )
        });
        assert!(
            cancelled.is_none(),
            "first parse should be cancelled by the progress callback"
        );

        // Deliberately do NOT call `set_language` here: tree-sitter's
        // `ts_parser_set_language` internally calls `ts_parser_reset`, which
        // would mask the very bug we're checking for. Instead we rely on the
        // language being preserved across `parser.reset()` (it is) and verify
        // that `with_parser` itself produces a clean parser for the next user.
        let tree = with_parser(|parser| {
            let bytes = small_input.as_bytes();
            parser
                .parse_with_options(
                    &mut |offset, _| {
                        if offset < bytes.len() {
                            &bytes[offset..]
                        } else {
                            &[]
                        }
                    },
                    None,
                    None,
                )
                .expect("parse of small_input should succeed")
        });

        assert_eq!(tree.root_node().byte_range(), 0..small_input.len());
        assert_eq!(tree.root_node().kind(), "source_file");
        assert!(
            !tree.root_node().has_error(),
            "tree should be error-free, got: {}",
            tree.root_node().to_sexp()
        );
    }

    #[gpui::test(iterations = 10)]

    async fn test_language_loading(cx: &mut TestAppContext) {
        let languages = LanguageRegistry::test(cx.executor());
        let languages = Arc::new(languages);
        languages.register_native_grammars([
            ("json", tree_sitter_json::LANGUAGE),
            ("rust", tree_sitter_rust::LANGUAGE),
        ]);
        languages.register_test_language(LanguageConfig {
            name: "JSON".into(),
            grammar: Some("json".into()),
            matcher: LanguageMatcher {
                path_suffixes: vec!["json".into()],
                ..Default::default()
            },
            ..Default::default()
        });
        languages.register_test_language(LanguageConfig {
            name: "Rust".into(),
            grammar: Some("rust".into()),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".into()],
                ..Default::default()
            },
            ..Default::default()
        });
        assert_eq!(
            languages.language_names(),
            &[
                LanguageName::new_static("JSON"),
                LanguageName::new_static("Plain Text"),
                LanguageName::new_static("Rust"),
            ]
        );

        let rust1 = languages.language_for_name("Rust");
        let rust2 = languages.language_for_name("Rust");

        // Ensure language is still listed even if it's being loaded.
        assert_eq!(
            languages.language_names(),
            &[
                LanguageName::new_static("JSON"),
                LanguageName::new_static("Plain Text"),
                LanguageName::new_static("Rust"),
            ]
        );

        let (rust1, rust2) = futures::join!(rust1, rust2);
        assert!(Arc::ptr_eq(&rust1.unwrap(), &rust2.unwrap()));

        // Ensure language is still listed even after loading it.
        assert_eq!(
            languages.language_names(),
            &[
                LanguageName::new_static("JSON"),
                LanguageName::new_static("Plain Text"),
                LanguageName::new_static("Rust"),
            ]
        );

        // Loading an unknown language returns an error.
        assert!(languages.language_for_name("Unknown").await.is_err());
    }

    #[gpui::test]
    async fn test_completion_label_omits_duplicate_data() {
        let regular_completion_item_1 = lsp::CompletionItem {
            label: "regular1".to_string(),
            detail: Some("detail1".to_string()),
            label_details: Some(lsp::CompletionItemLabelDetails {
                detail: None,
                description: Some("description 1".to_string()),
            }),
            ..lsp::CompletionItem::default()
        };

        let regular_completion_item_2 = lsp::CompletionItem {
            label: "regular2".to_string(),
            label_details: Some(lsp::CompletionItemLabelDetails {
                detail: None,
                description: Some("description 2".to_string()),
            }),
            ..lsp::CompletionItem::default()
        };

        let completion_item_with_duplicate_detail_and_proper_description = lsp::CompletionItem {
            detail: Some(regular_completion_item_1.label.clone()),
            ..regular_completion_item_1.clone()
        };

        let completion_item_with_duplicate_detail = lsp::CompletionItem {
            detail: Some(regular_completion_item_1.label.clone()),
            label_details: None,
            ..regular_completion_item_1.clone()
        };

        let completion_item_with_duplicate_description = lsp::CompletionItem {
            label_details: Some(lsp::CompletionItemLabelDetails {
                detail: None,
                description: Some(regular_completion_item_2.label.clone()),
            }),
            ..regular_completion_item_2.clone()
        };

        assert_eq!(
            CodeLabel::fallback_for_completion(&regular_completion_item_1, None).text,
            format!(
                "{} {}",
                regular_completion_item_1.label,
                regular_completion_item_1.detail.unwrap()
            ),
            "LSP completion items with both detail and label_details.description should prefer detail"
        );
        assert_eq!(
            CodeLabel::fallback_for_completion(&regular_completion_item_2, None).text,
            format!(
                "{} {}",
                regular_completion_item_2.label,
                regular_completion_item_2
                    .label_details
                    .as_ref()
                    .unwrap()
                    .description
                    .as_ref()
                    .unwrap()
            ),
            "LSP completion items without detail but with label_details.description should use that"
        );
        assert_eq!(
            CodeLabel::fallback_for_completion(
                &completion_item_with_duplicate_detail_and_proper_description,
                None
            )
            .text,
            format!(
                "{} {}",
                regular_completion_item_1.label,
                regular_completion_item_1
                    .label_details
                    .as_ref()
                    .unwrap()
                    .description
                    .as_ref()
                    .unwrap()
            ),
            "LSP completion items with both detail and label_details.description should prefer description only if the detail duplicates the completion label"
        );
        assert_eq!(
            CodeLabel::fallback_for_completion(&completion_item_with_duplicate_detail, None).text,
            regular_completion_item_1.label,
            "LSP completion items with duplicate label and detail, should omit the detail"
        );
        assert_eq!(
            CodeLabel::fallback_for_completion(&completion_item_with_duplicate_description, None)
                .text,
            regular_completion_item_2.label,
            "LSP completion items with duplicate label and detail, should omit the detail"
        );
    }

    #[test]
    fn test_deserializing_comments_backwards_compat() {
        // current version of `block_comment` and `documentation_comment` work
        {
            let config: LanguageConfig = ::toml::from_str(
                r#"
                name = "Foo"
                block_comment = { start = "a", end = "b", prefix = "c", tab_size = 1 }
                documentation_comment = { start = "d", end = "e", prefix = "f", tab_size = 2 }
                "#,
            )
            .unwrap();
            assert_matches!(config.block_comment, Some(BlockCommentConfig { .. }));
            assert_matches!(
                config.documentation_comment,
                Some(BlockCommentConfig { .. })
            );

            let block_config = config.block_comment.unwrap();
            assert_eq!(block_config.start.as_ref(), "a");
            assert_eq!(block_config.end.as_ref(), "b");
            assert_eq!(block_config.prefix.as_ref(), "c");
            assert_eq!(block_config.tab_size, 1);

            let doc_config = config.documentation_comment.unwrap();
            assert_eq!(doc_config.start.as_ref(), "d");
            assert_eq!(doc_config.end.as_ref(), "e");
            assert_eq!(doc_config.prefix.as_ref(), "f");
            assert_eq!(doc_config.tab_size, 2);
        }

        // former `documentation` setting is read into `documentation_comment`
        {
            let config: LanguageConfig = ::toml::from_str(
                r#"
                name = "Foo"
                documentation = { start = "a", end = "b", prefix = "c", tab_size = 1}
                "#,
            )
            .unwrap();
            assert_matches!(
                config.documentation_comment,
                Some(BlockCommentConfig { .. })
            );

            let config = config.documentation_comment.unwrap();
            assert_eq!(config.start.as_ref(), "a");
            assert_eq!(config.end.as_ref(), "b");
            assert_eq!(config.prefix.as_ref(), "c");
            assert_eq!(config.tab_size, 1);
        }

        // old block_comment format is read into BlockCommentConfig
        {
            let config: LanguageConfig = ::toml::from_str(
                r#"
                name = "Foo"
                block_comment = ["a", "b"]
                "#,
            )
            .unwrap();
            assert_matches!(config.block_comment, Some(BlockCommentConfig { .. }));

            let config = config.block_comment.unwrap();
            assert_eq!(config.start.as_ref(), "a");
            assert_eq!(config.end.as_ref(), "b");
            assert_eq!(config.prefix.as_ref(), "");
            assert_eq!(config.tab_size, 0);
        }
    }
}
