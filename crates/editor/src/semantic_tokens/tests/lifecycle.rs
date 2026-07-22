use super::*;

#[gpui::test]
async fn test_stopping_language_server_clears_semantic_tokens(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..LanguageSettingsContent::default()
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
                            token_modifiers: Vec::new(),
                        },
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                        ..lsp::SemanticTokensOptions::default()
                    },
                ),
            ),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(lsp::SemanticTokensResult::Tokens(
                lsp::SemanticTokens {
                    data: vec![
                        0, // delta_line
                        3, // delta_start
                        4, // length
                        0, // token_type
                        0, // token_modifiers_bitset
                    ],
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());
    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)],
        "Semantic tokens should be present before stopping the server"
    );

    cx.update_editor(|editor, _, cx| {
        let buffers = editor.buffer.read(cx).all_buffers().into_iter().collect();
        editor.project.as_ref().unwrap().update(cx, |project, cx| {
            project.stop_language_servers_for_buffers(buffers, HashSet::default(), cx);
        })
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        Vec::new(),
        "Semantic tokens should be cleared after stopping the server"
    );
}
async fn test_disabling_semantic_tokens_setting_clears_highlights(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..LanguageSettingsContent::default()
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
                            token_modifiers: Vec::new(),
                        },
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                        ..lsp::SemanticTokensOptions::default()
                    },
                ),
            ),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(lsp::SemanticTokensResult::Tokens(
                lsp::SemanticTokens {
                    data: vec![
                        0, // delta_line
                        3, // delta_start
                        4, // length
                        0, // token_type
                        0, // token_modifiers_bitset
                    ],
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());
    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)],
        "Semantic tokens should be present before disabling the setting"
    );

    update_test_language_settings(&mut cx, &|language_settings| {
        language_settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Off),
                ..LanguageSettingsContent::default()
            },
        );
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        Vec::new(),
        "Semantic tokens should be cleared after disabling the setting"
    );
}
