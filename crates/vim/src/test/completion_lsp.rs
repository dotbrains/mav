use super::*;

#[perf]
#[gpui::test]
async fn test_completion_menu_scroll_aside(cx: &mut TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "Test Item".to_string(),
                    documentation: Some(lsp::Documentation::String(
                        "This is some very long documentation content that will be displayed in the aside panel for scrolling.\n".repeat(50)
                    )),
                    ..Default::default()
                },
            ])))
        });

    cx.set_state("variableˇ", Mode::Insert);
    cx.simulate_keystroke(".");
    cx.executor().run_until_parked();

    let mut initial_offset: Pixels = px(0.0);

    cx.update_editor(|editor, _, _| {
        let binding = editor.context_menu().borrow();
        let Some(CodeContextMenu::Completions(menu)) = binding.as_ref() else {
            panic!("Should have completions menu open");
        };

        initial_offset = menu.scroll_handle_aside.offset().y;
    });

    // The `ctrl-e` shortcut should scroll the completion menu's aside content
    // down, so the updated offset should be lower than the initial offset.
    cx.simulate_keystroke("ctrl-e");
    cx.update_editor(|editor, _, _| {
        let binding = editor.context_menu().borrow();
        let Some(CodeContextMenu::Completions(menu)) = binding.as_ref() else {
            panic!("Should have completions menu open");
        };

        assert!(menu.scroll_handle_aside.offset().y < initial_offset);
    });

    // The `ctrl-y` shortcut should do the inverse scrolling as `ctrl-e`, so the
    // offset should now be the same as the initial offset.
    cx.simulate_keystroke("ctrl-y");
    cx.update_editor(|editor, _, _| {
        let binding = editor.context_menu().borrow();
        let Some(CodeContextMenu::Completions(menu)) = binding.as_ref() else {
            panic!("Should have completions menu open");
        };

        assert_eq!(menu.scroll_handle_aside.offset().y, initial_offset);
    });

    // The `ctrl-d` shortcut should scroll the completion menu's aside content
    // down, so the updated offset should be lower than the initial offset.
    cx.simulate_keystroke("ctrl-d");
    cx.update_editor(|editor, _, _| {
        let binding = editor.context_menu().borrow();
        let Some(CodeContextMenu::Completions(menu)) = binding.as_ref() else {
            panic!("Should have completions menu open");
        };

        assert!(menu.scroll_handle_aside.offset().y < initial_offset);
    });

    // The `ctrl-u` shortcut should do the inverse scrolling as `ctrl-u`, so the
    // offset should now be the same as the initial offset.
    cx.simulate_keystroke("ctrl-u");
    cx.update_editor(|editor, _, _| {
        let binding = editor.context_menu().borrow();
        let Some(CodeContextMenu::Completions(menu)) = binding.as_ref() else {
            panic!("Should have completions menu open");
        };

        assert_eq!(menu.scroll_handle_aside.offset().y, initial_offset);
    });
}

#[perf]
#[gpui::test]
async fn test_rename(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    cx.set_state("const beˇfore = 2; console.log(before)", Mode::Normal);
    let def_range = cx.lsp_range("const «beforeˇ» = 2; console.log(before)");
    let tgt_range = cx.lsp_range("const before = 2; console.log(«beforeˇ»)");
    let mut prepare_request = cx.set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
        move |_, _, _| async move { Ok(Some(lsp::PrepareRenameResponse::Range(def_range))) },
    );
    let mut rename_request =
        cx.set_request_handler::<lsp::request::Rename, _, _>(move |url, params, _| async move {
            Ok(Some(lsp::WorkspaceEdit {
                changes: Some(
                    [(
                        url.clone(),
                        vec![
                            lsp::TextEdit::new(def_range, params.new_name.clone()),
                            lsp::TextEdit::new(tgt_range, params.new_name),
                        ],
                    )]
                    .into(),
                ),
                ..Default::default()
            }))
        });

    cx.simulate_keystrokes("c d");
    prepare_request.next().await.unwrap();
    cx.simulate_input("after");
    cx.simulate_keystrokes("enter");
    rename_request.next().await.unwrap();
    cx.assert_state("const afterˇ = 2; console.log(after)", Mode::Normal)
}

