use super::*;

#[gpui::test]
async fn test_word_completions_continue_on_typing(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Disabled),
            words_min_length: Some(0),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                ..lsp::CompletionOptions::default()
            }),
            signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let _completion_requests_handler =
        cx.lsp
            .server
            .on_request::<lsp::request::Completion, _, _>(move |_, _| async move {
                panic!("LSP completions should not be queried when dealing with word completions")
            });

    cx.set_state(indoc! {"ˇ
        first
        last
        second
    "});
    cx.update_editor(|editor, window, cx| {
        editor.show_word_completions(&ShowWordCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["first", "last", "second"],
                "`ShowWordCompletions` action should show word completions"
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });

    cx.simulate_keystroke("l");
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["last"],
                "After showing word completions, further editing should filter them and not query the LSP"
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
async fn test_completions_use_selection_head(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Disabled),
            words_min_length: Some(0),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions::default()),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let _completion_requests_handler =
        cx.lsp
            .server
            .on_request::<lsp::request::Completion, _, _>(move |_, _| async move {
                panic!("LSP completions should not be queried when dealing with word completions")
            });

    cx.set_state(indoc! {"«applˇ»
        applepie
        banana
        cherry
    "});
    cx.update_editor(|editor, window, cx| {
        editor.show_word_completions(&ShowWordCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["applepie"],
                "Completion query should use the selection head (`appl`), filtering to words with that prefix"
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
async fn test_word_completions_usually_skip_digits(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words_min_length: Some(0),
            lsp: Some(false),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;

    cx.set_state(indoc! {"ˇ
        0_usize
        let
        33
        4.5f32
    "});
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, window, cx| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["let"],
                "With no digits in the completion query, no digits should be in the word completions"
            );
        } else {
            panic!("expected completion menu to be open");
        }
        editor.cancel(&Cancel, window, cx);
    });

    cx.set_state(indoc! {"3ˇ
        0_usize
        let
        3
        33.35f32
    "});
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["33", "35f32"], "The digit is in the completion query, \
                return matching words with digits (`33`, `35f32`) but exclude query duplicates (`3`)");
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
async fn test_word_completions_do_not_show_before_threshold(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Enabled),
            words_min_length: Some(3),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    cx.set_state(indoc! {"ˇ
        wow
        wowen
        wowser
    "});
    cx.simulate_keystroke("w");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!(
                "expected completion menu to be hidden, as words completion threshold is not met"
            );
        }
    });

    cx.update_editor(|editor, window, cx| {
        editor.show_word_completions(&ShowWordCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, window, cx| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["wowser", "wowen", "wow"], "Even though the threshold is not met, invoking word completions with an action should provide the completions");
        } else {
            panic!("expected completion menu to be open after the word completions are called with an action");
        }

        editor.cancel(&Cancel, window, cx);
    });
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!("expected completion menu to be hidden after canceling");
        }
    });

    cx.simulate_keystroke("o");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!(
                "expected completion menu to be hidden, as words completion threshold is not met still"
            );
        }
    });

    cx.simulate_keystroke("w");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["wowen", "wowser"], "After word completion threshold is met, matching words should be shown, excluding the already typed word");
        } else {
            panic!("expected completion menu to be open after the word completions threshold is met");
        }
    });
}

#[gpui::test]
async fn test_word_completions_disabled(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Enabled),
            words_min_length: Some(0),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    cx.update_editor(|editor, _, _| {
        editor.disable_word_completions();
    });
    cx.set_state(indoc! {"ˇ
        wow
        wowen
        wowser
    "});
    cx.simulate_keystroke("w");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!(
                "expected completion menu to be hidden, as words completion are disabled for this editor"
            );
        }
    });

    cx.update_editor(|editor, window, cx| {
        editor.show_word_completions(&ShowWordCompletions, window, cx);
    });
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!(
                "expected completion menu to be hidden even if called for explicitly, as words completion are disabled for this editor"
            );
        }
    });
}

#[gpui::test]
async fn test_word_completions_disabled_with_no_provider(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Disabled),
            words_min_length: Some(0),
            lsp_insert_mode: Some(LspInsertMode::Insert),
            ..Default::default()
        });
    });

    let mut cx = EditorLspTestContext::new_rust(lsp::ServerCapabilities::default(), cx).await;
    cx.update_editor(|editor, _, _| {
        editor.set_completion_provider(None);
    });
    cx.set_state(indoc! {"ˇ
        wow
        wowen
        wowser
    "});
    cx.simulate_keystroke("w");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if editor.context_menu.borrow_mut().is_some() {
            panic!("expected completion menu to be hidden, as disabled in settings");
        }
    });
}
