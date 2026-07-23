use super::test_support::*;

#[gpui::test]
async fn test_code_lens_blocks(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: None,
            }),
            execute_command_provider: Some(lsp::ExecuteCommandOptions {
                commands: vec!["lens_cmd".to_string()],
                ..lsp::ExecuteCommandOptions::default()
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let mut code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            Ok(Some(vec![
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                    command: Some(lsp::Command {
                        title: "2 references".to_owned(),
                        command: "lens_cmd".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(1, 0), lsp::Position::new(1, 19)),
                    command: Some(lsp::Command {
                        title: "0 references".to_owned(),
                        command: "lens_cmd".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
            ]))
        });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received a code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, cx| {
        assert_eq!(
            editor.code_lens_enabled(),
            true,
            "code lens should be enabled"
        );
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 2 references
                    Line 1: function hello() {}

                    Lenses: 0 references
                    Line 2: function world() {}
                "#},
            "both lenses should render their server-provided titles"
        );
    });
}

#[gpui::test]
async fn test_code_lens_refresh_requeries_open_document(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: None,
            }),
            execute_command_provider: Some(lsp::ExecuteCommandOptions {
                commands: vec!["lens_cmd".to_string()],
                ..lsp::ExecuteCommandOptions::default()
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let lens_title = Arc::new(Mutex::new("Initial lens".to_string()));
    let mut code_lens_request = cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>({
        let lens_title = lens_title.clone();
        move |_, _, _| {
            let lens_title = lens_title.clone();
            async move {
                let title = lens_title.lock().unwrap().clone();
                Ok(Some(vec![lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                    command: Some(lsp::Command {
                        title,
                        command: "lens_cmd".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                }]))
            }
        }
    });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");
    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();
    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: Initial lens
                    Line 1: function hello() {}
                "#},
            "initial fetch should render the server title"
        );
    });

    *lens_title.lock().unwrap() = "Refreshed lens".to_string();
    cx.lsp
        .request::<lsp::request::CodeLensRefresh>((), lsp::DEFAULT_LSP_REQUEST_TIMEOUT)
        .await
        .into_response()
        .expect("code lens refresh request failed");
    cx.executor()
        .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT * 2);
    cx.run_until_parked();
    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: Refreshed lens
                    Line 1: function hello() {}
                "#},
            "refresh should update the displayed lens to the new server title"
        );
    });
}

#[gpui::test]
async fn test_code_lens_dynamic_registration_requeries_open_document(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    // The server advertises no code lens capability up front; it registers
    // `textDocument/codeLens` dynamically only after the document is open.
    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            execute_command_provider: Some(lsp::ExecuteCommandOptions {
                commands: vec!["lens_cmd".to_string()],
                ..lsp::ExecuteCommandOptions::default()
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    let _code_lens_request =
        cx.set_request_handler::<lsp::request::CodeLensRequest, _, _>(move |_, _, _| async {
            Ok(Some(vec![lsp::CodeLens {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                command: Some(lsp::Command {
                    title: "Dynamic lens".to_owned(),
                    command: "lens_cmd".to_owned(),
                    arguments: None,
                }),
                data: None,
            }]))
        });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");
    // Drain any debounced refresh scheduled before the capability exists, so
    // the post-registration re-query can only come from the dynamic
    // registration handling itself.
    cx.executor()
        .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT * 2);
    cx.run_until_parked();
    cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            "\n",
            "no lenses should render before the capability is registered"
        );
    });

    cx.lsp
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: "code-lens".to_string(),
                    method: "textDocument/codeLens".to_string(),
                    register_options: Some(
                        serde_json::to_value(lsp::CodeLensOptions {
                            resolve_provider: None,
                        })
                        .unwrap(),
                    ),
                }],
            },
            lsp::DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .expect("register capability request failed");
    cx.executor()
        .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT * 2);
    cx.run_until_parked();
    cx.editor(|editor, _, cx| {
            assert_eq!(
                code_lens_assertion_text(editor, cx),
                indoc! {r#"
                    Lenses: Dynamic lens
                    Line 1: function hello() {}
                "#},
                "dynamic textDocument/codeLens registration should re-query and display lenses for the open document"
            );
        });
}

#[gpui::test]
async fn test_code_lens_blocks_kept_across_refresh(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(CodeLens::On);
    });

    let mut cx = EditorLspTestContext::new_typescript(
        lsp::ServerCapabilities {
            code_lens_provider: Some(lsp::CodeLensOptions {
                resolve_provider: None,
            }),
            execute_command_provider: Some(lsp::ExecuteCommandOptions {
                commands: vec!["lens_cmd".to_string()],
                ..lsp::ExecuteCommandOptions::default()
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
                command: Some(lsp::Command {
                    title: "1 reference".to_owned(),
                    command: "lens_cmd".to_owned(),
                    arguments: None,
                }),
                data: None,
            }]))
        });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received the initial code lens request"
    );
    cx.run_until_parked();

    let initial_block_ids = cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 1 reference
                    Line 1: function hello() {}
                "#},
            "initial fetch should render the server title"
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

    cx.update_editor(|editor, window, cx| {
        editor.move_to_end(&crate::actions::MoveToEnd, window, cx);
        editor.handle_input("\n// trailing comment", window, cx);
    });
    cx.executor()
        .advance_clock(LSP_REQUEST_DEBOUNCE_TIMEOUT + Duration::from_millis(50));
    assert!(
        code_lens_request.next().await.is_some(),
        "should have received another code lens request after edit"
    );
    cx.run_until_parked();

    let refreshed_block_ids = cx.editor(|editor, _, cx| {
        assert_eq!(
            code_lens_assertion_text(editor, cx),
            indoc! {r#"
                    Lenses: 1 reference
                    Line 1: function hello() {}
                "#},
            "refreshed block should keep rendering the same title"
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
        refreshed_block_ids, initial_block_ids,
        "Code lens blocks should be preserved across refreshes when their content is unchanged"
    );
}