#[gpui::test]
async fn test_go_to_definition(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new_typescript(cx).await;

    cx.set_state("const before = 2; console.log(beforˇe)", Mode::Normal);
    let def_range = cx.lsp_range("const «beforeˇ» = 2; console.log(before)");
    let mut go_to_request =
        cx.set_request_handler::<lsp::request::GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Scalar(
                lsp::Location::new(url.clone(), def_range),
            )))
        });

    cx.simulate_keystrokes("g d");
    go_to_request.next().await.unwrap();
    cx.run_until_parked();

    cx.assert_state("const ˇbefore = 2; console.log(before)", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_remap(cx: &mut gpui::TestAppContext) {
    let mut cx = VimTestContext::new(cx, true).await;

    // test moving the cursor
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g z",
            workspace::SendKeystrokes("l l l l".to_string()),
            None,
        )])
    });
    cx.set_state("ˇ123456789", Mode::Normal);
    cx.simulate_keystrokes("g z");
    cx.assert_state("1234ˇ56789", Mode::Normal);

    // test switching modes
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g y",
            workspace::SendKeystrokes("i f o o escape l".to_string()),
            None,
        )])
    });
    cx.set_state("ˇ123456789", Mode::Normal);
    cx.simulate_keystrokes("g y");
    cx.assert_state("fooˇ123456789", Mode::Normal);

    // test recursion
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g x",
            workspace::SendKeystrokes("g z g y".to_string()),
            None,
        )])
    });
    cx.set_state("ˇ123456789", Mode::Normal);
    cx.simulate_keystrokes("g x");
    cx.assert_state("1234fooˇ56789", Mode::Normal);

    // test command
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g w",
            workspace::SendKeystrokes(": j enter".to_string()),
            None,
        )])
    });
    cx.set_state("ˇ1234\n56789", Mode::Normal);
    cx.simulate_keystrokes("g w");
    cx.assert_state("1234ˇ 56789", Mode::Normal);

    // test leaving command
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g u",
            workspace::SendKeystrokes("g w g z".to_string()),
            None,
        )])
    });
    cx.set_state("ˇ1234\n56789", Mode::Normal);
    cx.simulate_keystrokes("g u");
    cx.assert_state("1234 567ˇ89", Mode::Normal);

    // test leaving command
    cx.update(|_, cx| {
        cx.bind_keys([KeyBinding::new(
            "g t",
            workspace::SendKeystrokes("i space escape".to_string()),
            None,
        )])
    });
    cx.set_state("12ˇ34", Mode::Normal);
    cx.simulate_keystrokes("g t");
    cx.assert_state("12ˇ 34", Mode::Normal);
}

#[perf]
#[gpui::test]
async fn test_undo(cx: &mut gpui::TestAppContext) {
    let mut cx = NeovimBackedTestContext::new(cx).await;

    cx.set_shared_state("hello quˇoel world").await;
    cx.simulate_shared_keystrokes("v i w s c o escape u").await;
    cx.shared_state().await.assert_eq("hello ˇquoel world");
    cx.simulate_shared_keystrokes("ctrl-r").await;
    cx.shared_state().await.assert_eq("hello ˇco world");
    cx.simulate_shared_keystrokes("a o right l escape").await;
    cx.shared_state().await.assert_eq("hello cooˇl world");
    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq("hello cooˇ world");
    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq("hello cˇo world");
    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq("hello ˇquoel world");

    cx.set_shared_state("hello quˇoel world").await;
    cx.simulate_shared_keystrokes("v i w ~ u").await;
    cx.shared_state().await.assert_eq("hello ˇquoel world");

    cx.set_shared_state("\nhello quˇoel world\n").await;
    cx.simulate_shared_keystrokes("shift-v s c escape u").await;
    cx.shared_state().await.assert_eq("\nˇhello quoel world\n");

    cx.set_shared_state(indoc! {"
        ˇ1
        2
        3"})
        .await;

    cx.simulate_shared_keystrokes("ctrl-v shift-g ctrl-a").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇ2
        3
        4"});

    cx.simulate_shared_keystrokes("u").await;
    cx.shared_state().await.assert_eq(indoc! {"
        ˇ1
        2
        3"});
}

#[perf]
#[gpui::test]
async fn test_lsp_completions_undo(cx: &mut gpui::TestAppContext) {
    use editor::test::editor_lsp_test_context::EditorLspTestContext;
    VimTestContext::init(cx);
    let mut cx = VimTestContext::new_with_lsp(
        EditorLspTestContext::new_rust(
            lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
                ..Default::default()
            },
            cx,
        )
        .await,
        true,
    );

    cx.set_state("fn main() { let a = ˇ }", Mode::Normal);
    cx.simulate_keystroke("i");
    cx.set_state("fn main() { let a = ˇ }", Mode::Insert);

    cx.simulate_keystroke(";");
    cx.assert_state("fn main() { let a = ;ˇ }", Mode::Insert);

    cx.simulate_keystroke("escape");
    cx.assert_state("fn main() { let a = ˇ; }", Mode::Normal);

    cx.simulate_keystroke("i");
    cx.simulate_keystroke("2");
    cx.assert_state("fn main() { let a = 2ˇ; }", Mode::Insert);
    cx.simulate_keystroke(".");

    let completion_item = lsp::CompletionItem {
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 22,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "completion".to_string(),
        })),
        additional_text_edits: None,
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                task_completion_item,
            ])))
        }
    });

    request.next().await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;

    let _ = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx)
            .unwrap()
    });

    cx.assert_editor_state("fn main() { let a = 2.completionˇ; }");

    cx.simulate_keystrokes("escape u");

    cx.assert_editor_state("fn main() { let a = 2.ˇ; }");
}

