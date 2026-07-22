use super::*;

async fn lsp_semantic_tokens_full_capability(cx: &mut TestAppContext) {
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

    let full_counter = Arc::new(AtomicUsize::new(0));
    let full_counter_clone = full_counter.clone();

    let mut full_request =
        cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(move |_, _, _| {
            full_counter_clone.fetch_add(1, atomic::Ordering::Release);
            async move {
                Ok(Some(lsp::SemanticTokensResult::Tokens(
                    lsp::SemanticTokens {
                        data: vec![
                            0, // delta_line
                            3, // delta_start
                            4, // length
                            0, // token_type
                            0, // token_modifiers_bitset
                        ],
                        // The server isn't capable of deltas, so even though we sent back
                        // a result ID, the client shouldn't request a delta.
                        result_id: Some("a".into()),
                    },
                )))
            }
        });

    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());

    cx.run_until_parked();

    cx.set_state("ˇfn main() { a }");
    assert!(full_request.next().await.is_some());

    cx.run_until_parked();

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)]
    );

    assert_eq!(full_counter.load(atomic::Ordering::Acquire), 2);
}
async fn lsp_semantic_tokens_dynamic_registration_requeries_open_document(cx: &mut TestAppContext) {
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

    // The server advertises no semantic tokens capability up front; it only
    // registers `textDocument/semanticTokens` dynamically, after the document
    // is already open (as Roslyn does).
    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;

    let full_counter = Arc::new(AtomicUsize::new(0));
    let _full_request = cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>({
        let full_counter = full_counter.clone();
        move |_, _, _| {
            full_counter.fetch_add(1, atomic::Ordering::Release);
            async move {
                Ok(Some(lsp::SemanticTokensResult::Tokens(
                    lsp::SemanticTokens {
                        data: vec![0, 3, 4, 0, 0],
                        result_id: None,
                    },
                )))
            }
        }
    });

    cx.set_state("ˇfn main() {}");
    // Drain the refresh scheduled on open (while no capability exists yet), so a
    // later request can only come from the dynamic-registration refresh itself.
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(
        full_counter.load(atomic::Ordering::Acquire),
        0,
        "no semantic tokens should be requested before the capability is registered"
    );
    assert!(
        extract_semantic_highlights(&cx.editor, &cx).is_empty(),
        "no semantic highlights before the capability is registered"
    );

    cx.lsp
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: "semantic-tokens".to_string(),
                    method: "textDocument/semanticTokens".to_string(),
                    register_options: Some(
                        serde_json::to_value(lsp::SemanticTokensRegistrationOptions {
                            text_document_registration_options:
                                lsp::TextDocumentRegistrationOptions {
                                    document_selector: None,
                                },
                            semantic_tokens_options: lsp::SemanticTokensOptions {
                                legend: lsp::SemanticTokensLegend {
                                    token_types: vec!["function".into()],
                                    token_modifiers: Vec::new(),
                                },
                                full: Some(lsp::SemanticTokensFullOptions::Bool(true)),
                                ..lsp::SemanticTokensOptions::default()
                            },
                            static_registration_options: lsp::StaticRegistrationOptions {
                                id: None,
                            },
                        })
                        .unwrap(),
                    ),
                }],
            },
            lsp::DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .expect("register capability request failed");

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert!(
        full_counter.load(atomic::Ordering::Acquire) >= 1,
        "dynamic registration should re-query semantic tokens for the open document"
    );

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)],
        "the open document should display semantic tokens after dynamic registration"
    );
}
async fn lsp_semantic_tokens_full_none_result_id(cx: &mut TestAppContext) {
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
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: Some(true) }),
                        ..lsp::SemanticTokensOptions::default()
                    },
                ),
            ),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let full_counter = Arc::new(AtomicUsize::new(0));
    let full_counter_clone = full_counter.clone();

    let mut full_request =
        cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(move |_, _, _| {
            full_counter_clone.fetch_add(1, atomic::Ordering::Release);
            async move {
                Ok(Some(lsp::SemanticTokensResult::Tokens(
                    lsp::SemanticTokens {
                        data: vec![
                            0, // delta_line
                            3, // delta_start
                            4, // length
                            0, // token_type
                            0, // token_modifiers_bitset
                        ],
                        result_id: None, // Sending back `None` forces the client to not use deltas.
                    },
                )))
            }
        });

    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());

    let task = cx.update_editor(|e, _, _| e.semantic_token_state.take_update_task());
    task.await;

    cx.set_state("ˇfn main() { a }");
    assert!(full_request.next().await.is_some());

    let task = cx.update_editor(|e, _, _| e.semantic_token_state.take_update_task());
    task.await;
    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)]
    );
    assert_eq!(full_counter.load(atomic::Ordering::Acquire), 2);
}
async fn lsp_semantic_tokens_delta(cx: &mut TestAppContext) {
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
                        full: Some(lsp::SemanticTokensFullOptions::Delta { delta: Some(true) }),
                        ..lsp::SemanticTokensOptions::default()
                    },
                ),
            ),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let full_counter = Arc::new(AtomicUsize::new(0));
    let full_counter_clone = full_counter.clone();
    let delta_counter = Arc::new(AtomicUsize::new(0));
    let delta_counter_clone = delta_counter.clone();

    let mut full_request =
        cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(move |_, _, _| {
            full_counter_clone.fetch_add(1, atomic::Ordering::Release);
            async move {
                Ok(Some(lsp::SemanticTokensResult::Tokens(
                    lsp::SemanticTokens {
                        data: vec![
                            0, // delta_line
                            3, // delta_start
                            4, // length
                            0, // token_type
                            0, // token_modifiers_bitset
                        ],
                        result_id: Some("a".into()),
                    },
                )))
            }
        });

    let mut delta_request = cx
        .set_request_handler::<lsp::request::SemanticTokensFullDeltaRequest, _, _>(
            move |_, params, _| {
                delta_counter_clone.fetch_add(1, atomic::Ordering::Release);
                assert_eq!(params.previous_result_id, "a");
                async move {
                    Ok(Some(lsp::SemanticTokensFullDeltaResult::TokensDelta(
                        lsp::SemanticTokensDelta {
                            edits: Vec::new(),
                            result_id: Some("b".into()),
                        },
                    )))
                }
            },
        );

    // Initial request, for the empty buffer.
    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());
    let task = cx.update_editor(|e, _, _| e.semantic_token_state.take_update_task());
    task.await;

    cx.set_state("ˇfn main() { a }");
    assert!(delta_request.next().await.is_some());
    let task = cx.update_editor(|e, _, _| e.semantic_token_state.take_update_task());
    task.await;

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)]
    );

    assert_eq!(full_counter.load(atomic::Ordering::Acquire), 1);
    assert_eq!(delta_counter.load(atomic::Ordering::Acquire), 1);
}
