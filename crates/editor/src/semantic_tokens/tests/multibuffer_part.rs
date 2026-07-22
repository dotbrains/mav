use super::*;

async fn lsp_semantic_tokens_multibuffer_part(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            "TOML".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..LanguageSettingsContent::default()
            },
        );
        language_settings.languages.0.insert(
            "Rust".into(),
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
    let rust_language = Arc::new(Language::new(
        LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".into()],
                ..LanguageMatcher::default()
            },
            ..LanguageConfig::default()
        },
        None,
    ));

    let toml_legend = lsp::SemanticTokensLegend {
        token_types: vec!["property".into()],
        token_modifiers: Vec::new(),
    };
    let rust_legend = lsp::SemanticTokensLegend {
        token_types: vec!["constant".into()],
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
    let full_counter_toml = Arc::new(AtomicUsize::new(0));
    let full_counter_toml_clone = full_counter_toml.clone();

    let mut toml_server = language_registry.register_fake_lsp(
        toml_language.name(),
        FakeLspAdapter {
            name: "toml",
            capabilities: lsp::ServerCapabilities {
                semantic_tokens_provider: Some(
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        lsp::SemanticTokensOptions {
                            legend: toml_legend,
                            full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                            ..lsp::SemanticTokensOptions::default()
                        },
                    ),
                ),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let full_counter_toml_clone = full_counter_toml_clone.clone();
                move |fake_server| {
                    let full_counter = full_counter_toml_clone.clone();
                    fake_server
                        .set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
                            move |_, _| {
                                full_counter.fetch_add(1, atomic::Ordering::Release);
                                async move {
                                    Ok(Some(lsp::SemanticTokensResult::Tokens(
                                        lsp::SemanticTokens {
                                            // highlight 'a', 'b', 'c' as properties on lines 0, 1, 2
                                            data: vec![
                                                0, // delta_line (line 0)
                                                0, // delta_start
                                                1, // length
                                                0, // token_type
                                                0, // token_modifiers_bitset
                                                1, // delta_line (line 1)
                                                0, // delta_start
                                                1, // length
                                                0, // token_type
                                                0, // token_modifiers_bitset
                                                1, // delta_line (line 2)
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
    language_registry.add(toml_language.clone());
    let mut rust_server = language_registry.register_fake_lsp(
        rust_language.name(),
        FakeLspAdapter {
            name: "rust",
            capabilities: lsp::ServerCapabilities {
                semantic_tokens_provider: Some(
                    lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        lsp::SemanticTokensOptions {
                            legend: rust_legend,
                            full: Some(lsp::SemanticTokensFullOptions::Delta { delta: None }),
                            ..lsp::SemanticTokensOptions::default()
                        },
                    ),
                ),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );
    language_registry.add(rust_language.clone());

    app_state
        .fs
        .as_fake()
        .insert_tree(
            EditorLspTestContext::root_path(),
            json!({
                ".git": {},
                "dir": {
                    "foo.toml": "a = 1\nb = 2\nc = 3\n",
                    "bar.rs": "const c: usize = 3;\n",
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

    let toml_file = cx.read(|cx| workspace.file_project_paths(cx)[1].clone());
    let rust_file = cx.read(|cx| workspace.file_project_paths(cx)[0].clone());
    let (toml_item, rust_item) = workspace.update_in(cx, |workspace, window, cx| {
        (
            workspace.open_path(toml_file, None, true, window, cx),
            workspace.open_path(rust_file, None, true, window, cx),
        )
    });
    let toml_item = toml_item.await.expect("Could not open test file");
    let rust_item = rust_item.await.expect("Could not open test file");

    let (toml_editor, rust_editor) = cx.update(|_, cx| {
        (
            toml_item
                .act_as::<Editor>(cx)
                .expect("Opened test file wasn't an editor"),
            rust_item
                .act_as::<Editor>(cx)
                .expect("Opened test file wasn't an editor"),
        )
    });
    let toml_buffer = cx.read(|cx| {
        toml_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
    });
    let rust_buffer = cx.read(|cx| {
        rust_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
    });
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            toml_buffer.clone(),
            [Point::new(0, 0)..Point::new(0, 4)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            rust_buffer.clone(),
            [Point::new(0, 0)..Point::new(0, 4)],
            0,
            cx,
        );
        multibuffer
    });

    let editor = workspace.update_in(cx, |workspace, window, cx| {
        let editor = cx.new(|cx| build_editor_with_project(project, multibuffer, window, cx));
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        editor
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

    let _toml_server = toml_server.next().await.unwrap();
    let _rust_server = rust_server.next().await.unwrap();

    // Initial request.
    cx.executor().advance_clock(Duration::from_millis(200));
    let task = editor.update_in(cx, |e, _, _| e.semantic_token_state.take_update_task());
    cx.run_until_parked();
    task.await;
    assert_eq!(full_counter_toml.load(atomic::Ordering::Acquire), 1);
    cx.run_until_parked();

    // Initially, excerpt only covers line 0, so only the 'a' token should be highlighted.
    // The excerpt content is "a = 1\n" (6 chars), so 'a' is at offset 0.
    assert_eq!(
        extract_semantic_highlights(&editor, &cx),
        vec![MultiBufferOffset(0)..MultiBufferOffset(1)]
    );

    // Get the excerpt id for the TOML excerpt and expand it down by 2 lines.
    let toml_anchor = editor.read_with(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .snapshot(cx)
            .anchor_in_excerpt(text::Anchor::min_for_buffer(
                toml_buffer.read(cx).remote_id(),
            ))
            .unwrap()
    });
    editor.update_in(cx, |editor, _, cx| {
        editor.buffer().update(cx, |buffer, cx| {
            buffer.expand_excerpts([toml_anchor], 2, ExpandExcerptDirection::Down, cx);
        });
    });

    // Wait for semantic tokens to be re-fetched after expansion.
    cx.executor().advance_clock(Duration::from_millis(200));
    let task = editor.update_in(cx, |e, _, _| e.semantic_token_state.take_update_task());
    cx.run_until_parked();
    task.await;

    // After expansion, the excerpt covers lines 0-2, so 'a', 'b', 'c' should all be highlighted.
    // Content is now "a = 1\nb = 2\nc = 3\n" (18 chars).
    // 'a' at offset 0, 'b' at offset 6, 'c' at offset 12.
    assert_eq!(
        extract_semantic_highlights(&editor, &cx),
        vec![
            MultiBufferOffset(0)..MultiBufferOffset(1),
            MultiBufferOffset(6)..MultiBufferOffset(7),
            MultiBufferOffset(12)..MultiBufferOffset(13),
        ]
    );
}
