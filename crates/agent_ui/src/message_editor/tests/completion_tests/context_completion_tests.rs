use super::*;

#[gpui::test]
async fn test_context_completion_provider_mentions(cx: &mut TestAppContext) {
    init_test(cx);

    let app_state = cx.update(AppState::test);

    cx.update(|cx| {
        editor::init(cx);
        workspace::init(app_state.clone(), cx);
    });

    app_state
        .fs
        .as_fake()
        .insert_tree(
            path!("/dir"),
            json!({
                "editor": "",
                "a": {
                    "one.txt": "1",
                    "two.txt": "2",
                    "three.txt": "3",
                    "four.txt": "4"
                },
                "b": {
                    "five.txt": "5",
                    "six.txt": "6",
                    "seven.txt": "7",
                    "eight.txt": "8",
                },
                "x.png": "",
            }),
        )
        .await;

    let project = Project::test(app_state.fs.clone(), [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();

    let worktree = project.update(cx, |project, cx| {
        let mut worktrees = project.worktrees(cx).collect::<Vec<_>>();
        assert_eq!(worktrees.len(), 1);
        worktrees.pop().unwrap()
    });
    let worktree_id = worktree.read_with(cx, |worktree, _| worktree.id());

    let mut cx = VisualTestContext::from_window(window.into(), cx);

    let paths = vec![
        rel_path("a/one.txt"),
        rel_path("a/two.txt"),
        rel_path("a/three.txt"),
        rel_path("a/four.txt"),
        rel_path("b/five.txt"),
        rel_path("b/six.txt"),
        rel_path("b/seven.txt"),
        rel_path("b/eight.txt"),
    ];

    let slash = PathStyle::local().primary_separator();

    let mut opened_editors = Vec::new();
    for path in paths {
        let buffer = workspace
            .update_in(&mut cx, |workspace, window, cx| {
                workspace.open_path(
                    ProjectPath {
                        worktree_id,
                        path: path.into(),
                    },
                    None,
                    false,
                    window,
                    cx,
                )
            })
            .await
            .unwrap();
        opened_editors.push(buffer);
    }

    let thread_store = cx.new(|cx| ThreadStore::new(cx));
    let session_capabilities = Arc::new(RwLock::new(SessionCapabilities::from_acp_commands(
        acp::PromptCapabilities::default(),
        vec![],
    )));

    let (message_editor, editor) = workspace.update_in(&mut cx, |workspace, window, cx| {
        let workspace_handle = cx.weak_entity();
        let message_editor = cx.new(|cx| {
            MessageEditor::new(
                workspace_handle,
                project.downgrade(),
                Some(thread_store),
                session_capabilities.clone(),
                "Test Agent".into(),
                "Test",
                EditorMode::AutoHeight {
                    max_lines: None,
                    min_lines: 1,
                },
                window,
                cx,
            )
        });
        workspace.active_pane().update(cx, |pane, cx| {
            pane.add_item(
                Box::new(cx.new(|_| MessageEditorItem(message_editor.clone()))),
                true,
                true,
                None,
                window,
                cx,
            );
        });
        message_editor.read(cx).focus_handle(cx).focus(window, cx);
        let editor = message_editor.read(cx).editor().clone();
        (message_editor, editor)
    });

    cx.simulate_input("Lorem @");

    editor.update_in(&mut cx, |editor, window, cx| {
        assert_eq!(editor.text(cx), "Lorem @");
        assert!(editor.has_visible_completions_menu());

        assert_eq!(
            current_completion_labels(editor),
            &[
                format!("eight.txt b{slash}"),
                format!("seven.txt b{slash}"),
                format!("six.txt b{slash}"),
                format!("five.txt b{slash}"),
                "Files & Directories".into(),
                "Symbols".into()
            ]
        );
        editor.set_text("", window, cx);
    });

    message_editor.update(&mut cx, |editor, _cx| {
        editor.session_capabilities.write().set_prompt_capabilities(
            acp::PromptCapabilities::new()
                .image(true)
                .audio(true)
                .embedded_context(true),
        );
    });

    cx.simulate_input("Lorem ");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "Lorem ");
        assert!(!editor.has_visible_completions_menu());
    });

    cx.simulate_input("@");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "Lorem @");
        assert!(editor.has_visible_completions_menu());
        assert_eq!(
            current_completion_labels(editor),
            &[
                format!("eight.txt b{slash}"),
                format!("seven.txt b{slash}"),
                format!("six.txt b{slash}"),
                format!("five.txt b{slash}"),
                "Files & Directories".into(),
                "Symbols".into(),
                "Threads".into(),
                "Fetch".into()
            ]
        );
    });

    // Select and confirm "File"
    editor.update_in(&mut cx, |editor, window, cx| {
        assert!(editor.has_visible_completions_menu());
        editor.context_menu_next(&editor::actions::ContextMenuNext, window, cx);
        editor.context_menu_next(&editor::actions::ContextMenuNext, window, cx);
        editor.context_menu_next(&editor::actions::ContextMenuNext, window, cx);
        editor.context_menu_next(&editor::actions::ContextMenuNext, window, cx);
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    cx.run_until_parked();

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "Lorem @file ");
        assert!(editor.has_visible_completions_menu());
    });

    cx.simulate_input("one");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(editor.text(cx), "Lorem @file one");
        assert!(editor.has_visible_completions_menu());
        assert_eq!(
            current_completion_labels(editor),
            vec![format!("one.txt a{slash}")]
        );
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        assert!(editor.has_visible_completions_menu());
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    let url_one = MentionUri::File {
        abs_path: path!("/dir/a/one.txt").into(),
    }
    .to_uri()
    .to_string();
    editor.update(&mut cx, |editor, cx| {
        let text = editor.text(cx);
        assert_eq!(text, format!("Lorem [@one.txt]({url_one}) "));
        assert!(!editor.has_visible_completions_menu());
        assert_eq!(fold_ranges(editor, cx).len(), 1);
    });

    let contents = message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .unwrap()
        .into_values()
        .collect::<Vec<_>>();

    {
        let [(uri, Mention::Text { content, .. })] = contents.as_slice() else {
            panic!("Unexpected mentions");
        };
        pretty_assertions::assert_eq!(content, "1");
        pretty_assertions::assert_eq!(
            uri,
            &MentionUri::parse(&url_one, PathStyle::local()).unwrap()
        );
    }

    cx.simulate_input(" ");

    editor.update(&mut cx, |editor, cx| {
        let text = editor.text(cx);
        assert_eq!(text, format!("Lorem [@one.txt]({url_one})  "));
        assert!(!editor.has_visible_completions_menu());
        assert_eq!(fold_ranges(editor, cx).len(), 1);
    });

    cx.simulate_input("Ipsum ");

    editor.update(&mut cx, |editor, cx| {
        let text = editor.text(cx);
        assert_eq!(text, format!("Lorem [@one.txt]({url_one})  Ipsum "),);
        assert!(!editor.has_visible_completions_menu());
        assert_eq!(fold_ranges(editor, cx).len(), 1);
    });

    cx.simulate_input("@file ");

    editor.update(&mut cx, |editor, cx| {
        let text = editor.text(cx);
        assert_eq!(text, format!("Lorem [@one.txt]({url_one})  Ipsum @file "),);
        assert!(editor.has_visible_completions_menu());
        assert_eq!(fold_ranges(editor, cx).len(), 1);
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    cx.run_until_parked();

    let contents = message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .unwrap()
        .into_values()
        .collect::<Vec<_>>();

    let url_eight = MentionUri::File {
        abs_path: path!("/dir/b/eight.txt").into(),
    }
    .to_uri()
    .to_string();

    {
        let [_, (uri, Mention::Text { content, .. })] = contents.as_slice() else {
            panic!("Unexpected mentions");
        };
        pretty_assertions::assert_eq!(content, "8");
        pretty_assertions::assert_eq!(
            uri,
            &MentionUri::parse(&url_eight, PathStyle::local()).unwrap()
        );
    }

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) ")
        );
        assert!(!editor.has_visible_completions_menu());
        assert_eq!(fold_ranges(editor, cx).len(), 2);
    });

    let plain_text_language = Arc::new(language::Language::new(
        language::LanguageConfig {
            name: "Plain Text".into(),
            matcher: language::LanguageMatcher {
                path_suffixes: vec!["txt".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        None,
    ));

    // Register the language and fake LSP
    let language_registry = project.read_with(&cx, |project, _| project.languages().clone());
    language_registry.add(plain_text_language);

    let mut fake_language_servers = language_registry.register_fake_lsp(
        "Plain Text",
        language::FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                workspace_symbol_provider: Some(lsp::OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    // Open the buffer to trigger LSP initialization
    let buffer = project
        .update(&mut cx, |project, cx| {
            project.open_local_buffer(path!("/dir/a/one.txt"), cx)
        })
        .await
        .unwrap();

    // Register the buffer with language servers
    let _handle = project.update(&mut cx, |project, cx| {
        project.register_buffer_with_language_servers(&buffer, cx)
    });

    cx.run_until_parked();

    let fake_language_server = fake_language_servers.next().await.unwrap();
    fake_language_server.set_request_handler::<lsp::WorkspaceSymbolRequest, _, _>(
        move |_, _| async move {
            Ok(Some(lsp::WorkspaceSymbolResponse::Flat(vec![
                #[allow(deprecated)]
                lsp::SymbolInformation {
                    name: "MySymbol".into(),
                    location: lsp::Location {
                        uri: lsp::Uri::from_file_path(path!("/dir/a/one.txt")).unwrap(),
                        range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 1)),
                    },
                    kind: lsp::SymbolKind::CONSTANT,
                    tags: None,
                    container_name: None,
                    deprecated: None,
                },
            ])))
        },
    );

    cx.simulate_input("@symbol ");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) @symbol ")
        );
        assert!(editor.has_visible_completions_menu());
        assert_eq!(current_completion_labels(editor), &["MySymbol one.txt L1"]);
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    let symbol = MentionUri::Symbol {
        abs_path: path!("/dir/a/one.txt").into(),
        name: "MySymbol".into(),
        line_range: 0..=0,
    };

    let contents = message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .unwrap()
        .into_values()
        .collect::<Vec<_>>();

    {
        let [_, _, (uri, Mention::Text { content, .. })] = contents.as_slice() else {
            panic!("Unexpected mentions");
        };
        pretty_assertions::assert_eq!(content, "1");
        pretty_assertions::assert_eq!(uri, &symbol);
    }

    cx.run_until_parked();

    editor.read_with(&cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!(
                "Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) [@MySymbol]({}) ",
                symbol.to_uri(),
            )
        );
    });

    // Try to mention an "image" file that will fail to load
    cx.simulate_input("@file x.png");

    editor.update(&mut cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!("Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) [@MySymbol]({}) @file x.png", symbol.to_uri())
        );
        assert!(editor.has_visible_completions_menu());
        assert_eq!(current_completion_labels(editor), &["x.png "]);
    });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    // Getting the message contents fails
    message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .expect_err("Should fail to load x.png");

    cx.run_until_parked();

    // Mention was removed
    editor.read_with(&cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!(
                "Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) [@MySymbol]({}) ",
                symbol.to_uri()
            )
        );
    });

    // Once more
    cx.simulate_input("@file x.png");

    editor.update(&mut cx, |editor, cx| {
                assert_eq!(
                    editor.text(cx),
                    format!("Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) [@MySymbol]({}) @file x.png", symbol.to_uri())
                );
                assert!(editor.has_visible_completions_menu());
                assert_eq!(current_completion_labels(editor), &["x.png "]);
            });

    editor.update_in(&mut cx, |editor, window, cx| {
        editor.confirm_completion(&editor::actions::ConfirmCompletion::default(), window, cx);
    });

    // This time don't immediately get the contents, just let the confirmed completion settle
    cx.run_until_parked();

    // Mention was removed
    editor.read_with(&cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            format!(
                "Lorem [@one.txt]({url_one})  Ipsum [@eight.txt]({url_eight}) [@MySymbol]({}) ",
                symbol.to_uri()
            )
        );
    });

    // Now getting the contents succeeds, because the invalid mention was removed
    let contents = message_editor
        .update(&mut cx, |message_editor, cx| {
            message_editor
                .mention_set()
                .update(cx, |mention_set, cx| mention_set.contents(false, cx))
        })
        .await
        .unwrap();
    assert_eq!(contents.len(), 3);
}

fn fold_ranges(editor: &Editor, cx: &mut App) -> Vec<Range<Point>> {
    let snapshot = editor.buffer().read(cx).snapshot(cx);
    editor.display_map.update(cx, |display_map, cx| {
        display_map
            .snapshot(cx)
            .folds_in_range(MultiBufferOffset(0)..snapshot.len())
            .map(|fold| fold.range.to_point(&snapshot))
            .collect()
    })
}

fn current_completion_labels(editor: &Editor) -> Vec<String> {
    let completions = editor.current_completions().expect("Missing completions");
    completions
        .into_iter()
        .map(|completion| completion.label.text)
        .collect::<Vec<_>>()
}

fn current_completion_labels_with_documentation(editor: &Editor) -> Vec<(String, String)> {
    let completions = editor.current_completions().expect("Missing completions");
    completions
        .into_iter()
        .map(|completion| {
            (
                completion.label.text,
                completion
                    .documentation
                    .map(|d| d.text().to_string())
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>()
}
