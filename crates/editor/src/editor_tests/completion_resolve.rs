use super::*;

#[gpui::test]
async fn test_completions_resolve_updates_labels_if_filter_text_matches(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");

    let item1 = lsp::CompletionItem {
        label: "method id()".to_string(),
        filter_text: Some("id".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".id".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };

    let item2 = lsp::CompletionItem {
        label: "other".to_string(),
        filter_text: Some("other".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".other".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };

    let item1 = item1.clone();
    cx.set_request_handler::<lsp::request::Completion, _, _>({
        let item1 = item1.clone();
        move |_, _, _| {
            let item1 = item1.clone();
            let item2 = item2.clone();
            async move { Ok(Some(lsp::CompletionResponse::Array(vec![item1, item2]))) }
        }
    })
    .next()
    .await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["method id()", "other"]
                )
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>({
        let item1 = item1.clone();
        move |_, item_to_resolve, _| {
            let item1 = item1.clone();
            async move {
                if item1 == item_to_resolve {
                    Ok(lsp::CompletionItem {
                        label: "method id()".to_string(),
                        filter_text: Some("id".to_string()),
                        detail: Some("Now resolved!".to_string()),
                        documentation: Some(lsp::Documentation::String("Docs".to_string())),
                        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                            range: lsp::Range::new(
                                lsp::Position::new(0, 22),
                                lsp::Position::new(0, 22),
                            ),
                            new_text: ".id".to_string(),
                        })),
                        ..lsp::CompletionItem::default()
                    })
                } else {
                    Ok(item_to_resolve)
                }
            }
        }
    })
    .next()
    .await
    .unwrap();
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&Default::default(), window, cx);
    });
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["method id() Now resolved!", "other"],
                    "Should update first completion label, but not second as the filter text did not match."
                );
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });
}

#[gpui::test]
async fn test_context_menus_hide_hover_popover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
            completion_provider: Some(lsp::CompletionOptions {
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    cx.set_state(indoc! {"
        struct TestStruct {
            field: i32
        }

        fn mainˇ() {
            let unused_var = 42;
            let test_struct = TestStruct { field: 42 };
        }
    "});
    let symbol_range = cx.lsp_range(indoc! {"
        struct TestStruct {
            field: i32
        }

        «fn main»() {
            let unused_var = 42;
            let test_struct = TestStruct { field: 42 };
        }
    "});
    let mut hover_requests =
        cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: "Function documentation".to_string(),
                }),
                range: Some(symbol_range),
            }))
        });

    // Case 1: Test that code action menu hide hover popover
    cx.dispatch_action(Hover);
    hover_requests.next().await;
    cx.condition(|editor, _| editor.hover_state.visible()).await;
    let mut code_action_requests = cx.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |_, _, _| async move {
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                lsp::CodeAction {
                    title: "Remove unused variable".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(lsp::WorkspaceEdit {
                        changes: Some(
                            [(
                                lsp::Uri::from_file_path(path!("/file.rs")).unwrap(),
                                vec![lsp::TextEdit {
                                    range: lsp::Range::new(
                                        lsp::Position::new(5, 4),
                                        lsp::Position::new(5, 27),
                                    ),
                                    new_text: "".to_string(),
                                }],
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]))
        },
    );
    cx.update_editor(|editor, window, cx| {
        editor.toggle_code_actions(
            &ToggleCodeActions {
                deployed_from: None,
                quick_launch: false,
            },
            window,
            cx,
        );
    });
    code_action_requests.next().await;
    cx.run_until_parked();
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Hover popover should be hidden when code action menu is shown"
        );
        // Hide code actions
        editor.context_menu.take();
    });

    // Case 2: Test that code completions hide hover popover
    cx.dispatch_action(Hover);
    hover_requests.next().await;
    cx.condition(|editor, _| editor.hover_state.visible()).await;
    let counter = Arc::new(AtomicUsize::new(0));
    let mut completion_requests =
        cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, atomic::Ordering::Release);
                Ok(Some(lsp::CompletionResponse::Array(vec![
                    lsp::CompletionItem {
                        label: "main".into(),
                        kind: Some(lsp::CompletionItemKind::FUNCTION),
                        detail: Some("() -> ()".to_string()),
                        ..Default::default()
                    },
                    lsp::CompletionItem {
                        label: "TestStruct".into(),
                        kind: Some(lsp::CompletionItemKind::STRUCT),
                        detail: Some("struct TestStruct".to_string()),
                        ..Default::default()
                    },
                ])))
            }
        });
    cx.update_editor(|editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    completion_requests.next().await;
    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Hover popover should be hidden when completion menu is shown"
        );
    });
}

