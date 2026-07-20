use super::*;

fn set_linked_edit_ranges(
    opening: (Point, Point),
    closing: (Point, Point),
    editor: &mut Editor,
    cx: &mut Context<Editor>,
) {
    let Some((buffer, _)) = editor
        .buffer
        .read(cx)
        .text_anchor_for_position(editor.selections.newest_anchor().start, cx)
    else {
        panic!("Failed to get buffer for selection position");
    };
    let buffer = buffer.read(cx);
    let buffer_id = buffer.remote_id();
    let opening_range = buffer.anchor_before(opening.0)..buffer.anchor_after(opening.1);
    let closing_range = buffer.anchor_before(closing.0)..buffer.anchor_after(closing.1);
    let mut linked_ranges = HashMap::default();
    linked_ranges.insert(buffer_id, vec![(opening_range, vec![closing_range])]);
    editor.linked_edit_ranges = LinkedEditingRanges(linked_ranges);
}

#[gpui::test]
async fn test_html_linked_edits_on_completion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.html"), Default::default())
        .await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let html_language = Arc::new(Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    ));
    language_registry.add(html_language);
    let mut fake_servers = language_registry.register_fake_lsp(
        "HTML",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let worktree_id = workspace.update_in(cx, |workspace, _window, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/file.html"), cx)
        })
        .await
        .unwrap();
    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path((worktree_id, rel_path("file.html")), None, true, window, cx)
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.run_until_parked();
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text("<ad></ad>", window, cx);
        editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
            selections.select_ranges([Point::new(0, 3)..Point::new(0, 3)]);
        });
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 3)),
            (Point::new(0, 6), Point::new(0, 8)),
            editor,
            cx,
        );
    });
    let mut completion_handle =
        fake_server.set_request_handler::<lsp::request::Completion, _, _>(move |_, _| async move {
            Ok(Some(lsp::CompletionResponse::Array(vec![
                lsp::CompletionItem {
                    label: "head".to_string(),
                    text_edit: Some(lsp::CompletionTextEdit::InsertAndReplace(
                        lsp::InsertReplaceEdit {
                            new_text: "head".to_string(),
                            insert: lsp::Range::new(
                                lsp::Position::new(0, 1),
                                lsp::Position::new(0, 3),
                            ),
                            replace: lsp::Range::new(
                                lsp::Position::new(0, 1),
                                lsp::Position::new(0, 3),
                            ),
                        },
                    )),
                    ..Default::default()
                },
            ])))
        });
    editor.update_in(cx, |editor, window, cx| {
        editor.show_completions(&ShowCompletions, window, cx);
    });
    cx.run_until_parked();
    completion_handle.next().await.unwrap();
    editor.update(cx, |editor, _| {
        assert!(
            editor.context_menu_visible(),
            "Completion menu should be visible"
        );
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.confirm_completion(&ConfirmCompletion::default(), window, cx)
    });
    cx.executor().run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.text(cx), "<head></head>");
    });
}

