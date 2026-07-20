use super::*;

#[gpui::test]
async fn test_handle_input_for_show_signature_help_auto_signature_help_true(
    cx: &mut TestAppContext,
) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(true);
                settings.editor.hover_popover_delay = Some(DelayMs(300));
            });
        });
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            signature_help_provider: Some(lsp::SignatureHelpOptions {
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    let language = Language::new(
        LanguageConfig {
            name: "Rust".into(),
            brackets: BracketPairConfig {
                pairs: vec![
                    BracketPair {
                        start: "{".to_string(),
                        end: "}".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "(".to_string(),
                        end: ")".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "/*".to_string(),
                        end: " */".to_string(),
                        close: true,
                        surround: true,
                        newline: true,
                    },
                    BracketPair {
                        start: "[".to_string(),
                        end: "]".to_string(),
                        close: false,
                        surround: false,
                        newline: true,
                    },
                    BracketPair {
                        start: "\"".to_string(),
                        end: "\"".to_string(),
                        close: true,
                        surround: true,
                        newline: false,
                    },
                    BracketPair {
                        start: "<".to_string(),
                        end: ">".to_string(),
                        close: false,
                        surround: true,
                        newline: true,
                    },
                ],
                ..Default::default()
            },
            autoclose_before: "})]".to_string(),
            ..Default::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    );
    let language = Arc::new(language);

    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    cx.set_state(
        &r#"
            fn main() {
                sampleˇ
            }
        "#
        .unindent(),
    );

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("(", window, cx);
    });
    cx.assert_editor_state(
        &"
            fn main() {
                sample(ˇ)
            }
        "
        .unindent(),
    );

    let mocked_response = lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn sample(param1: u8, param2: u8)".to_string(),
            documentation: None,
            parameters: Some(vec![
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("param1: u8".to_string()),
                    documentation: None,
                },
                lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("param2: u8".to_string()),
                    documentation: None,
                },
            ]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    };
    handle_signature_help_request(&mut cx, mocked_response).await;

    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;

    cx.editor(|editor, _, _| {
        let signature_help_state = editor.signature_help_state.popover().cloned();
        let signature = signature_help_state.unwrap();
        assert_eq!(
            signature.signatures[signature.current_signature].label,
            "fn sample(param1: u8, param2: u8)"
        );
    });
}

#[gpui::test]
async fn test_signature_help_delay_only_for_auto(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let delay_ms = 500;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(true);
                settings.editor.show_signature_help_after_edits = Some(false);
                settings.editor.hover_popover_delay = Some(DelayMs(delay_ms));
            });
        });
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mocked_response = lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn sample(param1: u8)".to_string(),
            documentation: None,
            parameters: Some(vec![lsp::ParameterInformation {
                label: lsp::ParameterLabel::Simple("param1: u8".to_string()),
                documentation: None,
            }]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    };

    cx.set_state(indoc! {"
        fn main() {
            sample(ˇ);
        }

        fn sample(param1: u8) {}
    "});

    // Manual trigger should show immediately without delay
    cx.update_editor(|editor, window, cx| {
        editor.show_signature_help(&ShowSignatureHelp, window, cx);
    });
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            editor.signature_help_state.is_shown(),
            "Manual trigger should show signature help without delay"
        );
    });

    cx.update_editor(|editor, _, cx| {
        editor.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
    });
    cx.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(!editor.signature_help_state.is_shown());
    });

    // Auto trigger (cursor movement into brackets) should respect delay
    cx.set_state(indoc! {"
        fn main() {
            sampleˇ();
        }

        fn sample(param1: u8) {}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.move_right(&MoveRight, window, cx);
    });
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            !editor.signature_help_state.is_shown(),
            "Auto trigger should wait for delay before showing signature help"
        );
    });

    cx.executor()
        .advance_clock(Duration::from_millis(delay_ms + 50));
    cx.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            editor.signature_help_state.is_shown(),
            "Auto trigger should show signature help after delay elapsed"
        );
    });
}

#[gpui::test]
async fn test_signature_help_after_edits_no_delay(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let delay_ms = 500;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(false);
                settings.editor.show_signature_help_after_edits = Some(true);
                settings.editor.hover_popover_delay = Some(DelayMs(delay_ms));
            });
        });
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "(".to_string(),
                    end: ")".to_string(),
                    close: true,
                    surround: true,
                    newline: true,
                }],
                ..BracketPairConfig::default()
            },
            autoclose_before: "})".to_string(),
            ..LanguageConfig::default()
        },
        Some(tree_sitter_rust::LANGUAGE.into()),
    ));
    cx.language_registry().add(language.clone());
    cx.update_buffer(|buffer, cx| {
        buffer.set_language(Some(language), cx);
    });

    let mocked_response = lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn sample(param1: u8)".to_string(),
            documentation: None,
            parameters: Some(vec![lsp::ParameterInformation {
                label: lsp::ParameterLabel::Simple("param1: u8".to_string()),
                documentation: None,
            }]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    };

    cx.set_state(indoc! {"
        fn main() {
            sampleˇ
        }
    "});

    // Typing bracket should show signature help immediately without delay
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("(", window, cx);
    });
    handle_signature_help_request(&mut cx, mocked_response).await;
    cx.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            editor.signature_help_state.is_shown(),
            "show_signature_help_after_edits should show signature help without delay"
        );
    });
}
