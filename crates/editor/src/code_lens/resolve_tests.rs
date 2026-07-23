use super::test_support::*;

#[gpui::test]
async fn test_code_lens_blocks_kept_when_only_resolve_fills_titles(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: Some(true),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // The LSP returns shallow code lenses on every fetch; only `resolve`
    // populates the command/title. This is the realistic flow with
    // servers like rust-analyzer and exercises the path where each
    // post-edit refresh comes back unresolved before the resolve catches
    // up.
    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            Ok(Some(vec![lsp::CodeLens {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                command: None,
                data: Some(serde_json::json!({"id": "lens_1"})),
            }]))
        });

    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>(|lens, _| async move {
            Ok(lsp::CodeLens {
                command: Some(lsp::Command {
                    title: "1 reference".to_owned(),
                    command: "resolved_cmd".to_owned(),
                    arguments: None,
                }),
                ..lens
            })
        });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    let initial = cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 1 reference
                    Line 1: function hello() {}
                "#},
            "resolve should fill the placeholder with the server title"
        );
        editor
            .code_lens
            .as_ref()
            .map(|s| {
                s.blocks
                    .values()
                    .flatten()
                    .map(|b| b.block_id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default()
    });

    for keystroke in [" ", "x", "y"] {
        cx.update_editor(|editor, window, cx| {
            editor.move_to_end(&crate::actions::MoveToEnd, window, cx);
            editor.handle_input(keystroke, window, cx);
        });
        cx.executor()
            .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT + Duration::from_millis(50));
        assert!(
            code_lens_request.next().await.is_some(),
            "should have received another (shallow) code lens request after edit"
        );
        cx.run_until_parked();

        let after = cx.editor(|editor, _, cx| {
            assert_eq!(
                code_lens_assertion_text(editor, cx),
                indoc! {r#"
                        Lenses: 1 reference
                        Line 1: function hello() {}
                    "#},
                "refresh+resolve cycle should keep rendering the same title"
            );
            editor
                .code_lens
                .as_ref()
                .map(|s| {
                    s.blocks
                        .values()
                        .flatten()
                        .map(|b| b.block_id)
                        .collect::<HashSet<_>>()
                })
                .unwrap_or_default()
        });
        assert_eq!(
            after, initial,
            "Block IDs must survive the unresolved-fetch → resolve cycle without churn"
        );
    }
}

#[gpui::test]
async fn test_code_lens_placeholder_block_before_resolve(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: Some(true),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            let mut lenses = Vec::new();
            lenses.push(lsp::CodeLens {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                command: None,
                data: Some(serde_json::json!({"id": "lens_1"})),
            });
            Ok(Some(lenses))
        });

    let (resolve_tx, resolve_rx) = futures::channel::oneshot::channel::<()>();
    let resolve_rx = std::sync::Mutex::new(Some(resolve_rx));
    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>(move |lens, _| {
            let rx = resolve_rx.lock().unwrap().take();
            async move {
                if let Some(rx) = rx {
                    rx.await.ok();
                }
                Ok(lsp::CodeLens {
                    command: Some(lsp::Command {
                        title: "1 reference".to_owned(),
                        command: "resolved_cmd".to_owned(),
                        arguments: None,
                    }),
                    ..lens
                })
            }
        });

    cx.set_state("ˇfunction hello() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: <placeholder>
                    Line 1: function hello() {}
                "#},
            "placeholder spacer should be reserved with no rendered text before resolve"
        );
    });

    resolve_tx.send(()).ok();
    cx.run_until_parked();

    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 1 reference
                    Line 1: function hello() {}
                "#},
            "after resolve the placeholder should display the server title"
        );
    });
}

#[gpui::test]
async fn test_code_lens_placeholder_kept_when_resolve_yields_empty_title(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: Some(true),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            let mut lenses = Vec::new();
            lenses.push(lsp::CodeLens {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                command: None,
                data: Some(serde_json::json!({"id": "lens_1"})),
            });
            Ok(Some(lenses))
        });

    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>(|lens, _| async move {
            Ok(lsp::CodeLens {
                command: Some(lsp::Command {
                    title: String::new(),
                    command: "noop".to_owned(),
                    arguments: None,
                }),
                ..lens
            })
        });

    cx.set_state("ˇfunction hello() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 0 references
                    Line 1: function hello() {}
                "#},
            "lens resolved to an empty title should fall back to the synthetic label"
        );
    });
}

#[gpui::test]
async fn test_code_lens_same_range_lenses_resolve_independently(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: Some(true),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // Two shallow lenses on the same range, distinguished only by `data`
    // — exactly the shape vtsls/TypeScript-LS uses for the
    // "references" + "implementations" pair on the same line.
    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            Ok(Some(vec![
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                    command: None,
                    data: Some(serde_json::json!({"kind": "references"})),
                },
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                    command: None,
                    data: Some(serde_json::json!({"kind": "implementations"})),
                },
            ]))
        });

    let resolve_calls = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>({
            let resolve_calls = resolve_calls.clone();
            move |lens, _| {
                let resolve_calls = resolve_calls.clone();
                async move {
                    let kind = lens
                        .data
                        .as_ref()
                        .and_then(|d| d.get("kind"))
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    resolve_calls.lock().unwrap().push(kind.clone());
                    let title = match kind.as_str() {
                        Some("references") => "2 references",
                        Some("implementations") => "1 implementation",
                        _ => "",
                    };
                    Ok(lsp::CodeLens {
                        command: Some(lsp::Command {
                            title: title.to_owned(),
                            command: "noop".to_owned(),
                            arguments: None,
                        }),
                        ..lens
                    })
                }
            }
        });

    cx.set_state("ˇfunction hello() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    let calls = resolve_calls.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        2,
        "both same-range lenses should be resolved independently, got {calls:?}"
    );
    let kinds: Vec<&str> = calls.iter().filter_map(|v| v.as_str()).collect();
    assert_eq!(kinds.contains(&"references"), true);
    assert_eq!(kinds.contains(&"implementations"), true);

    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 2 references | 1 implementation
                    Line 1: function hello() {}
                "#},
            "both same-range lenses should render their resolved titles"
        );
    });
}

#[gpui::test]
async fn test_code_lens_placeholder_kept_when_resolve_yields_no_command(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: Some(true),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            Ok(Some(vec![lsp::CodeLens {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                command: None,
                data: Some(serde_json::json!({"id": "lens_1"})),
            }]))
        });

    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>(|lens, _| async move {
            Ok(lsp::CodeLens {
                command: None,
                ..lens
            })
        });

    cx.set_state("ˇfunction hello() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 0 references
                    Line 1: function hello() {}
                "#},
            "lens resolved without a command should fall back to the synthetic label"
        );
    });
}
