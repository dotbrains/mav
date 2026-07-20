use super::*;

#[gpui::test]
async fn test_multiple_formatters(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.remove_trailing_whitespace_on_save = Some(true);
        settings.defaults.formatter = Some(FormatterList::Vec(vec![
            Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current),
            Formatter::CodeAction("code-action-1".into()),
            Formatter::CodeAction("code-action-2".into()),
        ]))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.rs"), "one  \ntwo   \nthree".into())
        .await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                document_formatting_provider: Some(lsp::OneOf::Left(true)),
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: vec!["the-command-for-code-action-1".into()],
                    ..Default::default()
                }),
                code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.rs"), cx)
        })
        .await
        .unwrap();

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });

    let fake_server = fake_servers.next().await.unwrap();
    fake_server.set_request_handler::<lsp::request::Formatting, _, _>(
        move |_params, _| async move {
            Ok(Some(vec![lsp::TextEdit::new(
                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                "applied-formatting\n".to_string(),
            )]))
        },
    );
    fake_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |params, _| async move {
            let requested_code_actions = params.context.only.expect("Expected code action request");
            assert_eq!(requested_code_actions.len(), 1);

            let uri = lsp::Uri::from_file_path(path!("/file.rs")).unwrap();
            let code_action = match requested_code_actions[0].as_str() {
                "code-action-1" => lsp::CodeAction {
                    kind: Some("code-action-1".into()),
                    edit: Some(lsp::WorkspaceEdit::new(
                        [(
                            uri,
                            vec![lsp::TextEdit::new(
                                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                                "applied-code-action-1-edit\n".to_string(),
                            )],
                        )]
                        .into_iter()
                        .collect(),
                    )),
                    command: Some(lsp::Command {
                        command: "the-command-for-code-action-1".into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                "code-action-2" => lsp::CodeAction {
                    kind: Some("code-action-2".into()),
                    edit: Some(lsp::WorkspaceEdit::new(
                        [(
                            uri,
                            vec![lsp::TextEdit::new(
                                lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)),
                                "applied-code-action-2-edit\n".to_string(),
                            )],
                        )]
                        .into_iter()
                        .collect(),
                    )),
                    ..Default::default()
                },
                req => panic!("Unexpected code action request: {:?}", req),
            };
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                code_action,
            )]))
        },
    );

    fake_server.set_request_handler::<lsp::request::CodeActionResolveRequest, _, _>({
        move |params, _| async move { Ok(params) }
    });

    let command_lock = Arc::new(futures::lock::Mutex::new(()));
    fake_server.set_request_handler::<lsp::request::ExecuteCommand, _, _>({
        let fake = fake_server.clone();
        let lock = command_lock.clone();
        move |params, _| {
            assert_eq!(params.command, "the-command-for-code-action-1");
            let fake = fake.clone();
            let lock = lock.clone();
            async move {
                lock.lock().await;
                fake.server
                    .request::<lsp::request::ApplyWorkspaceEdit>(
                        lsp::ApplyWorkspaceEditParams {
                            label: None,
                            edit: lsp::WorkspaceEdit {
                                changes: Some(
                                    [(
                                        lsp::Uri::from_file_path(path!("/file.rs")).unwrap(),
                                        vec![lsp::TextEdit {
                                            range: lsp::Range::new(
                                                lsp::Position::new(0, 0),
                                                lsp::Position::new(0, 0),
                                            ),
                                            new_text: "applied-code-action-1-command\n".into(),
                                        }],
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                                ..Default::default()
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
    });

    editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap()
        .await;
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            r#"
                applied-code-action-2-edit
                applied-code-action-1-command
                applied-code-action-1-edit
                applied-formatting
                one
                two
                three
            "#
            .unindent()
        );
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.undo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "one  \ntwo   \nthree");
    });

    // Perform a manual edit while waiting for an LSP command
    // that's being run as part of a formatting code action.
    let lock_guard = command_lock.lock().await;
    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_format(
                project.clone(),
                FormatTrigger::Manual,
                FormatTarget::Buffers(editor.buffer().read(cx).all_buffers()),
                window,
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.text(cx),
            r#"
                applied-code-action-1-edit
                applied-formatting
                one
                two
                three
            "#
            .unindent()
        );

        editor.buffer.update(cx, |buffer, cx| {
            let ix = buffer.len(cx);
            buffer.edit([(ix..ix, "edited\n")], None, cx);
        });
    });

    // Allow the LSP command to proceed. Because the buffer was edited,
    // the second code action will not be run.
    drop(lock_guard);
    format.await;
    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(
            editor.text(cx),
            r#"
                applied-code-action-1-command
                applied-code-action-1-edit
                applied-formatting
                one
                two
                three
                edited
            "#
            .unindent()
        );

        // The manual edit is undone first, because it is the last thing the user did
        // (even though the command completed afterwards).
        editor.undo(&Default::default(), window, cx);
        assert_eq!(
            editor.text(cx),
            r#"
                applied-code-action-1-command
                applied-code-action-1-edit
                applied-formatting
                one
                two
                three
            "#
            .unindent()
        );

        // All the formatting (including the command, which completed after the manual edit)
        // is undone together.
        editor.undo(&Default::default(), window, cx);
        assert_eq!(editor.text(cx), "one  \ntwo   \nthree");
    });
}

