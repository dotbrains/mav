use super::*;

#[gpui::test]
async fn test_insert_snippet(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    cx.update_editor(|editor, _, cx| {
        editor.project().unwrap().update(cx, |project, cx| {
            project.snippets().update(cx, |snippets, _cx| {
                let snippet = project::snippet_provider::Snippet {
                    prefix: vec![], // no prefix needed!
                    body: "an Unspecified".to_string(),
                    description: Some("shhhh it's a secret".to_string()),
                    name: "super secret snippet".to_string(),
                };
                snippets.add_snippet_for_test(
                    None,
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );

                let snippet = project::snippet_provider::Snippet {
                    prefix: vec![], // no prefix needed!
                    body: " Location".to_string(),
                    description: Some("the word 'location'".to_string()),
                    name: "location word".to_string(),
                };
                snippets.add_snippet_for_test(
                    Some("Markdown".to_string()),
                    PathBuf::from("test_snippets.json"),
                    vec![Arc::new(snippet)],
                );
            });
        })
    });

    cx.set_state(indoc!(r#"First cursor at ˇ and second cursor at ˇ"#));

    cx.update_editor(|editor, window, cx| {
        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: None,
                name: Some("super secret snippet".to_string()),
                snippet: None,
            },
            window,
            cx,
        );

        // Language is specified in the action,
        // so the buffer language does not need to match
        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: Some("Markdown".to_string()),
                name: Some("location word".to_string()),
                snippet: None,
            },
            window,
            cx,
        );

        editor.insert_snippet_at_selections(
            &InsertSnippet {
                language: None,
                name: None,
                snippet: Some("$0 after".to_string()),
            },
            window,
            cx,
        );
    });

    cx.assert_editor_state(
        r#"First cursor at an Unspecified Locationˇ after and second cursor at an Unspecified Locationˇ after"#,
    );
}

#[gpui::test]
async fn test_inlay_hints_request_timeout(cx: &mut TestAppContext) {
    use crate::inlays::inlay_hints::InlayHintRefreshReason;
    use crate::inlays::inlay_hints::tests::{cached_hint_labels, init_test, visible_hint_labels};
    use settings::InlayHintSettingsContent;
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    const BASE_TIMEOUT_SECS: u64 = 1;

    let request_count = Arc::new(AtomicU32::new(0));
    let closure_request_count = request_count.clone();

    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, &|settings: &mut SettingsContent| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    request_timeout: Some(BASE_TIMEOUT_SECS),
                    button: Some(true),
                    notifications: None,
                    semantic_token_rules: None,
                });
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let a = 5; }",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let request_count = closure_request_count.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                    move |params, cx| {
                        let request_count = request_count.clone();
                        async move {
                            cx.background_executor()
                                .timer(Duration::from_secs(BASE_TIMEOUT_SECS * 2))
                                .await;
                            let count = request_count.fetch_add(1, atomic::Ordering::Release) + 1;
                            assert_eq!(
                                params.text_document.uri,
                                lsp::Uri::from_file_path(path!("/a/main.rs")).unwrap(),
                            );
                            Ok(Some(vec![lsp::InlayHint {
                                position: lsp::Position::new(0, 1),
                                label: lsp::InlayHintLabel::String(count.to_string()),
                                kind: None,
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            }]))
                        }
                    },
                );
            })),
            ..FakeLspAdapter::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor = cx.add_window(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));

    cx.executor().run_until_parked();
    let fake_server = fake_servers.next().await.unwrap();

    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert!(
                cached_hint_labels(editor, cx).is_empty(),
                "First request should time out, no hints cached"
            );
        })
        .unwrap();

    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(1),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert!(
                cached_hint_labels(editor, cx).is_empty(),
                "Second request should also time out with BASE_TIMEOUT, no hints cached"
            );
        })
        .unwrap();

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.global_lsp_settings = Some(GlobalLspSettingsContent {
                    request_timeout: Some(BASE_TIMEOUT_SECS * 4),
                    button: Some(true),
                    notifications: None,
                    semantic_token_rules: None,
                });
            });
        });
    });
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server.server.server_id(),
                    request_id: Some(2),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor()
        .advance_clock(Duration::from_secs(BASE_TIMEOUT_SECS * 4) + Duration::from_millis(100));
    cx.executor().run_until_parked();
    editor
        .update(cx, |editor, _window, cx| {
            assert_eq!(
                vec!["1".to_string()],
                cached_hint_labels(editor, cx),
                "With extended timeout (BASE * 4), hints should arrive successfully"
            );
            assert_eq!(vec!["1".to_string()], visible_hint_labels(editor, cx));
        })
        .unwrap();
}
