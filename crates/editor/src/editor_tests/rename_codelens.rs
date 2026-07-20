use super::*;

#[gpui::test]
async fn test_rename_with_duplicate_edits(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Fˇoo {}
    "});

    cx.update_editor(|editor, _, cx| {
        let highlight_range = Point::new(0, 7)..Point::new(0, 10);
        let highlight_range = highlight_range.to_anchors(&editor.buffer().read(cx).snapshot(cx));
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[highlight_range],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    let mut prepare_rename_handler = cx
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
            move |_, _, _| async move {
                Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                })))
            },
        );
    let prepare_rename_task = cx
        .update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started");
    prepare_rename_handler.next().await.unwrap();
    prepare_rename_task.await.expect("Prepare rename failed");

    let mut rename_handler =
        cx.set_request_handler::<lsp::request::Rename, _, _>(move |url, _, _| async move {
            let edit = lsp::TextEdit {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                },
                new_text: "FooRenamed".to_string(),
            };
            Ok(Some(lsp::WorkspaceEdit::new(
                // Specify the same edit twice
                std::collections::HashMap::from_iter(Some((url, vec![edit.clone(), edit]))),
            )))
        });
    let rename_task = cx
        .update_editor(|e, window, cx| e.confirm_rename(&ConfirmRename, window, cx))
        .expect("Confirm rename was not started");
    rename_handler.next().await.unwrap();
    rename_task.await.expect("Confirm rename failed");
    cx.run_until_parked();

    // Despite two edits, only one is actually applied as those are identical
    cx.assert_editor_state(indoc! {"
        struct FooRenamedˇ {}
    "});
}

#[gpui::test]
async fn test_rename_with_out_of_order_document_highlights(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Right(lsp::RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Foo {}
        fn main() {
            let first = Foo {};
            let second = Fˇoo {};
        }
    "});

    cx.update_editor(|editor, _window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let read_highlight = (Point::new(2, 16)..Point::new(2, 19)).to_anchors(&snapshot);
        let write_highlight = (Point::new(3, 17)..Point::new(3, 20)).to_anchors(&snapshot);
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[read_highlight],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
        editor.highlight_background(
            HighlightKey::DocumentHighlightWrite,
            &[write_highlight],
            |_, theme| theme.colors().editor_document_highlight_write_background,
            cx,
        );
    });

    let mut prepare_rename_handler = cx
        .set_request_handler::<lsp::request::PrepareRenameRequest, _, _>(
            move |_, _, _| async move {
                Ok(Some(lsp::PrepareRenameResponse::Range(lsp::Range {
                    start: lsp::Position {
                        line: 3,
                        character: 17,
                    },
                    end: lsp::Position {
                        line: 3,
                        character: 20,
                    },
                })))
            },
        );
    let prepare_rename_task = cx
        .update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started");
    prepare_rename_handler.next().await.unwrap();
    prepare_rename_task.await.expect("Prepare rename failed");

    cx.update_editor(|editor, window, cx| {
        editor
            .snapshot(window, cx)
            .layout_row(DisplayRow(2), &editor.text_layout_details(window, cx));
    });
}

#[gpui::test]
async fn test_rename_without_prepare(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    // These capabilities indicate that the server does not support prepare rename.
    let capabilities = lsp::ServerCapabilities {
        rename_provider: Some(lsp::OneOf::Left(true)),
        ..Default::default()
    };
    let mut cx = EditorLspTestContext::new_rust(capabilities, cx).await;

    cx.set_state(indoc! {"
        struct Fˇoo {}
    "});

    cx.update_editor(|editor, _window, cx| {
        let highlight_range = Point::new(0, 7)..Point::new(0, 10);
        let highlight_range = highlight_range.to_anchors(&editor.buffer().read(cx).snapshot(cx));
        editor.highlight_background(
            HighlightKey::DocumentHighlightRead,
            &[highlight_range],
            |_, theme| theme.colors().editor_document_highlight_read_background,
            cx,
        );
    });

    cx.update_editor(|e, window, cx| e.rename(&Rename, window, cx))
        .expect("Prepare rename was not started")
        .await
        .expect("Prepare rename failed");

    let mut rename_handler =
        cx.set_request_handler::<lsp::request::Rename, _, _>(move |url, _, _| async move {
            let edit = lsp::TextEdit {
                range: lsp::Range {
                    start: lsp::Position {
                        line: 0,
                        character: 7,
                    },
                    end: lsp::Position {
                        line: 0,
                        character: 10,
                    },
                },
                new_text: "FooRenamed".to_string(),
            };
            Ok(Some(lsp::WorkspaceEdit::new(
                std::collections::HashMap::from_iter(Some((url, vec![edit]))),
            )))
        });
    let rename_task = cx
        .update_editor(|e, window, cx| e.confirm_rename(&ConfirmRename, window, cx))
        .expect("Confirm rename was not started");
    rename_handler.next().await.unwrap();
    rename_task.await.expect("Confirm rename failed");
    cx.run_until_parked();

    // Correct range is renamed, as `surrounding_word` is used to find it.
    cx.assert_editor_state(indoc! {"
        struct FooRenamedˇ {}
    "});
}

#[gpui::test]
async fn test_tree_sitter_brackets_newline_insertion(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorTestContext::new(cx).await;

    let language = Arc::new(
        Language::new(
            LanguageConfig::default(),
            Some(tree_sitter_html::LANGUAGE.into()),
        )
        .with_brackets_query(
            r#"
            ("<" @open "/>" @close)
            ("</" @open ">" @close)
            ("<" @open ">" @close)
            ("\"" @open "\"" @close)
            ((element (start_tag) @open (end_tag) @close) (#set! newline.only))
        "#,
        )
        .unwrap(),
    );
    cx.update_buffer(|buffer, cx| buffer.set_language(Some(language), cx));

    cx.set_state(indoc! {"
        <span>ˇ</span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span>
        ˇ
        </span>
    "});

    cx.set_state(indoc! {"
        <span><span></span>ˇ</span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span><span></span>
        ˇ</span>
    "});

    cx.set_state(indoc! {"
        <span>ˇ
        </span>
    "});
    cx.update_editor(|e, window, cx| e.newline(&Newline, window, cx));
    cx.assert_editor_state(indoc! {"
        <span>
        ˇ
        </span>
    "});
}

#[gpui::test(iterations = 10)]
async fn test_apply_code_lens_actions_with_commands(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    update_test_editor_settings(cx, &|settings| {
        settings.code_lens = Some(settings::CodeLens::Menu);
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "a",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(*window, cx);

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..Default::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                code_lens_provider: Some(lsp::CodeLensOptions {
                    resolve_provider: Some(true),
                }),
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: vec!["_the/command".to_string()],
                    ..lsp::ExecuteCommandOptions::default()
                }),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );

    let editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/dir/a.ts")),
                OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx.executor().run_until_parked();

    let fake_server = fake_language_servers.next().await.unwrap();

    let buffer = editor.update(cx, |editor, cx| {
        editor
            .buffer()
            .read(cx)
            .as_singleton()
            .expect("have opened a single file by path")
    });

    let buffer_snapshot = buffer.update(cx, |buffer, _| buffer.snapshot());
    let anchor = buffer_snapshot.anchor_at(0, text::Bias::Left);
    drop(buffer_snapshot);
    let actions = cx
        .update_window(*window, |_, window, cx| {
            project.code_actions(&buffer, anchor..anchor, window, cx)
        })
        .unwrap();

    fake_server
        .set_request_handler::<lsp::request::CodeLensRequest, _, _>(|_, _| async move {
            Ok(Some(vec![
                lsp::CodeLens {
                    range: lsp::Range::default(),
                    command: Some(lsp::Command {
                        title: "Code lens command".to_owned(),
                        command: "_the/command".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
                lsp::CodeLens {
                    range: lsp::Range {
                        start: lsp::Position {
                            line: 1,
                            character: 1,
                        },
                        end: lsp::Position {
                            line: 1,
                            character: 1,
                        },
                    },
                    command: Some(lsp::Command {
                        title: "Command not in range".to_owned(),
                        command: "_the/command".to_owned(),
                        arguments: None,
                    }),
                    data: None,
                },
            ]))
        })
        .next()
        .await;

    let actions = actions.await.unwrap();
    assert_eq!(
        actions.len(),
        1,
        "Should have only one valid action for the 0..0 range, got: {actions:#?}"
    );
    let action = actions[0].clone();
    let apply = project.update(cx, |project, cx| {
        project.apply_code_action(buffer.clone(), action, true, cx)
    });

    // Resolving the code action does not populate its edits. In absence of
    // edits, we must execute the given command.
    fake_server.set_request_handler::<lsp::request::CodeLensResolve, _, _>(
        |mut lens, _| async move {
            let lens_command = lens.command.as_mut().expect("should have a command");
            assert_eq!(lens_command.title, "Code lens command");
            lens_command.arguments = Some(vec![json!("the-argument")]);
            Ok(lens)
        },
    );

    // While executing the command, the language server sends the editor
    // a `workspaceEdit` request.
    fake_server
        .set_request_handler::<lsp::request::ExecuteCommand, _, _>({
            let fake = fake_server.clone();
            move |params, _| {
                assert_eq!(params.command, "_the/command");
                let fake = fake.clone();
                async move {
                    fake.server
                        .request::<lsp::request::ApplyWorkspaceEdit>(
                            lsp::ApplyWorkspaceEditParams {
                                label: None,
                                edit: lsp::WorkspaceEdit {
                                    changes: Some(
                                        [(
                                            lsp::Uri::from_file_path(path!("/dir/a.ts")).unwrap(),
                                            vec![lsp::TextEdit {
                                                range: lsp::Range::new(
                                                    lsp::Position::new(0, 0),
                                                    lsp::Position::new(0, 0),
                                                ),
                                                new_text: "X".into(),
                                            }],
                                        )]
                                        .into_iter()
                                        .collect(),
                                    ),
                                    ..lsp::WorkspaceEdit::default()
                                },
                            },
                            DEFAULT_LSP_REQUEST_TIMEOUT,
                        )
                        .await
                        .into_response()
                        .unwrap();
                    Ok(Some(json!(null)))
                }
            }
        })
        .next()
        .await;

    // Applying the code lens command returns a project transaction containing the edits
    // sent by the language server in its `workspaceEdit` request.
    let transaction = apply.await.unwrap();
    assert!(transaction.0.contains_key(&buffer));
    buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "Xa");
        buffer.undo(cx);
        assert_eq!(buffer.text(), "a");
    });

    let actions_after_edits = cx
        .update(|window, cx| project.code_actions(&buffer, anchor..anchor, window, cx))
        .unwrap()
        .await;
    assert_eq!(
        actions, actions_after_edits,
        "For the same selection, same code lens actions should be returned"
    );

    let _responses =
        fake_server.set_request_handler::<lsp::request::CodeLensRequest, _, _>(|_, _| async move {
            panic!("No more code lens requests are expected");
        });
    editor.update_in(cx, |editor, window, cx| {
        editor.select_all(&SelectAll, window, cx);
    });
    cx.executor().run_until_parked();
    let new_actions = cx
        .update(|window, cx| project.code_actions(&buffer, anchor..anchor, window, cx))
        .unwrap()
        .await;
    assert_eq!(
        actions, new_actions,
        "Code lens are queried for the same range and should get the same set back, but without additional LSP queries now"
    );
}
