use super::*;

#[gpui::test]
async fn test_signature_help_multiple_signatures(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

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

    cx.set_state(indoc! {"
        fn main() {
            overloadedˇ
        }
    "});

    cx.update_editor(|editor, window, cx| {
        editor.handle_input("(", window, cx);
        editor.show_signature_help(&ShowSignatureHelp, window, cx);
    });

    // Mock response with 3 signatures
    let mocked_response = lsp::SignatureHelp {
        signatures: vec![
            lsp::SignatureInformation {
                label: "fn overloaded(x: i32)".to_string(),
                documentation: None,
                parameters: Some(vec![lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("x: i32".to_string()),
                    documentation: None,
                }]),
                active_parameter: None,
            },
            lsp::SignatureInformation {
                label: "fn overloaded(x: i32, y: i32)".to_string(),
                documentation: None,
                parameters: Some(vec![
                    lsp::ParameterInformation {
                        label: lsp::ParameterLabel::Simple("x: i32".to_string()),
                        documentation: None,
                    },
                    lsp::ParameterInformation {
                        label: lsp::ParameterLabel::Simple("y: i32".to_string()),
                        documentation: None,
                    },
                ]),
                active_parameter: None,
            },
            lsp::SignatureInformation {
                label: "fn overloaded(x: i32, y: i32, z: i32)".to_string(),
                documentation: None,
                parameters: Some(vec![
                    lsp::ParameterInformation {
                        label: lsp::ParameterLabel::Simple("x: i32".to_string()),
                        documentation: None,
                    },
                    lsp::ParameterInformation {
                        label: lsp::ParameterLabel::Simple("y: i32".to_string()),
                        documentation: None,
                    },
                    lsp::ParameterInformation {
                        label: lsp::ParameterLabel::Simple("z: i32".to_string()),
                        documentation: None,
                    },
                ]),
                active_parameter: None,
            },
        ],
        active_signature: Some(1),
        active_parameter: Some(0),
    };
    handle_signature_help_request(&mut cx, mocked_response).await;

    cx.condition(|editor, _| editor.signature_help_state.is_shown())
        .await;

    // Verify we have multiple signatures and the right one is selected
    cx.editor(|editor, _, _| {
        let popover = editor.signature_help_state.popover().cloned().unwrap();
        assert_eq!(popover.signatures.len(), 3);
        // active_signature was 1, so that should be the current
        assert_eq!(popover.current_signature, 1);
        assert_eq!(popover.signatures[0].label, "fn overloaded(x: i32)");
        assert_eq!(popover.signatures[1].label, "fn overloaded(x: i32, y: i32)");
        assert_eq!(
            popover.signatures[2].label,
            "fn overloaded(x: i32, y: i32, z: i32)"
        );
    });

    // Test navigation functionality
    cx.update_editor(|editor, window, cx| {
        editor.signature_help_next(&crate::SignatureHelpNext, window, cx);
    });

    cx.editor(|editor, _, _| {
        let popover = editor.signature_help_state.popover().cloned().unwrap();
        assert_eq!(popover.current_signature, 2);
    });

    // Test wrap around
    cx.update_editor(|editor, window, cx| {
        editor.signature_help_next(&crate::SignatureHelpNext, window, cx);
    });

    cx.editor(|editor, _, _| {
        let popover = editor.signature_help_state.popover().cloned().unwrap();
        assert_eq!(popover.current_signature, 0);
    });

    // Test previous navigation
    cx.update_editor(|editor, window, cx| {
        editor.signature_help_prev(&crate::SignatureHelpPrevious, window, cx);
    });

    cx.editor(|editor, _, _| {
        let popover = editor.signature_help_state.popover().cloned().unwrap();
        assert_eq!(popover.current_signature, 2);
    });
}

#[gpui::test]
async fn test_completion_mode(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    struct Run {
        run_description: &'static str,
        initial_state: String,
        buffer_marked_text: String,
        completion_label: &'static str,
        completion_text: &'static str,
        expected_with_insert_mode: String,
        expected_with_replace_mode: String,
        expected_with_replace_subsequence_mode: String,
        expected_with_replace_suffix_mode: String,
    }

    let runs = [
        Run {
            run_description: "Start of word matches completion text",
            initial_state: "before ediˇ after".into(),
            buffer_marked_text: "before <edi|> after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇ after".into(),
            expected_with_replace_mode: "before editorˇ after".into(),
            expected_with_replace_subsequence_mode: "before editorˇ after".into(),
            expected_with_replace_suffix_mode: "before editorˇ after".into(),
        },
        Run {
            run_description: "Accept same text at the middle of the word",
            initial_state: "before ediˇtor after".into(),
            buffer_marked_text: "before <edi|tor> after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇtor after".into(),
            expected_with_replace_mode: "before editorˇ after".into(),
            expected_with_replace_subsequence_mode: "before editorˇ after".into(),
            expected_with_replace_suffix_mode: "before editorˇ after".into(),
        },
        Run {
            run_description: "End of word matches completion text -- cursor at end",
            initial_state: "before torˇ after".into(),
            buffer_marked_text: "before <tor|> after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇ after".into(),
            expected_with_replace_mode: "before editorˇ after".into(),
            expected_with_replace_subsequence_mode: "before editorˇ after".into(),
            expected_with_replace_suffix_mode: "before editorˇ after".into(),
        },
        Run {
            run_description: "End of word matches completion text -- cursor at start",
            initial_state: "before ˇtor after".into(),
            buffer_marked_text: "before <|tor> after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇtor after".into(),
            expected_with_replace_mode: "before editorˇ after".into(),
            expected_with_replace_subsequence_mode: "before editorˇ after".into(),
            expected_with_replace_suffix_mode: "before editorˇ after".into(),
        },
        Run {
            run_description: "Prepend text containing whitespace",
            initial_state: "pˇfield: bool".into(),
            buffer_marked_text: "<p|field>: bool".into(),
            completion_label: "pub ",
            completion_text: "pub ",
            expected_with_insert_mode: "pub ˇfield: bool".into(),
            expected_with_replace_mode: "pub ˇ: bool".into(),
            expected_with_replace_subsequence_mode: "pub ˇfield: bool".into(),
            expected_with_replace_suffix_mode: "pub ˇfield: bool".into(),
        },
        Run {
            run_description: "Add element to start of list",
            initial_state: "[element_ˇelement_2]".into(),
            buffer_marked_text: "[<element_|element_2>]".into(),
            completion_label: "element_1",
            completion_text: "element_1",
            expected_with_insert_mode: "[element_1ˇelement_2]".into(),
            expected_with_replace_mode: "[element_1ˇ]".into(),
            expected_with_replace_subsequence_mode: "[element_1ˇelement_2]".into(),
            expected_with_replace_suffix_mode: "[element_1ˇelement_2]".into(),
        },
        Run {
            run_description: "Add element to start of list -- first and second elements are equal",
            initial_state: "[elˇelement]".into(),
            buffer_marked_text: "[<el|element>]".into(),
            completion_label: "element",
            completion_text: "element",
            expected_with_insert_mode: "[elementˇelement]".into(),
            expected_with_replace_mode: "[elementˇ]".into(),
            expected_with_replace_subsequence_mode: "[elementˇelement]".into(),
            expected_with_replace_suffix_mode: "[elementˇ]".into(),
        },
        Run {
            run_description: "Ends with matching suffix",
            initial_state: "SubˇError".into(),
            buffer_marked_text: "<Sub|Error>".into(),
            completion_label: "SubscriptionError",
            completion_text: "SubscriptionError",
            expected_with_insert_mode: "SubscriptionErrorˇError".into(),
            expected_with_replace_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_subsequence_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_suffix_mode: "SubscriptionErrorˇ".into(),
        },
        Run {
            run_description: "Suffix is a subsequence -- contiguous",
            initial_state: "SubˇErr".into(),
            buffer_marked_text: "<Sub|Err>".into(),
            completion_label: "SubscriptionError",
            completion_text: "SubscriptionError",
            expected_with_insert_mode: "SubscriptionErrorˇErr".into(),
            expected_with_replace_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_subsequence_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_suffix_mode: "SubscriptionErrorˇErr".into(),
        },
        Run {
            run_description: "Suffix is a subsequence -- non-contiguous -- replace intended",
            initial_state: "Suˇscrirr".into(),
            buffer_marked_text: "<Su|scrirr>".into(),
            completion_label: "SubscriptionError",
            completion_text: "SubscriptionError",
            expected_with_insert_mode: "SubscriptionErrorˇscrirr".into(),
            expected_with_replace_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_subsequence_mode: "SubscriptionErrorˇ".into(),
            expected_with_replace_suffix_mode: "SubscriptionErrorˇscrirr".into(),
        },
        Run {
            run_description: "Suffix is a subsequence -- non-contiguous -- replace unintended",
            initial_state: "foo(indˇix)".into(),
            buffer_marked_text: "foo(<ind|ix>)".into(),
            completion_label: "node_index",
            completion_text: "node_index",
            expected_with_insert_mode: "foo(node_indexˇix)".into(),
            expected_with_replace_mode: "foo(node_indexˇ)".into(),
            expected_with_replace_subsequence_mode: "foo(node_indexˇix)".into(),
            expected_with_replace_suffix_mode: "foo(node_indexˇix)".into(),
        },
        Run {
            run_description: "Replace range ends before cursor - should extend to cursor",
            initial_state: "before editˇo after".into(),
            buffer_marked_text: "before <{ed}>it|o after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇo after".into(),
            expected_with_replace_mode: "before editorˇo after".into(),
            expected_with_replace_subsequence_mode: "before editorˇo after".into(),
            expected_with_replace_suffix_mode: "before editorˇo after".into(),
        },
        Run {
            run_description: "Uses label for suffix matching",
            initial_state: "before ediˇtor after".into(),
            buffer_marked_text: "before <edi|tor> after".into(),
            completion_label: "editor",
            completion_text: "editor()",
            expected_with_insert_mode: "before editor()ˇtor after".into(),
            expected_with_replace_mode: "before editor()ˇ after".into(),
            expected_with_replace_subsequence_mode: "before editor()ˇ after".into(),
            expected_with_replace_suffix_mode: "before editor()ˇ after".into(),
        },
        Run {
            run_description: "Case insensitive subsequence and suffix matching",
            initial_state: "before EDiˇtoR after".into(),
            buffer_marked_text: "before <EDi|toR> after".into(),
            completion_label: "editor",
            completion_text: "editor",
            expected_with_insert_mode: "before editorˇtoR after".into(),
            expected_with_replace_mode: "before editorˇ after".into(),
            expected_with_replace_subsequence_mode: "before editorˇ after".into(),
            expected_with_replace_suffix_mode: "before editorˇ after".into(),
        },
    ];

    for run in runs {
        let run_variations = [
            (LspInsertMode::Insert, run.expected_with_insert_mode),
            (LspInsertMode::Replace, run.expected_with_replace_mode),
            (
                LspInsertMode::ReplaceSubsequence,
                run.expected_with_replace_subsequence_mode,
            ),
            (
                LspInsertMode::ReplaceSuffix,
                run.expected_with_replace_suffix_mode,
            ),
        ];

        for (lsp_insert_mode, expected_text) in run_variations {
            eprintln!(
                "run = {:?}, mode = {lsp_insert_mode:.?}",
                run.run_description,
            );

            update_test_language_settings(&mut cx, &|settings| {
                settings.defaults.completions = Some(CompletionSettingsContent {
                    lsp_insert_mode: Some(lsp_insert_mode),
                    words: Some(WordsCompletionMode::Disabled),
                    words_min_length: Some(0),
                    ..Default::default()
                });
            });

            cx.set_state(&run.initial_state);

            // Set up resolve handler before showing completions, since resolve may be
            // triggered when menu becomes visible (for documentation), not just on confirm.
            cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(
                move |_, _, _| async move {
                    Ok(lsp::CompletionItem {
                        additional_text_edits: None,
                        ..Default::default()
                    })
                },
            );

            cx.update_editor(|editor, window, cx| {
                editor.show_completions(&ShowCompletions, window, cx);
            });

            let counter = Arc::new(AtomicUsize::new(0));
            handle_completion_request_with_insert_and_replace(
                &mut cx,
                &run.buffer_marked_text,
                vec![(run.completion_label, run.completion_text)],
                counter.clone(),
            )
            .await;
            cx.condition(|editor, _| editor.context_menu_visible())
                .await;
            assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

            let apply_additional_edits = cx.update_editor(|editor, window, cx| {
                editor
                    .confirm_completion(&ConfirmCompletion::default(), window, cx)
                    .unwrap()
            });
            cx.assert_editor_state(&expected_text);
            apply_additional_edits.await.unwrap();
        }
    }
}

#[gpui::test]
async fn test_completion_with_mode_specified_by_action(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    let initial_state = "SubˇError";
    let buffer_marked_text = "<Sub|Error>";
    let completion_text = "SubscriptionError";
    let expected_with_insert_mode = "SubscriptionErrorˇError";
    let expected_with_replace_mode = "SubscriptionErrorˇ";

    update_test_language_settings(&mut cx, &|settings| {
        settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Disabled),
            words_min_length: Some(0),
            // set the opposite here to ensure that the action is overriding the default behavior
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    cx.set_state(initial_state);
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });

    let counter = Arc::new(AtomicUsize::new(0));
    handle_completion_request_with_insert_and_replace(
        &mut cx,
        buffer_marked_text,
        vec![(completion_text, completion_text)],
        counter.clone(),
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion_replace(&ConfirmCompletionReplace, window, cx)
            .unwrap()
    });
    cx.assert_editor_state(expected_with_replace_mode);
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();

    update_test_language_settings(&mut cx, &|settings| {
        settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Disabled),
            words_min_length: Some(0),
            // set the opposite here to ensure that the action is overriding the default behavior
            lsp_insert_mode: Some(LspInsertMode::Replace),
            ..Default::default()
        });
    });

    cx.set_state(initial_state);
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    handle_completion_request_with_insert_and_replace(
        &mut cx,
        buffer_marked_text,
        vec![(completion_text, completion_text)],
        counter.clone(),
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 2);

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion_insert(&ConfirmCompletionInsert, window, cx)
            .unwrap()
    });
    cx.assert_editor_state(expected_with_insert_mode);
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();
}
