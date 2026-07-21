use super::*;

#[gpui::test]
async fn test_outline_panel_lsp_document_symbols(cx: &mut TestAppContext) {
    init_test(cx);

    let root = path!("/root");
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        root,
        json!({
            "src": {
                "lib.rs": "struct Foo {\n    bar: u32,\n    baz: String,\n}\n",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [Path::new(root)], cx).await;
    let language_registry = project.read_with(cx, |project, _| {
        project.languages().add(rust_lang());
        project.languages().clone()
    });

    let mut fake_language_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_symbol_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(|fake_language_server| {
                fake_language_server
                    .set_request_handler::<lsp::request::DocumentSymbolRequest, _, _>(
                        move |_, _| async move {
                            #[allow(deprecated)]
                            Ok(Some(lsp::DocumentSymbolResponse::Nested(vec![
                                lsp::DocumentSymbol {
                                    name: "Foo".to_string(),
                                    detail: None,
                                    kind: lsp::SymbolKind::STRUCT,
                                    tags: None,
                                    deprecated: None,
                                    range: lsp::Range::new(
                                        lsp::Position::new(0, 0),
                                        lsp::Position::new(3, 1),
                                    ),
                                    selection_range: lsp::Range::new(
                                        lsp::Position::new(0, 7),
                                        lsp::Position::new(0, 10),
                                    ),
                                    children: Some(vec![
                                        lsp::DocumentSymbol {
                                            name: "bar".to_string(),
                                            detail: None,
                                            kind: lsp::SymbolKind::FIELD,
                                            tags: None,
                                            deprecated: None,
                                            range: lsp::Range::new(
                                                lsp::Position::new(1, 4),
                                                lsp::Position::new(1, 13),
                                            ),
                                            selection_range: lsp::Range::new(
                                                lsp::Position::new(1, 4),
                                                lsp::Position::new(1, 7),
                                            ),
                                            children: None,
                                        },
                                        lsp::DocumentSymbol {
                                            name: "lsp_only_field".to_string(),
                                            detail: None,
                                            kind: lsp::SymbolKind::FIELD,
                                            tags: None,
                                            deprecated: None,
                                            range: lsp::Range::new(
                                                lsp::Position::new(2, 4),
                                                lsp::Position::new(2, 15),
                                            ),
                                            selection_range: lsp::Range::new(
                                                lsp::Position::new(2, 4),
                                                lsp::Position::new(2, 7),
                                            ),
                                            children: None,
                                        },
                                    ]),
                                },
                            ])))
                        },
                    );
            })),
            ..FakeLspAdapter::default()
        },
    );

    let (window, workspace) = add_outline_panel(&project, cx).await;
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let outline_panel = outline_panel(&workspace, cx);
    cx.update(|window, cx| {
        outline_panel.update(cx, |outline_panel, cx| {
            outline_panel.set_active(true, window, cx)
        });
    });

    let _editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/root/src/lib.rs")),
                OpenOptions {
                    visible: Some(OpenVisible::All),
                    ..OpenOptions::default()
                },
                window,
                cx,
            )
        })
        .await
        .expect("Failed to open Rust source file")
        .downcast::<Editor>()
        .expect("Should open an editor for Rust source file");
    let _fake_language_server = fake_language_servers.next().await.unwrap();
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    // Step 1: tree-sitter outlines by default
    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Foo  <==== selected
  outline: bar
  outline: baz"
            ),
            "Step 1: tree-sitter outlines should be displayed by default"
        );
    });

    // Step 2: Switch to LSP document symbols
    cx.update(|_, cx| {
        settings::SettingsStore::update_global(cx, |store: &mut settings::SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.document_symbols =
                    Some(settings::DocumentSymbols::On);
            });
        });
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Foo  <==== selected
  outline: bar
  outline: lsp_only_field"
            ),
            "Step 2: After switching to LSP, should see LSP-provided symbols"
        );
    });

    // Step 3: Switch back to tree-sitter
    cx.update(|_, cx| {
        settings::SettingsStore::update_global(cx, |store: &mut settings::SettingsStore, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.document_symbols =
                    Some(settings::DocumentSymbols::Off);
            });
        });
    });
    cx.executor()
        .advance_clock(UPDATE_DEBOUNCE + Duration::from_millis(100));
    cx.run_until_parked();

    outline_panel.update(cx, |outline_panel, cx| {
        assert_eq!(
            display_entries(
                &project,
                &snapshot(outline_panel, cx),
                &outline_panel.cached_entries,
                outline_panel.selected_entry(),
                cx,
            ),
            indoc!(
                "
outline: struct Foo  <==== selected
  outline: bar
  outline: baz"
            ),
            "Step 3: tree-sitter outlines should be restored"
        );
    });
}
