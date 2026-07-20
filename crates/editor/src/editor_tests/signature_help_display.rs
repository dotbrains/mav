use super::*;

#[gpui::test]
async fn test_handle_input_with_different_show_signature_settings(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(false);
                settings.editor.show_signature_help_after_edits = Some(false);
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

    // Ensure that signature_help is not called when no signature help is enabled.
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
    cx.editor(|editor, _, _| {
        assert!(editor.signature_help_state.task().is_none());
    });

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

    // Ensure that signature_help is called when enabled afte edits
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(false);
                settings.editor.show_signature_help_after_edits = Some(true);
            });
        });
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
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;
    cx.update_editor(|editor, _, _| {
        let signature_help_state = editor.signature_help_state.popover().cloned();
        assert!(signature_help_state.is_some());
        let signature = signature_help_state.unwrap();
        assert_eq!(
            signature.signatures[signature.current_signature].label,
            "fn sample(param1: u8, param2: u8)"
        );
        editor.signature_help_state = SignatureHelpState::default();
    });

    // Ensure that signature_help is called when auto signature help override is enabled
    cx.update(|_, cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(true);
                settings.editor.show_signature_help_after_edits = Some(false);
            });
        });
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
    handle_signature_help_request(&mut cx, mocked_response).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;
    cx.editor(|editor, _, _| {
        let signature_help_state = editor.signature_help_state.popover().cloned();
        assert!(signature_help_state.is_some());
        let signature = signature_help_state.unwrap();
        assert_eq!(
            signature.signatures[signature.current_signature].label,
            "fn sample(param1: u8, param2: u8)"
        );
    });
}

#[gpui::test]
async fn test_signature_help(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.auto_signature_help = Some(true);
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

    // A test that directly calls `show_signature_help`
    cx.update_editor(|editor, window, cx| {
        editor.show_signature_help(&ShowSignatureHelp, window, cx);
    });

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
        assert!(signature_help_state.is_some());
        let signature = signature_help_state.unwrap();
        assert_eq!(
            signature.signatures[signature.current_signature].label,
            "fn sample(param1: u8, param2: u8)"
        );
    });

    // When exiting outside from inside the brackets, `signature_help` is closed.
    cx.set_state(indoc! {"
        fn main() {
            sample(ˇ);
        }

        fn sample(param1: u8, param2: u8) {}
    "});

    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
        });
    });

    let mocked_response = lsp::SignatureHelp {
        signatures: Vec::new(),
        active_signature: None,
        active_parameter: None,
    };
    handle_signature_help_request(&mut cx, mocked_response).await;

    cx.condition(|editor, _| !editor.signature_help_state.is_shown())
        .await;

    cx.editor(|editor, _, _| {
        assert!(!editor.signature_help_state.is_shown());
    });

    // When entering inside the brackets from outside, `show_signature_help` is automatically called.
    cx.set_state(indoc! {"
        fn main() {
            sample(ˇ);
        }

        fn sample(param1: u8, param2: u8) {}
    "});

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
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;
    cx.editor(|editor, _, _| {
        assert!(editor.signature_help_state.is_shown());
    });

    // Restore the popover with more parameter input
    cx.set_state(indoc! {"
        fn main() {
            sample(param1, param2ˇ);
        }

        fn sample(param1: u8, param2: u8) {}
    "});

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
        active_parameter: Some(1),
    };
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;

    // When selecting a range, the popover is gone.
    // Avoid using `cx.set_state` to not actually edit the document, just change its selections.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(1, 25)..Point::new(1, 19)));
        })
    });
    cx.assert_editor_state(indoc! {"
        fn main() {
            sample(param1, «ˇparam2»);
        }

        fn sample(param1: u8, param2: u8) {}
    "});
    cx.editor(|editor, _, _| {
        assert!(!editor.signature_help_state.is_shown());
    });

    // When unselecting again, the popover is back if within the brackets.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(1, 19)..Point::new(1, 19)));
        })
    });
    cx.assert_editor_state(indoc! {"
        fn main() {
            sample(param1, ˇparam2);
        }

        fn sample(param1: u8, param2: u8) {}
    "});
    handle_signature_help_request(&mut cx, mocked_response).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;
    cx.editor(|editor, _, _| {
        assert!(editor.signature_help_state.is_shown());
    });

    // Test to confirm that SignatureHelp does not appear after deselecting multiple ranges when it was hidden by pressing Escape.
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(0, 0)..Point::new(0, 0)));
            s.select_ranges(Some(Point::new(1, 19)..Point::new(1, 19)));
        })
    });
    cx.assert_editor_state(indoc! {"
        fn main() {
            sample(param1, ˇparam2);
        }

        fn sample(param1: u8, param2: u8) {}
    "});

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
        active_parameter: Some(1),
    };
    handle_signature_help_request(&mut cx, mocked_response.clone()).await;
    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;
    cx.update_editor(|editor, _, cx| {
        editor.hide_signature_help(cx, SignatureHelpHiddenBy::Escape);
    });
    cx.condition(|editor, _| !editor.signature_help_state.is_shown())
        .await;
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(1, 25)..Point::new(1, 19)));
        })
    });
    cx.assert_editor_state(indoc! {"
        fn main() {
            sample(param1, «ˇparam2»);
        }

        fn sample(param1: u8, param2: u8) {}
    "});
    cx.update_editor(|editor, window, cx| {
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
            s.select_ranges(Some(Point::new(1, 19)..Point::new(1, 19)));
        })
    });
    cx.assert_editor_state(indoc! {"
        fn main() {
            sample(param1, ˇparam2);
        }

        fn sample(param1: u8, param2: u8) {}
    "});
    cx.condition(|editor, _| !editor.signature_help_state.is_shown()) // because hidden by escape
        .await;
}