#[perf]
#[gpui::test]
async fn test_lsp_completions_with_additional_edits_undo(cx: &mut gpui::TestAppContext) {
    use editor::test::editor_lsp_test_context::EditorLspTestContext;
    VimTestContext::init(cx);
    let mut cx = VimTestContext::new_with_lsp(
        EditorLspTestContext::new_rust(
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
        .await,
        true,
    );

    cx.set_state("fn main() { let a = ˇ }", Mode::Normal);
    cx.simulate_keystroke("i");
    cx.set_state("fn main() { let a = ˇ }", Mode::Insert);

    cx.simulate_keystroke("2");
    cx.simulate_keystroke(";");
    cx.assert_state("fn main() { let a = 2;ˇ }", Mode::Insert);

    cx.simulate_keystroke("escape");
    cx.assert_state("fn main() { let a = 2ˇ; }", Mode::Normal);

    cx.simulate_keystroke("i");
    cx.assert_state("fn main() { let a = 2ˇ; }", Mode::Insert);

    cx.simulate_keystroke(".");
    let completion_item = lsp::CompletionItem {
        label: "some".into(),
        kind: Some(lsp::CompletionItemKind::SNIPPET),
        detail: Some("Wrap the expression in an `Option::Some`".to_string()),
        documentation: Some(lsp::Documentation::MarkupContent(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "```rust\nSome(2)\n```".to_string(),
        })),
        deprecated: Some(false),
        sort_text: Some("fffffff2".to_string()),
        filter_text: Some("some".to_string()),
        insert_text_format: Some(lsp::InsertTextFormat::SNIPPET),
        text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 22,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "Some(2)".to_string(),
        })),
        additional_text_edits: Some(vec![lsp::TextEdit {
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 20,
                },
                end: lsp::Position {
                    line: 0,
                    character: 22,
                },
            },
            new_text: "".to_string(),
        }]),
        ..Default::default()
    };

    let closure_completion_item = completion_item.clone();
    let mut request = cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let task_completion_item = closure_completion_item.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                task_completion_item,
            ])))
        }
    });

    request.next().await;

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;

    let apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor
            .confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state("fn main() { let a = 2.Some(2)ˇ; }");

    cx.set_request_handler::<lsp::request::ResolveCompletionItem, _, _>(move |_, _, _| {
        let task_completion_item = completion_item.clone();
        async move { Ok(task_completion_item) }
    })
    .next()
    .await
    .unwrap();

    apply_additional_edits.await.unwrap();
    cx.assert_editor_state("fn main() { let a = Some(2)ˇ; }");

    cx.simulate_keystrokes("escape u");

    cx.assert_editor_state("fn main() { let a = 2.ˇ; }");
}
