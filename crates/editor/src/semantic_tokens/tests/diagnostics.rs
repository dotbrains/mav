use super::*;

#[gpui::test]
async fn test_diagnostics_visible_when_semantic_token_set_to_full(cx: &mut TestAppContext) {
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

    let mut full_request =
        cx.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(move |_, _, _| {
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

    cx.set_state("ˇfn main() {}");
    assert!(full_request.next().await.is_some());

    let task = cx.update_editor(|e, _, _| e.semantic_token_state.take_update_task());
    task.await;

    cx.update_buffer(|buffer, cx| {
        buffer.update_diagnostics(
            LanguageServerId(0),
            DiagnosticSet::new(
                [DiagnosticEntry {
                    range: PointUtf16::new(0, 3)..PointUtf16::new(0, 7),
                    diagnostic: Diagnostic {
                        severity: lsp::DiagnosticSeverity::ERROR,
                        group_id: 1,
                        message: "unused function".into(),
                        ..Default::default()
                    },
                }],
                buffer,
            ),
            cx,
        )
    });

    cx.run_until_parked();
    let chunks = cx.update_editor(|editor, window, cx| {
        editor
            .snapshot(window, cx)
            .display_snapshot
            .chunks(
                crate::display_map::DisplayRow(0)..crate::display_map::DisplayRow(1),
                LanguageAwareStyling {
                    tree_sitter: false,
                    diagnostics: true,
                },
                crate::HighlightStyles::default(),
            )
            .map(|chunk| {
                (
                    chunk.text.to_string(),
                    chunk.diagnostic_severity,
                    chunk.highlight_style,
                )
            })
            .collect::<Vec<_>>()
    });

    assert_eq!(
        extract_semantic_highlights(&cx.editor, &cx),
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)]
    );

    assert!(
        chunks.iter().any(
            |(text, severity, style): &(
                String,
                Option<lsp::DiagnosticSeverity>,
                Option<gpui::HighlightStyle>
            )| {
                text == "main"
                    && *severity == Some(lsp::DiagnosticSeverity::ERROR)
                    && style.is_some()
            }
        ),
        "expected 'main' chunk to have both diagnostic and semantic styling: {:?}",
        chunks
    );
}
