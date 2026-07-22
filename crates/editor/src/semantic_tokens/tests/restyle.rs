use super::*;

async fn test_semantic_tokens_rules_changes_restyle_tokens(cx: &mut TestAppContext) {
    use gpui::{Hsla, Rgba, UpdateGlobal as _};
    use settings::{GlobalLspSettingsContent, SemanticTokenRule};

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
                            token_types: Vec::from(["function".into()]),
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
                            0, // token_type (function)
                            0, // token_modifiers_bitset
                        ],
                        result_id: None,
                    },
                )))
            }
        });

    // Trigger initial semantic tokens fetch
    cx.set_state("ˇfn main() {}");
    full_request.next().await;
    cx.run_until_parked();

    // Verify initial highlights exist (with no custom color yet)
    let initial_ranges = extract_semantic_highlights(&cx.editor, &cx);
    assert_eq!(
        initial_ranges,
        vec![MultiBufferOffset(3)..MultiBufferOffset(7)],
        "Should have initial semantic token highlights"
    );
    let initial_styles = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(initial_styles.len(), 1, "Should have one highlight style");
    // Initial color should be None or theme default (not red or blue)
    let initial_color = initial_styles[0].color;

    // Set a custom foreground color for function tokens via settings.json
    let red_color = Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    semantic_token_rules: Some(SemanticTokenRules {
                        rules: Vec::from([SemanticTokenRule {
                            token_type: Some("function".to_string()),
                            foreground_color: Some(red_color),
                            ..SemanticTokenRule::default()
                        }]),
                    }),
                    ..GlobalLspSettingsContent::default()
                });
            });
        });
    });

    // Trigger a refetch by making an edit (which forces semantic tokens update)
    cx.set_state("ˇfn main() { }");
    full_request.next().await;
    cx.run_until_parked();

    // Verify the highlights now have the custom red color
    let styles_after_settings_change = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(
        styles_after_settings_change.len(),
        1,
        "Should still have one highlight"
    );
    assert_eq!(
        styles_after_settings_change[0].color,
        Some(Hsla::from(red_color)),
        "Highlight should have the custom red color from settings.json"
    );
    assert_ne!(
        styles_after_settings_change[0].color, initial_color,
        "Color should have changed from initial"
    );
}
async fn test_theme_override_changes_restyle_semantic_tokens(cx: &mut TestAppContext) {
    use collections::IndexMap;
    use gpui::{Hsla, Rgba, UpdateGlobal as _};
    use theme_settings::{HighlightStyleContent, ThemeStyleContent};

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
                            token_types: Vec::from(["function".into()]),
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
                        0, // token_type (function)
                        0, // token_modifiers_bitset
                    ],
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇfn main() {}");
    full_request.next().await;
    cx.run_until_parked();

    let initial_styles = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(initial_styles.len(), 1, "Should have one highlight style");
    let initial_color = initial_styles[0].color;

    // Changing experimental_theme_overrides triggers GlobalTheme reload,
    // which fires theme_changed → refresh_semantic_token_highlights.
    let red_color: Hsla = Rgba {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
    .into();
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.experimental_theme_overrides = Some(ThemeStyleContent {
                    syntax: IndexMap::from_iter([(
                        "function".to_string(),
                        HighlightStyleContent {
                            color: Some("#ff0000".to_string()),
                            background_color: None,
                            font_style: None,
                            font_weight: None,
                        },
                    )]),
                    ..ThemeStyleContent::default()
                });
            });
        });
    });

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let styles_after_override = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(styles_after_override.len(), 1);
    assert_eq!(
        styles_after_override[0].color,
        Some(red_color),
        "Highlight should have red color from theme override"
    );
    assert_ne!(
        styles_after_override[0].color, initial_color,
        "Color should have changed from initial"
    );

    // Changing the override to a different color also restyles.
    let blue_color: Hsla = Rgba {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    }
    .into();
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.experimental_theme_overrides = Some(ThemeStyleContent {
                    syntax: IndexMap::from_iter([(
                        "function".to_string(),
                        HighlightStyleContent {
                            color: Some("#0000ff".to_string()),
                            background_color: None,
                            font_style: None,
                            font_weight: None,
                        },
                    )]),
                    ..ThemeStyleContent::default()
                });
            });
        });
    });

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let styles_after_second_override = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(styles_after_second_override.len(), 1);
    assert_eq!(
        styles_after_second_override[0].color,
        Some(blue_color),
        "Highlight should have blue color from updated theme override"
    );

    // Removing overrides reverts to the original theme color.
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.experimental_theme_overrides = None;
            });
        });
    });

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let styles_after_clear = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(styles_after_clear.len(), 1);
    assert_eq!(
        styles_after_clear[0].color, initial_color,
        "Highlight should revert to initial color after clearing overrides"
    );
}
async fn test_per_theme_overrides_restyle_semantic_tokens(cx: &mut TestAppContext) {
    use collections::IndexMap;
    use gpui::{Hsla, Rgba, UpdateGlobal as _};
    use theme_settings::{HighlightStyleContent, ThemeStyleContent};
    use ui::ActiveTheme as _;

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
                            token_types: Vec::from(["function".into()]),
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
                        0, // token_type (function)
                        0, // token_modifiers_bitset
                    ],
                    result_id: None,
                },
            )))
        },
    );

    cx.set_state("ˇfn main() {}");
    full_request.next().await;
    cx.run_until_parked();

    let initial_styles = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(initial_styles.len(), 1, "Should have one highlight style");
    let initial_color = initial_styles[0].color;

    // Per-theme overrides (theme_overrides keyed by theme name) also go through
    // GlobalTheme reload → theme_changed → refresh_semantic_token_highlights.
    let theme_name = cx.update(|_, cx| cx.theme().name.to_string());
    let green_color: Hsla = Rgba {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    }
    .into();
    cx.update(|_, cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.theme.theme_overrides = collections::HashMap::from_iter([(
                    theme_name.clone(),
                    ThemeStyleContent {
                        syntax: IndexMap::from_iter([(
                            "function".to_string(),
                            HighlightStyleContent {
                                color: Some("#00ff00".to_string()),
                                background_color: None,
                                font_style: None,
                                font_weight: None,
                            },
                        )]),
                        ..ThemeStyleContent::default()
                    },
                )]);
            });
        });
    });

    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let styles_after_override = extract_semantic_highlight_styles(&cx.editor, &cx);
    assert_eq!(styles_after_override.len(), 1);
    assert_eq!(
        styles_after_override[0].color,
        Some(green_color),
        "Highlight should have green color from per-theme override"
    );
    assert_ne!(
        styles_after_override[0].color, initial_color,
        "Color should have changed from initial"
    );
}
