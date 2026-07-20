use super::*;

#[gpui::test]
async fn test_completions_default_resolve_data_handling(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let item_0 = lsp::CompletionItem {
        label: "abs".into(),
        insert_text: Some("abs".into()),
        data: Some(json!({ "very": "special"})),
        insert_text_mode: Some(lsp::InsertTextMode::ADJUST_INDENTATION),
        text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
            lsp::InsertReplaceEdit {
                new_text: "abs".to_string(),
                insert: lsp::Range::default(),
                replace: lsp::Range::default(),
            },
        )),
        ..lsp::CompletionItem::default()
    };
    let items = iter::once(item_0.clone())
        .chain((11..51).map(|i| lsp::CompletionItem {
            label: format!("item_{}", i),
            insert_text: Some(format!("item_{}", i)),
            insert_text_format: Some(lsp::InsertTextFormat::PLAIN_TEXT),
            ..lsp::CompletionItem::default()
        }))
        .collect::<Vec<_>>();

    let default_commit_characters = vec!["?".to_string()];
    let default_data = json!({ "default": "data"});
    let default_insert_text_format = lsp::InsertTextFormat::SNIPPET;
    let default_insert_text_mode = lsp::InsertTextMode::AS_IS;
    let default_edit_range = lsp::Range {
        start: lsp::Position {
            line: 0,
            character: 5,
        },
        end: lsp::Position {
            line: 0,
            character: 5,
        },
    };

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

    let completion_data = default_data.clone();
    let completion_characters = default_commit_characters.clone();
    let completion_items = items.clone();
    cx.set_request_handler::<lsp::request::Completion, _, _>(move |_, _, _| {
        let default_data = completion_data.clone();
        let default_commit_characters = completion_characters.clone();
        let items = completion_items.clone();
        async move {
            Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                items,
                item_defaults: Some(lsp::CompletionListItemDefaults {
                    data: Some(default_data.clone()),
                    commit_characters: Some(default_commit_characters.clone()),
                    edit_range: Some(lsp::CompletionListItemDefaultsEditRange::Range(
                        default_edit_range,
                    )),
                    insert_text_format: Some(default_insert_text_format),
                    insert_text_mode: Some(default_insert_text_mode),
                }),
                ..lsp::CompletionList::default()
            })))
        }
    })
    .next()
    .await;

    let resolved_items = Arc::new(Mutex::new(Vec::new()));
    cx.lsp
        .server
        .on_request::<lsp::request::ResolveCompletionItem, _, _>({
            let closure_resolved_items = resolved_items.clone();
            move |item_to_resolve, _| {
                let closure_resolved_items = closure_resolved_items.clone();
                async move {
                    closure_resolved_items.lock().push(item_to_resolve.clone());
                    Ok(item_to_resolve)
                }
            }
        })
        .detach();

    cx.condition(|editor, _| editor.context_menu_visible())
        .await;
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| {
        let menu = editor.context_menu.borrow_mut();
        match menu.as_ref().expect("should have the completions menu") {
            CodeContextMenu::Completions(completions_menu) => {
                assert_eq!(
                    completions_menu
                        .entries
                        .borrow()
                        .iter()
                        .filter_map(|entry| entry.as_match().map(|m| m.string.clone()))
                        .collect::<Vec<String>>(),
                    items
                        .iter()
                        .map(|completion| completion.label.clone())
                        .collect::<Vec<String>>()
                );
            }
            CodeContextMenu::CodeActions(_) => panic!("Expected to have the completions menu"),
        }
    });
    // Approximate initial displayed interval is 0..12. With extra item padding of 4 this is 0..16
    // with 4 from the end.
    assert_eq!(
        *resolved_items.lock(),
        [&items[0..16], &items[items.len() - 4..items.len()]]
            .concat()
            .iter()
            .cloned()
            .map(|mut item| {
                if item.data.is_none() {
                    item.data = Some(default_data.clone());
                }
                item
            })
            .collect::<Vec<lsp::CompletionItem>>(),
        "Items sent for resolve should be unchanged modulo resolve `data` filled with default if missing"
    );
    resolved_items.lock().clear();

    cx.update_editor(|editor, window, cx| {
        editor.context_menu_prev(&ContextMenuPrevious, window, cx);
    });
    cx.run_until_parked();
    // Completions that have already been resolved are skipped.
    assert_eq!(
        *resolved_items.lock(),
        items[items.len() - 17..items.len() - 4]
            .iter()
            .cloned()
            .map(|mut item| {
                if item.data.is_none() {
                    item.data = Some(default_data.clone());
                }
                item
            })
            .collect::<Vec<lsp::CompletionItem>>()
    );
    resolved_items.lock().clear();
}

#[gpui::test]
async fn test_completions_in_languages_with_extra_word_characters(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new(
        Language::new(
            LanguageConfig {
                matcher: LanguageMatcher {
                    path_suffixes: vec!["jsx".into()],
                    ..Default::default()
                },
                overrides: [(
                    "element".into(),
                    LanguageConfigOverride {
                        completion_query_characters: Override::Set(['-'].into_iter().collect()),
                        ..Default::default()
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
            Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        )
        .with_override_query("(jsx_self_closing_element) @element")
        .unwrap(),
        lsp::ServerCapabilities {
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![":".to_string()]),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.lsp
        .set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "bg-blue".into(),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "bg-red".into(),
                    ..Default::default()
                },
                lsp::CompletionItem {
                    label: "bg-yellow".into(),
                    ..Default::default()
                },
            ])))
        });

    cx.set_state(r#"<p class="bgˇ" />"#);

    // Trigger completion when typing a dash, because the dash is an extra
    // word character in the 'element' scope, which contains the cursor.
    cx.simulate_keystroke("-");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(
                completion_menu_entries(menu),
                &["bg-blue", "bg-red", "bg-yellow"]
            );
        } else {
            panic!("expected completion menu to be open");
        }
    });

    cx.simulate_keystroke("l");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["bg-blue", "bg-yellow"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });

    // When filtering completions, consider the character after the '-' to
    // be the start of a subword.
    cx.set_state(r#"<p class="yelˇ" />"#);
    cx.simulate_keystroke("l");
    cx.executor().run_until_parked();
    cx.update_editor(|editor, _, _| {
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            assert_eq!(completion_menu_entries(menu), &["bg-yellow"]);
        } else {
            panic!("expected completion menu to be open");
        }
    });
}
