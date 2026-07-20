use super::*;

#[gpui::test]
async fn test_completion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
            ..Default::default()
        },
        cx,
    )
    .await;
    let counter = Arc::new(AtomicUsize::new(0));

    cx.set_state(indoc! {"
        oneˇ
        two
        three
    "});
    cx.simulate_keystroke(".");
    handle_completion_request(
        indoc! {"
            one.|<>
            two
            three
        "},
        vec!["first_completion", "second_completion"],
        true,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

    let _handler = handle_signature_help_request(
        &mut cx,
        lsp::SignatureHelp {
            signatures: vec![lsp::SignatureInformation {
                label: "test signature".to_string(),
                documentation: None,
                parameters: Some(vec![lsp::ParameterInformation {
                    label: lsp::ParameterLabel::Simple("foo: u8".to_string()),
                    documentation: None,
                }]),
                active_parameter: None,
            }],
            active_signature: None,
            active_parameter: None,
        },
    );
    cx.update_editor(|editor, window, cx| {
        assert!(
            !editor.signature_help_state.is_shown(),
            "No signature help was called for"
        );
        editor.show_signature_help(&ShowSignatureHelp, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.signature_help_state.is_shown(),
            "No signature help should be shown when completions menu is open"
        );
    });

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&Default::default(), window, cx);
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state(indoc! {"
        one.second_completionˇ
        two
        three
    "});

    handle_resolve_completion_request(
        &mut cx,
        Some(vec![
            (
                //This overlaps with the primary completion edit which is
                //misbehavior from the LSP spec, test that we filter it out
                indoc! {"
                    one.second_ˇcompletion
                    two
                    threeˇ
                "},
                "overlapping additional edit",
            ),
            (
                indoc! {"
                    one.second_completion
                    two
                    threeˇ
                "},
                "\nadditional edit",
            ),
        ]),
    )
    .await;
    apply_additional_edits.await.unwrap();
    cx.assert_editor_state(indoc! {"
        one.second_completionˇ
        two
        three
        additional edit
    "});

    cx.set_state(indoc! {"
        one.second_completion
        twoˇ
        threeˇ
        additional edit
    "});
    cx.simulate_keystroke(" ");
    assert!(cx.editor(|e, _, _| e.context_menu.borrow_mut().is_none()));
    cx.simulate_keystroke("s");
    assert!(cx.editor(|e, _, _| e.context_menu.borrow_mut().is_none()));

    cx.assert_editor_state(indoc! {"
        one.second_completion
        two sˇ
        three sˇ
        additional edit
    "});
    handle_completion_request(
        indoc! {"
            one.second_completion
            two s
            three <s|>
            additional edit
        "},
        vec!["fourth_completion", "fifth_completion", "sixth_completion"],
        true,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 2);

    cx.simulate_keystroke("i");

    handle_completion_request(
        indoc! {"
            one.second_completion
            two si
            three <si|>
            additional edit
        "},
        vec!["fourth_completion", "fifth_completion", "sixth_completion"],
        true,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 3);

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state(indoc! {"
        one.second_completion
        two sixth_completionˇ
        three sixth_completionˇ
        additional edit
    "});

    apply_additional_edits.await.unwrap();

    update_test_language_settings(&mut cx, &|settings| {
        settings.defaults.show_completions_on_input = Some(false);
    });
    cx.set_state("editorˇ");
    cx.simulate_keystroke(".");
    assert!(cx.editor(|e, _, _| e.context_menu.borrow_mut().is_none()));
    cx.simulate_keystrokes("c l o");
    cx.assert_editor_state("editor.cloˇ");
    assert!(cx.editor(|e, _, _| e.context_menu.borrow_mut().is_none()));
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    handle_completion_request(
        "editor.<clo|>",
        vec!["close", "clobber"],
        true,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 4);

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state("editor.clobberˇ");
    handle_resolve_completion_request(&mut cx, None).await;
    apply_additional_edits.await.unwrap();
}
