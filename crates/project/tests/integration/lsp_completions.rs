use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_completions_with_text_edit(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    // When text_edit exists, it takes precedence over insert_text and label
    let text = "let a = obj.fqn";
    buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
    let completions = project.update(cx, |project, cx| {
        project.completions(&buffer, text.len(), DEFAULT_COMPLETION_CONTEXT, cx)
    });

    fake_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "labelText".into(),
                    insert_text: Some("insertText".into()),
                    text_edit: Some(lsp::CompletionTextEdit::Edit(lsp::TextEdit {
                        range: lsp::Range::new(
                            lsp::Position::new(0, text.len() as u32 - 3),
                            lsp::Position::new(0, text.len() as u32),
                        ),
                        new_text: "textEditText".into(),
                    })),
                    ..Default::default()
                },
            ])))
        })
        .next()
        .await;

    let completions = completions
        .await
        .unwrap()
        .into_iter()
        .flat_map(|response| response.completions)
        .collect::<Vec<_>>();
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].new_text, "textEditText");
    assert_eq!(
        completions[0].replace_range.to_offset(&snapshot),
        text.len() - 3..text.len()
    );
}

#[gpui::test]
async fn test_completions_with_edit_ranges(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();
    let text = "let a = obj.fqn";

    // Test 1: When text_edit is None but text_edit_text exists with default edit_range
    {
        buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
        let completions = project.update(cx, |project, cx| {
            project.completions(&buffer, text.len(), DEFAULT_COMPLETION_CONTEXT, cx)
        });

        fake_server
            .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async {
                Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                    is_incomplete: false,
                    item_defaults: Some(lsp::CompletionListItemDefaults {
                        edit_range: Some(lsp::CompletionListItemDefaultsEditRange::Range(
                            lsp::Range::new(
                                lsp::Position::new(0, text.len() as u32 - 3),
                                lsp::Position::new(0, text.len() as u32),
                            ),
                        )),
                        ..Default::default()
                    }),
                    items: vec![lsp::CompletionItem {
                        label: "labelText".into(),
                        text_edit_text: Some("textEditText".into()),
                        text_edit: None,
                        ..Default::default()
                    }],
                })))
            })
            .next()
            .await;

        let completions = completions
            .await
            .unwrap()
            .into_iter()
            .flat_map(|response| response.completions)
            .collect::<Vec<_>>();
        let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].new_text, "textEditText");
        assert_eq!(
            completions[0].replace_range.to_offset(&snapshot),
            text.len() - 3..text.len()
        );
    }

    // Test 2: When both text_edit and text_edit_text are None with default edit_range
    {
        buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
        let completions = project.update(cx, |project, cx| {
            project.completions(&buffer, text.len(), DEFAULT_COMPLETION_CONTEXT, cx)
        });

        fake_server
            .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async {
                Ok(Some(lsp::CompletionResponse::List(lsp::CompletionList {
                    is_incomplete: false,
                    item_defaults: Some(lsp::CompletionListItemDefaults {
                        edit_range: Some(lsp::CompletionListItemDefaultsEditRange::Range(
                            lsp::Range::new(
                                lsp::Position::new(0, text.len() as u32 - 3),
                                lsp::Position::new(0, text.len() as u32),
                            ),
                        )),
                        ..Default::default()
                    }),
                    items: vec![lsp::CompletionItem {
                        label: "labelText".into(),
                        text_edit_text: None,
                        insert_text: Some("irrelevant".into()),
                        text_edit: None,
                        ..Default::default()
                    }],
                })))
            })
            .next()
            .await;

        let completions = completions
            .await
            .unwrap()
            .into_iter()
            .flat_map(|response| response.completions)
            .collect::<Vec<_>>();
        let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());

        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].new_text, "labelText");
        assert_eq!(
            completions[0].replace_range.to_offset(&snapshot),
            text.len() - 3..text.len()
        );
    }
}

#[gpui::test]
async fn test_completions_without_edit_ranges(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![":".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    // Test 1: When text_edit is None but insert_text exists (no edit_range in defaults)
    let text = "let a = b.fqn";
    buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
    let completions = project.update(cx, |project, cx| {
        project.completions(&buffer, text.len(), DEFAULT_COMPLETION_CONTEXT, cx)
    });

    fake_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "fullyQualifiedName?".into(),
                    insert_text: Some("fullyQualifiedName".into()),
                    ..Default::default()
                },
            ])))
        })
        .next()
        .await;
    let completions = completions
        .await
        .unwrap()
        .into_iter()
        .flat_map(|response| response.completions)
        .collect::<Vec<_>>();
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].new_text, "fullyQualifiedName");
    assert_eq!(
        completions[0].replace_range.to_offset(&snapshot),
        text.len() - 3..text.len()
    );

    // Test 2: When both text_edit and insert_text are None (no edit_range in defaults)
    let text = "let a = \"atoms/cmp\"";
    buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
    let completions = project.update(cx, |project, cx| {
        project.completions(&buffer, text.len() - 1, DEFAULT_COMPLETION_CONTEXT, cx)
    });

    fake_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "component".into(),
                    ..Default::default()
                },
            ])))
        })
        .next()
        .await;
    let completions = completions
        .await
        .unwrap()
        .into_iter()
        .flat_map(|response| response.completions)
        .collect::<Vec<_>>();
    let snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].new_text, "component");
    assert_eq!(
        completions[0].replace_range.to_offset(&snapshot),
        text.len() - 4..text.len() - 1
    );
}

#[gpui::test]
async fn test_completions_with_carriage_returns(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![":".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    let text = "let a = b.fqn";
    buffer.update(cx, |buffer, cx| buffer.set_text(text, cx));
    let completions = project.update(cx, |project, cx| {
        project.completions(&buffer, text.len(), DEFAULT_COMPLETION_CONTEXT, cx)
    });

    fake_server
        .set_request_handler::<lsp::request::Completion, _, _>(|_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "fullyQualifiedName?".into(),
                    insert_text: Some("fully\rQualified\r\nName".into()),
                    ..Default::default()
                },
            ])))
        })
        .next()
        .await;
    let completions = completions
        .await
        .unwrap()
        .into_iter()
        .flat_map(|response| response.completions)
        .collect::<Vec<_>>();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].new_text, "fully\nQualified\nName");
}