#[gpui::test]
async fn test_organize_imports_manual_trigger(cx: &mut TestAppContext) {
    init_test(cx, |settings| {
        settings.defaults.formatter = Some(FormatterList::Vec(vec![Formatter::LanguageServer(
            settings::LanguageServerFormatterSpecifier::Current,
        )]))
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_file(path!("/file.ts"), Default::default()).await;

    let project = Project::test(fs, [path!("/").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(Arc::new(Language::new(
        LanguageConfig {
            name: "TypeScript".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["ts".to_string()],
                ..Default::default()
            },
            ..LanguageConfig::default()
        },
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
    )));
    update_test_language_settings(cx, &|settings| {
        settings.defaults.prettier.get_or_insert_default().allowed = Some(true);
    });
    let mut fake_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/file.ts"), cx)
        })
        .await
        .unwrap();

    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let (editor, cx) = cx.add_window_view(|window, cx| {
        build_editor_with_project(project.clone(), buffer, window, cx)
    });
    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(
            "import { a } from 'module';\nimport { b } from 'module';\n\nconst x = a;\n",
            window,
            cx,
        )
    });

    let fake_server = fake_servers.next().await.unwrap();

    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_code_action_kind(
                project.clone(),
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                window,
                cx,
            )
        })
        .unwrap();
    fake_server
        .set_request_handler::<lsp::request::CodeActionRequest, _, _>(move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.ts")).unwrap()
            );
            Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                lsp::CodeAction {
                    title: "Organize Imports".to_string(),
                    kind: Some(lsp::CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                    edit: Some(lsp::WorkspaceEdit {
                        changes: Some(
                            [(
                                params.text_document.uri.clone(),
                                vec![lsp::TextEdit::new(
                                    lsp::Range::new(
                                        lsp::Position::new(1, 0),
                                        lsp::Position::new(2, 0),
                                    ),
                                    "".to_string(),
                                )],
                            )]
                            .into_iter()
                            .collect(),
                        ),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            )]))
        })
        .next()
        .await;
    format.await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "import { a } from 'module';\n\nconst x = a;\n"
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.set_text(
            "import { a } from 'module';\nimport { b } from 'module';\n\nconst x = a;\n",
            window,
            cx,
        )
    });
    // Ensure we don't lock if code action hangs.
    fake_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
        move |params, _| async move {
            assert_eq!(
                params.text_document.uri,
                lsp::Uri::from_file_path(path!("/file.ts")).unwrap()
            );
            futures::future::pending::<()>().await;
            unreachable!()
        },
    );
    let format = editor
        .update_in(cx, |editor, window, cx| {
            editor.perform_code_action_kind(
                project,
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                window,
                cx,
            )
        })
        .unwrap();
    cx.executor().advance_clock(super::CODE_ACTION_TIMEOUT);
    format.await;
    assert_eq!(
        editor.update(cx, |editor, cx| editor.text(cx)),
        "import { a } from 'module';\nimport { b } from 'module';\n\nconst x = a;\n"
    );
}
