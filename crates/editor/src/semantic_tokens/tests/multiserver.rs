use super::*;

#[gpui::test]
async fn lsp_semantic_tokens_multiserver_full(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            "TOML".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..LanguageSettingsContent::default()
            },
        );
    });

    let toml_language = Arc::new(Language::new(
        LanguageConfig {
            name: "TOML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["toml".into()],
                ..LanguageMatcher::default()
            },
            ..LanguageConfig::default()
        },
        None,
    ));

    // We have 2 language servers for TOML in this test.
    let toml_legend_1 = lsp::SemanticTokensLegend {
        token_types: vec!["property".into()],
        token_modifiers: Vec::new(),
    };
    let toml_legend_2 = lsp::SemanticTokensLegend {
        token_types: vec!["number".into()],
        token_modifiers: Vec::new(),
    };

    let app_state = cx.update(workspace::AppState::test);

    cx.update(|cx| {
        assets::Assets.load_test_fonts(cx);
        crate::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    let project = Project::test(app_state.fs.clone(), [], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let full_counter_toml_1 = Arc::new(AtomicUsize::new(0));
    let full_counter_toml_1_clone = full_counter_toml_1.clone();
    let full_counter_toml_2 = Arc::new(AtomicUsize::new(0));
    let full_counter_toml_2_clone = full_counter_toml_2.clone();

    let mut toml_server_1 = language_registry.register_fake_lsp(
        toml_language.name(),
        FakeLspAdapter {
            name: "toml1",
            capabilities: lsp::ServerCapabilities {
                semantic_tokens_provider: Some(
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        lsp::SemanticTokensOptions {
                            legend: toml_legend_1,
                            full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                            ..lsp::SemanticTokensOptions::default()
                        },
                    ),
                ),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let full_counter_toml_1_clone = full_counter_toml_1_clone.clone();
                move |fake_server| {
                    let full_counter = full_counter_toml_1_clone.clone();
                    fake_server
                        .set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
                            move |_, _| {
                                full_counter.fetch_add(1, atomic::Ordering::Release);
                                async move {
                                    Ok(Some(lsp::SemanticTokensResult::Tokens(
                                        lsp::SemanticTokens {
                                            // highlight 'a' as a property
                                            data: vec![
                                                0, // delta_line
                                                0, // delta_start
                                                1, // length
                                                0, // token_type
                                                0, // token_modifiers_bitset
                                            ],
                                            result_id: Some("a".into()),
                                        },
                                    )))
                                }
                            },
                        );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );
    let mut toml_server_2 = language_registry.register_fake_lsp(
        toml_language.name(),
        FakeLspAdapter {
            name: "toml2",
            capabilities: lsp::ServerCapabilities {
                semantic_tokens_provider: Some(
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        lsp::SemanticTokensOptions {
                            legend: toml_legend_2,
                            full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                            ..lsp::SemanticTokensOptions::default()
                        },
                    ),
                ),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let full_counter_toml_2_clone = full_counter_toml_2_clone.clone();
                move |fake_server| {
                    let full_counter = full_counter_toml_2_clone.clone();
                    fake_server
                        .set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
                            move |_, _| {
                                full_counter.fetch_add(1, atomic::Ordering::Release);
                                async move {
                                    Ok(Some(lsp::SemanticTokensResult::Tokens(
                                        lsp::SemanticTokens {
                                            // highlight '3' as a literal
                                            data: vec![
                                                0, // delta_line
                                                4, // delta_start
                                                1, // length
                                                0, // token_type
                                                0, // token_modifiers_bitset
                                            ],
                                            result_id: Some("a".into()),
                                        },
                                    )))
                                }
                            },
                        );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );
    language_registry.add(toml_language.clone());

    app_state
        .fs
        .as_fake()
        .insert_tree(
            EditorLspTestContext::root_path(),
            json!({
                ".git": {},
                "dir": {
                    "foo.toml": "a = 1\nb = 2\n",
                }
            }),
        )
        .await;

    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(EditorLspTestContext::root_path(), true, cx)
        })
        .await
        .unwrap();
    cx.read(|cx| workspace.read(cx).worktree_scans_complete(cx))
        .await;

    let toml_file = cx.read(|cx| workspace.file_project_paths(cx)[0].clone());
    let toml_item = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(toml_file, None, true, window, cx)
        })
        .await
        .expect("Could not open test file");

    let editor = cx.update(|_, cx| {
        toml_item
            .act_as::<Editor>(cx)
            .expect("Opened test file wasn't an editor")
    });

    editor.update_in(cx, |editor, window, cx| {
        let nav_history = workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .nav_history_for_item(&cx.entity());
        editor.set_nav_history(Some(nav_history));
        window.focus(&editor.focus_handle(cx), cx)
    });

    let _toml_server_1 = toml_server_1.next().await.unwrap();
    let _toml_server_2 = toml_server_2.next().await.unwrap();

    // Trigger semantic tokens.
    editor.update_in(cx, |editor, _, cx| {
        editor.edit([(MultiBufferOffset(0)..MultiBufferOffset(1), "b")], cx);
    });
    cx.executor().advance_clock(Duration::from_millis(200));
    let task = editor.update_in(cx, |e, _, _| e.semantic_token_state.take_update_task());
    cx.run_until_parked();
    task.await;

    assert_eq!(
        extract_semantic_highlights(&editor, &cx),
        vec![
            MultiBufferOffset(0)..MultiBufferOffset(1),
            MultiBufferOffset(4)..MultiBufferOffset(5),
        ]
    );

    assert_eq!(full_counter_toml_1.load(atomic::Ordering::Acquire), 1);
    assert_eq!(full_counter_toml_2.load(atomic::Ordering::Acquire), 1);
}
