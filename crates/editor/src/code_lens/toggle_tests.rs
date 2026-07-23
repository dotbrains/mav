use super::test_support::*;

#[gpui::test]
async fn test_code_lens_disabled_by_default(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

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

    cx.lsp
        .set_request_handler::<lsp::request::CodeLensRequest, _, _>(|_, _| async move {
            panic!("Should not request code lenses when disabled");
        });

    cx.set_state("ˇfunction hello() {}");
    cx.run_until_parked();

    cx.editor(|editor, _, _cx| {
        assert_eq!(
            editor.code_lens_enabled(),
            false,
            "code lens should not be enabled when setting is off"
        );
    });
}

#[gpui::test]
async fn test_code_lens_toggling(cx: &mut TestAppContext) {
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

    cx.set_state("ˇfunction hello() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received a code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, _cx| {
        assert_eq!(
            editor.code_lens_enabled(),
            true,
            "code lens should be enabled"
        );
        let total_blocks: usize = editor
            .code_lens
            .as_ref()
            .map(|s| s.blocks.values().map(|v| v.len()).sum())
            .unwrap_or(0);
        assert_eq!(total_blocks, 1, "Should have one code lens block");
    });

    cx.update_editor(|editor, _window, cx| {
        editor.clear_code_lenses(cx);
    });

    cx.editor(|editor, _, _cx| {
        assert_eq!(
            editor.code_lens_enabled(),
            false,
            "code lens should be disabled after clearing"
        );
    });
}

#[gpui::test]
async fn test_code_lens_resolve(cx: &mut TestAppContext) {
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
            Ok(Some(vec![
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 19)),
                    command: None,
                    data: Some(serde_json::json!({"id": "lens_1"})),
                },
                lsp::CodeLens {
                    range: lsp::Range::new(lsp::Position::new(1, 0), lsp::Position::new(1, 19)),
                    command: None,
                    data: Some(serde_json::json!({"id": "lens_2"})),
                },
            ]))
        });

    cx.lsp
        .set_request_handler::<lsp::request::CodeLensResolve, _, _>(|lens, _| async move {
            let id = lens
                .data
                .as_ref()
                .and_then(|d| d.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let title = match id {
                "lens_1" => "3 references",
                "lens_2" => "1 implementation",
                _ => "unknown",
            };
            Ok(lsp::CodeLens {
                command: Some(lsp::Command {
                    title: title.to_owned(),
                    command: format!("resolved_{id}"),
                    arguments: None,
                }),
                ..lens
            })
        });

    cx.set_state("ˇfunction hello() {}\nfunction world() {}");

    assert!(
        code_lens_request.next().await.is_some(),
        "should have received a code lens request"
    );
    cx.run_until_parked();

    cx.editor(|editor, _, _cx| {
        let total_blocks: usize = editor
            .code_lens
            .as_ref()
            .map(|s| s.blocks.values().map(|v| v.len()).sum())
            .unwrap_or(0);
        assert_eq!(
            total_blocks, 2,
            "Unresolved lenses should have been resolved and displayed"
        );
    });
}