#[gpui::test]
async fn test_completions_resolve_happens_once(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state("fn main() { let a = 2ˇ; }");
    cx.simulate_keystroke(".");

    let unresolved_item_1 = lsp::CompletionItem {
        label: "id".to_string(),
        filter_text: Some("id".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".id".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };
    let resolved_item_1 = lsp::CompletionItem {
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 20), lsp::Position::new(0, 22)),
            new_text: "!!".to_string(),
        }]),
        ..unresolved_item_1.clone()
    };
    let unresolved_item_2 = lsp::CompletionItem {
        label: "other".to_string(),
        filter_text: Some("other".to_string()),
        detail: None,
        documentation: None,
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 22), lsp::Position::new(0, 22)),
            new_text: ".other".to_string(),
        })),
        ..lsp::CompletionItem::default()
    };
    let resolved_item_2 = lsp::CompletionItem {
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range::new(lsp::Position::new(0, 20), lsp::Position::new(0, 22)),
            new_text: "??".to_string(),
        }]),
        ..unresolved_item_2.clone()
    };

    let resolve_requests_1 = Arc::new(AtomicUsize::new(0));
    let resolve_requests_2 = Arc::new(AtomicUsize::new(0));
    cx.lsp
        .server
        .on_request::<lsp::request::ResolveCompletionItem, _, _>({
            let unresolved_item_1 = unresolved_item_1.clone();
            let resolved_item_1 = resolved_item_1.clone();
            let unresolved_item_2 = unresolved_item_2.clone();
            let resolved_item_2 = resolved_item_2.clone();
            let resolve_requests_1 = resolve_requests_1.clone();
            let resolve_requests_2 = resolve_requests_2.clone();
            move |unresolved_request, _| {
                let unresolved_item_1 = unresolved_item_1.clone();
                let resolved_item_1 = resolved_item_1.clone();
                let unresolved_item_2 = unresolved_item_2.clone();
                let resolved_item_2 = resolved_item_2.clone();
                let resolve_requests_1 = resolve_requests_1.clone();
                let resolve_requests_2 = resolve_requests_2.clone();
                async move {
                    if unresolved_request == unresolved_item_1 {
                        resolve_requests_1.fetch_add(1, atomic::Ordering::Release);
                        Ok(resolved_item_1.clone())
                    } else if unresolved_request == unresolved_item_2 {
                        resolve_requests_2.fetch_add(1, atomic::Ordering::Release);
                        Ok(resolved_item_2.clone())
                    } else {
                        panic!("Unexpected completion item {unresolved_request:?}")
                    }
                }
            }
        })
        .detach();

    cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let unresolved_item_1 = unresolved_item_1.clone();
        let unresolved_item_2 = unresolved_item_2.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                unresolved_item_1,
                unresolved_item_2,
            ])))
        }
    })
    .next()
    .await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.update_editor(|editor, _, _| {
        let context_menu = editor.context_menu.borrow_mut();
        let context_menu = context_menu
            .as_ref()
            .expect("Should have the context menu deployed");
        match context_menu {
            CodeContextMenu::Completions(completions_menu) => {
                let completions = completions_menu.completions.borrow_mut();
                assert_eq!(
                    completions
                        .iter()
                        .map(|completion| &completion.label.text)
                        .collect::<Vec<_>>(),
                    vec!["id", "other"]
                )
            }
            CodeContextMenu::CodeActions(_) => panic!("Should show the completions menu"),
        }
    });
    cx.run_until_parked();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&ContextMenuNext, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.context_menu_prev(&ContextMenuPrevious, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&ContextMenuNext, window, cx);
    });
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor
            .compose_completion(&ComposeCompletion::default(), window, cx)
            .expect("No task returned")
    })
    .await
    .expect("Completion failed");
    cx.run_until_parked();

    cx.update_editor(|editor, _, cx| {
        assert_eq!(
            resolve_requests_1.load(atomic::Ordering::Acquire),
            1,
            "Should always resolve once despite multiple selections"
        );
        assert_eq!(
            resolve_requests_2.load(atomic::Ordering::Acquire),
            1,
            "Should always resolve once after multiple selections and applying the completion"
        );
        assert_eq!(
            editor.text(cx),
            "fn main() { let a = ??.other; }",
            "Should use resolved data when applying the completion"
        );
    });
}
