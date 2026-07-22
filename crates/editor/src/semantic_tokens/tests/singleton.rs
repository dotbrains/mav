use super::*;

#[gpui::test]
async fn lsp_semantic_tokens_singleton_opened_from_multibuffer(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    update_test_language_settings(cx, &|language_settings| {
        language_settings.languages.0.insert(
            "Rust".into(),
            LanguageSettingsContent {
                semantic_tokens: Some(SemanticTokens::Full),
                ..LanguageSettingsContent::default()
            },
        );
    });

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

    let rust_legend = lsp::SemanticTokensLegend {
        token_types: vec!["function".into()],
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
            initializer: Some(Box::new(move |fake_server| {
                fake_server.set_request_handler::<lsp::request::SemanticTokensFullRequest, _, _>(
                    move |_, _| async move {
                        Ok(Some(lsp::SemanticTokensResult::Tokens(
                            lsp::SemanticTokens {
                                data: vec![0, 3, 4, 0, 0],
                                result_id: None,
                            },
                        )))
                    },
                );
            })),
            ..FakeLspAdapter::default()
        },
    );
    language_registry.add(rust_language.clone());

    // foo.rs must be long enough that autoscroll triggers an actual scroll
    // position change when opening from the multibuffer with cursor near
    // the end. This reproduces the race: set_visible_line_count spawns a
    // task, then autoscroll fires ScrollPositionChanged whose handler
    // replaces post_scroll_update with a debounced task that skips
    // update_lsp_data for singletons.
    let mut foo_content = String::from("fn test() {}\n");
    for i in 0..100 {
        foo_content.push_str(&format!("fn func_{i}() {{}}\n"));
    }

    app_state
        .fs
        .as_fake()
        .insert_tree(
            EditorLspTestContext::root_path(),
            json!({
                ".git": {},
                "bar.rs": "fn main() {}\n",
                "foo.rs": foo_content,
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

    // Open bar.rs as an editor to start the LSP server.
    let bar_file = cx.read(|cx| workspace.file_project_paths(cx)[0].clone());
    let bar_item = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(bar_file, None, true, window, cx)
        })
        .await
        .expect("Could not open bar.rs");
    let bar_editor = cx.update(|_, cx| {
        bar_item
            .act_as::<Editor>(cx)
            .expect("Opened test file wasn't an editor")
    });
    let bar_buffer = cx.read(|cx| {
        bar_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap()
    });

    let _rust_server = rust_server.next().await.unwrap();

    cx.executor().advance_clock(Duration::from_millis(200));
    let task = bar_editor.update_in(cx, |e, _, _| e.semantic_token_state.take_update_task());
    cx.run_until_parked();
    task.await;
    cx.run_until_parked();

    assert!(
        !extract_semantic_highlights(&bar_editor, &cx).is_empty(),
        "bar.rs should have semantic tokens after initial open"
    );

    // Get foo.rs buffer directly from the project. No editor has ever
    // fetched semantic tokens for this buffer.
    let foo_file = cx.read(|cx| workspace.file_project_paths(cx)[1].clone());
    let foo_buffer = project
        .update(cx, |project, cx| project.open_buffer(foo_file, cx))
        .await
        .expect("Could not open foo.rs buffer");

    // Build a multibuffer with both files. The foo.rs excerpt covers a
    // range near the end of the file so that opening the singleton will
    // autoscroll to a position that requires changing scroll_position.
    let multibuffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            bar_buffer.clone(),
            [Point::new(0, 0)..Point::new(0, 12)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            foo_buffer.clone(),
            [Point::new(95, 0)..Point::new(100, 0)],
            0,
            cx,
        );
        multibuffer
    });

    let mb_editor = workspace.update_in(cx, |workspace, window, cx| {
        let editor =
            cx.new(|cx| build_editor_with_project(project.clone(), multibuffer, window, cx));
        workspace.add_item_to_active_pane(Box::new(editor.clone()), None, true, window, cx);
        editor
    });
    mb_editor.update_in(cx, |editor, window, cx| {
        let nav_history = workspace
            .read(cx)
            .active_pane()
            .read(cx)
            .nav_history_for_item(&cx.entity());
        editor.set_nav_history(Some(nav_history));
        window.focus(&editor.focus_handle(cx), cx)
    });

    // Close bar.rs tab so only the multibuffer remains.
    workspace
        .update_in(cx, |workspace, window, cx| {
            let pane = workspace.active_pane().clone();
            pane.update(cx, |pane, cx| {
                pane.close_item_by_id(
                    bar_editor.entity_id(),
                    workspace::SaveIntent::Skip,
                    window,
                    cx,
                )
            })
        })
        .await
        .ok();

    cx.run_until_parked();

    // Position cursor in the foo.rs excerpt (near line 95+).
    mb_editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.display_snapshot(cx);
        let end = snapshot.buffer_snapshot().len();
        editor.change_selections(None.into(), window, cx, |s| {
            s.select_ranges([end..end]);
        });
    });

    // Open the singleton from the multibuffer. open_buffers_in_workspace
    // creates the editor and calls change_selections with autoscroll.
    // During render, set_visible_line_count fires first (spawning a task),
    // then autoscroll_vertically scrolls to line ~95 which emits
    // ScrollPositionChanged, whose handler replaces post_scroll_update.
    mb_editor.update_in(cx, |editor, window, cx| {
        editor.open_excerpts(&crate::actions::OpenExcerpts, window, cx);
    });

    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();

    let active_editor = workspace.read_with(cx, |workspace, cx| {
        workspace
            .active_item(cx)
            .and_then(|item| item.act_as::<Editor>(cx))
            .expect("Active item should be an editor")
    });

    assert!(
        active_editor.read_with(cx, |editor, cx| editor.buffer().read(cx).is_singleton()),
        "Active editor should be a singleton buffer"
    );

    // Wait for semantic tokens on the singleton.
    cx.executor().advance_clock(Duration::from_millis(200));
    let task = active_editor.update_in(cx, |e, _, _| e.semantic_token_state.take_update_task());
    task.await;
    cx.run_until_parked();

    let highlights = extract_semantic_highlights(&active_editor, &cx);
    assert!(
        !highlights.is_empty(),
        "Singleton editor opened from multibuffer should have semantic tokens"
    );
}
