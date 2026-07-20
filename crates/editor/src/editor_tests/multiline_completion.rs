use super::*;

fn gen_text_edit(params: &CompletionParams, text: &str) -> Option<lsp::CompletionTextEdit> {
    let position = || lsp::Position {
        line: params.text_document_position.position.line,
        character: params.text_document_position.position.character,
    };
    Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
        range: lsp::Range {
            start: position(),
            end: position(),
        },
        new_text: text.to_string(),
    }))
}

#[gpui::test]
async fn test_multiline_completion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.ts": "a",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let typescript_language = Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..LanguageMatcher::default()
            },
            line_comments: vec!["// ".into()],
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    ));
    language_registry.add(typescript_language.clone());
    let mut fake_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    ..lsp::CompletionOptions::default()
                }),
                signature_help_provider: Some(lsp::SignatureHelpOptions::default()),
                ..lsp::ServerCapabilities::default()
            },
            // Emulate vtsls label generation
            label_for_completion: Some(Box::new(|item, _| {
                let text = if let Some(description) = item
                    .label_details
                    .as_ref()
                    .and_then(|label_details| label_details.description.as_ref())
                {
                    format!("{} {}", item.label, description)
                } else if let Some(detail) = &item.detail {
                    format!("{} {}", item.label, detail)
                } else {
                    item.label.clone()
                };
                Some(language::CodeLabel::plain(text, None))
            })),
            ..FakeLspAdapter::default()
        },
    );
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);
    let worktree_id = workspace.update_in(cx, |workspace, _window, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.ts"), cx)
        })
        .await
        .unwrap();
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("main.ts")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    let fake_server = fake_servers.next().await.unwrap();
    cx.run_until_parked();

    let multiline_label = "StickyHeaderExcerpt {\n            excerpt,\n            next_excerpt_controls_present,\n            next_buffer_row,\n        }: StickyHeaderExcerpt<'_>,";
    let multiline_label_2 = "a\nb\nc\n";
    let multiline_detail = "[]struct {\n\tSignerId\tstruct {\n\t\tIssuer\t\t\tstring\t`json:\"issuer\"`\n\t\tSubjectSerialNumber\"`\n}}";
    let multiline_description = "d\ne\nf\n";
    let multiline_detail_2 = "g\nh\ni\n";

    let mut completion_handle = fake_server.set_request_handler::<lsp::request::Completion, _, _>(
        move |params, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: multiline_label.to_string(),
                    text_edit: gen_text_edit(&params, "new_text_1"),
                    ..lsp::CompletionItem::default()
                },
                lsp::CompletionItem {
                    label: "single line label 1".to_string(),
                    detail: Some(multiline_detail.to_string()),
                    text_edit: gen_text_edit(&params, "new_text_2"),
                    ..lsp::CompletionItem::default()
                },
                lsp::CompletionItem {
                    label: "single line label 2".to_string(),
                    label_details: Some(lsp::CompletionItemLabelDetails {
                        description: Some(multiline_description.to_string()),
                        detail: None,
                    }),
                    text_edit: gen_text_edit(&params, "new_text_2"),
                    ..lsp::CompletionItem::default()
                },
                lsp::CompletionItem {
                    label: multiline_label_2.to_string(),
                    detail: Some(multiline_detail_2.to_string()),
                    text_edit: gen_text_edit(&params, "new_text_3"),
                    ..lsp::CompletionItem::default()
                },
                lsp::CompletionItem {
                    label: "Label with many     spaces and \t but without newlines".to_string(),
                    detail: Some(
                        "Details with many     spaces and \t but without newlines".to_string(),
                    ),
                    text_edit: gen_text_edit(&params, "new_text_4"),
                    ..lsp::CompletionItem::default()
                },
            ])))
        },
    );

    editor.update_in(cx, |editor, window, cx| {
        cx.focus_self(window);
        editor.move_to_end(&MoveToEnd, window, cx);
        editor.handle_input(".", window, cx);
    });
    cx.run_until_parked();
    completion_handle.next().await.unwrap();

    editor.update(cx, |editor, _| {
        assert!(editor.context_menu_visible());
        if let Some(CodeContextMenu::Completions(menu)) = editor.context_menu.borrow_mut().as_ref()
        {
            let completion_labels = menu
                .completions
                .borrow()
                .iter()
                .map(|c| c.label.text.clone())
                .collect::<Vec<_>>();
            assert_eq!(
                completion_labels,
                &[
                    "StickyHeaderExcerpt { excerpt, next_excerpt_controls_present, next_buffer_row, }: StickyHeaderExcerpt<'_>,",
                    "single line label 1 []struct { SignerId struct { Issuer string `json:\"issuer\"` SubjectSerialNumber\"` }}",
                    "single line label 2 d e f ",
                    "a b c g h i ",
                    "Label with many     spaces and \t but without newlines Details with many     spaces and \t but without newlines",
                ],
                "Completion items should have their labels without newlines, also replacing excessive whitespaces. Completion items without newlines should not be altered.",
            );

            for completion in menu
                .completions
                .borrow()
                .iter() {
                    assert_eq!(
                        completion.label.filter_range,
                        0..completion.label.text.len(),
                        "Adjusted completion items should still keep their filter ranges for the entire label. Item: {completion:?}"
                    );
                }
        } else {
            panic!("expected completion menu to be open");
        }
    });
}
