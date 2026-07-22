use super::*;

#[gpui::test]
async fn test_diagnostics_with_links(cx: &mut TestAppContext) {
    init_test(cx);

    let mut cx = EditorTestContext::new(cx).await;

    cx.set_state(indoc! {"
        fn func(abˇc def: i32) -> u32 {
        }
    "});
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());

    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.update_diagnostics(
                LanguageServerId(0),
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/root/file")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range: lsp::Range::new(lsp::Position::new(0, 8), lsp::Position::new(0, 12)),
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "we've had problems with <https://link.one>, and <https://link.two> is broken".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
        })
    }).unwrap();
    cx.run_until_parked();
    cx.update_editor(|editor, window, cx| {
        editor::hover_popover::hover(editor, &Default::default(), window, cx)
    });
    cx.run_until_parked();
    cx.update_editor(|editor, _, _| assert!(editor.hover_state.diagnostic_popover.is_some()))
}

#[gpui::test]
async fn test_hover_diagnostic_and_info_popovers(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Hover with just diagnostic, pops DiagnosticPopover immediately and then
    // info popover once request completes
    cx.set_state(indoc! {"
        fn teˇst() { println!(); }
    "});
    // Send diagnostic to client
    let range = cx.lsp_range(indoc! {"
        fn «test»() { println!(); }
    "});
    let lsp_store =
        cx.update_editor(|editor, _, cx| editor.project().unwrap().read(cx).lsp_store());
    cx.update(|_, cx| {
        lsp_store.update(cx, |lsp_store, cx| {
            lsp_store.update_diagnostics(
                LanguageServerId(0),
                lsp::PublishDiagnosticsParams {
                    uri: lsp::Uri::from_file_path(path!("/root/dir/file.rs")).unwrap(),
                    version: None,
                    diagnostics: vec![lsp::Diagnostic {
                        range,
                        severity: Some(lsp::DiagnosticSeverity::ERROR),
                        message: "A test diagnostic message.".to_string(),
                        ..Default::default()
                    }],
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
        })
    })
    .unwrap();
    cx.run_until_parked();

    // Hover pops diagnostic immediately
    cx.update_editor(|editor, window, cx| editor::hover_popover::hover(editor, &Hover, window, cx));
    cx.background_executor.run_until_parked();

    cx.editor(|Editor { hover_state, .. }, _, _| {
        assert!(hover_state.diagnostic_popover.is_some());
        assert!(hover_state.info_popovers.is_empty());
    });

    // Info Popover shows after request responded to
    let range = cx.lsp_range(indoc! {"
            fn «test»() { println!(); }
        "});
    cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
        Ok(Some(lsp::Hover {
            contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                kind: lsp::MarkupKind::Markdown,
                value: "some new docs".to_string(),
            }),
            range: Some(range),
        }))
    });
    let delay = cx.update(|_, cx| EditorSettings::get_global(cx).hover_popover_delay.0 + 1);
    cx.background_executor
        .advance_clock(Duration::from_millis(delay));

    cx.background_executor.run_until_parked();
    cx.editor(|Editor { hover_state, .. }, _, _| {
        hover_state.diagnostic_popover.is_some() && hover_state.info_task.is_some()
    });
}
#[gpui::test]
async fn test_diagnostics_with_code(cx: &mut TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "main.js": "
                function test() {
                    const x = 10;
                    const y = 20;
                    return 1;
                }
                test();
            "
            .unindent(),
        }),
    )
    .await;

    let language_server_id = LanguageServerId(0);
    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let lsp_store = project.read_with(cx, |project, _| project.lsp_store());
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let uri = lsp::Uri::from_file_path(path!("/root/main.js")).unwrap();

    // Create diagnostics with code fields
    lsp_store.update(cx, |lsp_store, cx| {
        lsp_store
            .update_diagnostics(
                language_server_id,
                lsp::PublishDiagnosticsParams {
                    uri: uri.clone(),
                    diagnostics: vec![
                        lsp::Diagnostic {
                            range: lsp::Range::new(
                                lsp::Position::new(1, 4),
                                lsp::Position::new(1, 14),
                            ),
                            severity: Some(lsp::DiagnosticSeverity::WARNING),
                            code: Some(lsp::NumberOrString::String("no-unused-vars".to_string())),
                            source: Some("eslint".to_string()),
                            message: "'x' is assigned a value but never used".to_string(),
                            ..Default::default()
                        },
                        lsp::Diagnostic {
                            range: lsp::Range::new(
                                lsp::Position::new(2, 4),
                                lsp::Position::new(2, 14),
                            ),
                            severity: Some(lsp::DiagnosticSeverity::WARNING),
                            code: Some(lsp::NumberOrString::String("no-unused-vars".to_string())),
                            source: Some("eslint".to_string()),
                            message: "'y' is assigned a value but never used".to_string(),
                            ..Default::default()
                        },
                    ],
                    version: None,
                },
                None,
                DiagnosticSourceKind::Pushed,
                &[],
                cx,
            )
            .unwrap();
    });

    // Open the project diagnostics view
    let diagnostics = window.build_entity(cx, |window, cx| {
        ProjectDiagnosticsEditor::new(true, project.clone(), workspace.downgrade(), window, cx)
    });
    let editor = diagnostics.update(cx, |diagnostics, _| diagnostics.editor.clone());

    diagnostics
        .next_notification(DIAGNOSTICS_UPDATE_DEBOUNCE + Duration::from_millis(10), cx)
        .await;

    // Verify that the diagnostic codes are displayed correctly
    pretty_assertions::assert_eq!(
        editor_content_with_blocks(&editor, cx),
        indoc::indoc! {
            "§ main.js
             § -----
             function test() {
                 const x = 10; § 'x' is assigned a value but never used (eslint no-unused-vars)
                 const y = 20; § 'y' is assigned a value but never used (eslint no-unused-vars)
                 return 1;
             }"
        }
    );
}
