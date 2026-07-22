use super::*;

#[gpui::test]
async fn test_semantic_token_disabling_with_empty_rule(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|s| {
        s.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..Default::default()
            },
        );
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            semantic_tokens_provider: Some(
                lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                    lsp::SemanticTokensOptions {
                        legend: lsp::SemanticTokensLegend {
                            token_types: vec!["function".into()],
                            token_modifiers: vec![],
                        },
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                        ..Default::default()
                    },
                ),
            ),
            ..Default::default()
        },
        cx,
    )
    .await;

    let mut full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(lsp::SemanticTokensResult::Tokens(
                lsp::SemanticTokens {
                    data: vec![0, 3, 4, 0, 0],
                    result_id: None,
                },
            )))
        },
    );

    // Verify it highlights by default
    cx.set_state("ˇfn main() {}");
    full_request.next().await;
    cx.run_until_parked();
    assert_eq!(extract_semantic_highlights(&cx.editor, &cx).len(), 1);

    // Apply EMPTY rule to disable it
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    semantic_token_rules: Some(SemanticTokenRules {
                        rules: vec![SemanticTokenRule {
                            token_type: Some("function".to_string()),
                            ..Default::default()
                        }],
                    }),
                    ..Default::default()
                });
            });
        });
    });

    cx.set_state("ˇfn main() { }");
    full_request.next().await;
    cx.run_until_parked();

    assert!(
        extract_semantic_highlights(&cx.editor, &cx).is_empty(),
        "Highlighting should be disabled by empty style setting"
    );
}
async fn test_semantic_token_broad_rule_disables_specific_token(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_language_settings(cx, &|s| {
        s.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..Default::default()
            },
        );
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            semantic_tokens_provider: Some(
                lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                    lsp::SemanticTokensOptions {
                        legend: lsp::SemanticTokensLegend {
                            token_types: vec!["comment".into()],
                            token_modifiers: vec!["documentation".into()],
                        },
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                        ..Default::default()
                    },
                ),
            ),
            ..Default::default()
        },
        cx,
    )
    .await;

    let mut full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(lsp::SemanticTokensResult::Tokens(
                lsp::SemanticTokens {
                    data: vec![0, 0, 5, 0, 1], // comment [documentation]
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇ/// d\n");
    full_request.next().await;
    cx.run_until_parked();
    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx).len(),
        1,
        "Documentation comment should be highlighted"
    );

    // Apply a BROAD empty rule for "comment" (no modifiers)
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    semantic_token_rules: Some(SemanticTokenRules {
                        rules: vec![SemanticTokenRule {
                            token_type: Some("comment".to_string()),
                            ..Default::default()
                        }],
                    }),
                    ..Default::default()
                });
            });
        });
    });

    cx.set_state("ˇ/// d\n");
    full_request.next().await;
    cx.run_until_parked();

    assert!(
        extract_semantic_highlights(&cx.editor, &cx).is_empty(),
        "Broad empty rule should disable specific documentation comment"
    );
}
async fn test_semantic_token_specific_rule_does_not_disable_broad_token(cx: &mut TestAppContext) {
    use gpui::UpdateGlobal as _;
    use settings::{GlobalLspSettingsContent, SemanticTokenRule};

    init_test(cx, |_| {});
    update_test_language_settings(cx, &|s| {
        s.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..Default::default()
            },
        );
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            semantic_tokens_provider: Some(
                lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                    lsp::SemanticTokensOptions {
                        legend: lsp::SemanticTokensLegend {
                            token_types: vec!["comment".into()],
                            token_modifiers: vec!["documentation".into()],
                        },
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                        ..Default::default()
                    },
                ),
            ),
            ..Default::default()
        },
        cx,
    )
    .await;

    let mut full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(lsp::SemanticTokensResult::Tokens(
                lsp::SemanticTokens {
                    data: vec![
                        0, 0, 5, 0, 1, // comment [documentation]
                        1, 0, 5, 0, 0, // normal comment
                    ],
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇ/// d\n// n\n");
    full_request.next().await;
    cx.run_until_parked();
    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx).len(),
        2,
        "Both documentation and normal comments should be highlighted initially"
    );

    // Apply a SPECIFIC empty rule for documentation only
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    semantic_token_rules: Some(SemanticTokenRules {
                        rules: vec![SemanticTokenRule {
                            token_type: Some("comment".to_string()),
                            token_modifiers: vec!["documentation".to_string()],
                            ..Default::default()
                        }],
                    }),
                    ..Default::default()
                });
            });
        });
    });

    cx.set_state("ˇ/// d\n// n\n");
    full_request.next().await;
    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx).len(),
        1,
        "Normal comment should still be highlighted (matched by default rule)"
    );
}
