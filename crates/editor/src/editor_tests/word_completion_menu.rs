use super::*;

#[gpui::test]
async fn test_word_completion(cx: &mut TestAppContext) {
    let lsp_fetch_timeout_ms = 10;
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words_min_length: Some(0),
            lsp_fetch_timeout_ms: Some(10),
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

    let throttle_completions = Arc::new(AtomicBool::new(false));

    let lsp_throttle_completions = throttle_completions.clone();
    let _completion_requests_handler =
        cx.lsp
            .server
            .on_request::<lsp::request::Completion, _, _>(move |_, cx| {
                let lsp_throttle_completions = lsp_throttle_completions.clone();
                let cx = cx.clone();
                async move {
                    if lsp_throttle_completions.load(atomic::Ordering::Acquire) {
                        cx.background_executor()
                            .timer(Duration::from_millis(lsp_fetch_timeout_ms * 10))
                            .await;
                    }
                    Ok(Some(lsp::CompletionResponse::Array(vec![
                        lsp::CompletionItem {
                            label: "first".into(),
                            ..lsp::CompletionItem::default()
                        },
                        lsp::CompletionItem {
                            label: "last".into(),
                            ..lsp::CompletionItem::default()
                        },
                    ])))
                }
            });

    cx.set_state(indoc! {"
        oneˇ
        two
        three
    "});
    cx.simulate_keystroke(".");
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, window, cx| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["first", "last"],
                "When LSP server is fast to reply, no fallback word completions are used"
            );
        } else {
            panic!("expected completion menu to be open");
        }
        editor.cancel(&Cancel, window, cx);
    });
    cx.executor().run_until_parked();
    cx.condition(|editor, _| !editor.context_menu_visible())
        .await;

    throttle_completions.store(true, atomic::Ordering::Release);
    cx.simulate_keystroke(".");
    cx.executor()
        .advance_clock(Duration::from_millis(lsp_fetch_timeout_ms * 2));
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["one", "three", "two"],
                "When LSP server is slow, document words can be shown instead, if configured accordingly");
        } else {
            panic!("expected completion menu to be open");
        }
    });
}

#[gpui::test]
async fn test_word_completions_do_not_duplicate_lsp_ones(cx: &mut TestAppContext) {
    init_test(cx, |language_settings| {
        language_settings.defaults.completions = Some(CompletionSettingsContent {
            words: Some(WordsCompletionMode::Enabled),
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
                Ok(Some(lsp::CompletionResponse::Array(vec![
                    lsp::CompletionItem {
                        label: "first".into(),
                        ..lsp::CompletionItem::default()
                    },
                    lsp::CompletionItem {
                        label: "last".into(),
                        ..lsp::CompletionItem::default()
                    },
                ])))
            });

    cx.set_state(indoc! {"ˇ
        first
        last
        second
    "});
    cx.simulate_keystroke(".");
    cx.executor().run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["first", "last", "second"],
                "Word completions that has the same edit as the any of the LSP ones, should not be proposed"
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });
}