#[gpui::test]
async fn test_linked_edits_on_typing_punctuation(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "TSX".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["tsx".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            linked_edit_characters: HashSet::from_iter(['.']),
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    // Test typing > does not extend linked pair
    cx.set_state("<divˇ<div></div>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 4)),
            (Point::new(0, 11), Point::new(0, 14)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(">", window, cx);
    });
    cx.assert_editor_state("<div>ˇ<div></div>");

    // Test typing . do extend linked pair
    cx.set_state("<Animatedˇ></Animated>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 9)),
            (Point::new(0, 12), Point::new(0, 20)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(".", window, cx);
    });
    cx.assert_editor_state("<Animated.ˇ></Animated.>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 10)),
            (Point::new(0, 13), Point::new(0, 21)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input("V", window, cx);
    });
    cx.assert_editor_state("<Animated.Vˇ></Animated.V>");
}

#[gpui::test]
async fn test_linked_edits_on_typing_dot_without_language_override(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorTestContext::new(cx).await;
    let language = Arc::new(Language::new(
        LanguageConfig {
            name: "HTML".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["html".to_string()],
                ..LanguageMatcher::default()
            },
            brackets: BracketPairConfig {
                pairs: vec![BracketPair {
                    start: "<".into(),
                    end: ">".into(),
                    close: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_html::LANGUAGE.into()),
    ));
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state("<Tableˇ></Table>");
    cx.update_editor(|editor, _, cx| {
        set_linked_edit_ranges(
            (Point::new(0, 1), Point::new(0, 6)),
            (Point::new(0, 9), Point::new(0, 14)),
            editor,
            cx,
        );
    });
    cx.update_editor(|editor, window, cx| {
        editor.handle_input(".", window, cx);
    });
    cx.assert_editor_state("<Table.ˇ></Table.>");
}

#[gpui::test]
async fn test_invisible_worktree_servers(cx: &mut TestAppContext) {
    init_test(cx, |_| {});

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "a": {
                "main.rs": "fn main() {}",
            },
            "foo": {
                "bar": {
                    "external_file.rs": "pub mod external {}",
                }
            }
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/root/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let _fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            ..FakeLspAdapter::default()
        },
    );
    let (multi_workspace, cx) =
        cx.add_window_view(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = multi_workspace.read_with(cx, |mw, _| mw.workspace().clone());
    let worktree_id = workspace.update(cx, |workspace, cx| {
        workspace.project().update(cx, |project, cx| {
            project.worktrees(cx).next().unwrap().read(cx).id()
        })
    });

    let assert_language_servers_count =
        |expected: usize, context: &str, cx: &mut VisualTestContext| {
            project.update(cx, |project, cx| {
                let current = project
                    .lsp_store()
                    .read(cx)
                    .as_local()
                    .unwrap()
                    .language_servers
                    .len();
                assert_eq!(expected, current, "{context}");
            });
        };

    assert_language_servers_count(
        0,
        "No servers should be running before any file is open",
        cx,
    );
    let pane = workspace.update(cx, |workspace, _| workspace.active_pane().clone());
    let main_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_path(
                (worktree_id, rel_path("main.rs")),
                Some(pane.downgrade()),
                true,
                window,
                cx,
            )
        })
        .unwrap()
        .await
        .downcast::<Editor>()
        .unwrap();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "fn main() {}",
                "Original main.rs text on initial open",
            );
        });
        assert_eq!(open_editor, main_editor);
    });
    assert_language_servers_count(1, "First *.rs file starts a language server", cx);

    let external_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from("/root/foo/bar/external_file.rs"),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .expect("opening external file")
        .downcast::<Editor>()
        .expect("downcasted external file's open element to editor");
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "pub mod external {}",
                "External file is open now",
            );
        });
        assert_eq!(open_editor, external_editor);
    });
    assert_language_servers_count(
        1,
        "Second, external, *.rs file should join the existing server",
        cx,
    );

    pane.update_in(cx, |pane, window, cx| {
        pane.close_active_item(&CloseActiveItem::default(), window, cx)
    })
    .await
    .unwrap();
    pane.update_in(cx, |pane, window, cx| {
        pane.navigate_backward(&Default::default(), window, cx);
    });
    cx.run_until_parked();
    pane.update(cx, |pane, cx| {
        let open_editor = pane.active_item().unwrap().downcast::<Editor>().unwrap();
        open_editor.update(cx, |editor, cx| {
            assert_eq!(
                editor.display_text(cx),
                "pub mod external {}",
                "External file is open now",
            );
        });
    });
    assert_language_servers_count(
        1,
        "After closing and reopening (with navigate back) of an external file, no extra language servers should appear",
        cx,
    );

    cx.update(|_, cx| {
        workspace::reload(cx);
    });
    assert_language_servers_count(
        1,
        "After reloading the worktree with local and external files opened, only one project should be started",
        cx,
    );
}
