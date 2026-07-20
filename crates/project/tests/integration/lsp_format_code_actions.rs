use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_supports_range_formatting_ignores_unrelated_language_servers(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.formatter = Some(FormatterList::Single(
                    Formatter::LanguageServer(settings::LanguageServerFormatterSpecifier::Current),
                ));
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "",
            "b.rs": "",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    language_registry.add(rust_lang());

    let mut typescript_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            name: "typescript-fake-language-server",
            capabilities: lsp::ServerCapabilities {
                document_range_formatting_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );
    let mut rust_language_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-fake-language-server",
            capabilities: lsp::ServerCapabilities {
                document_formatting_provider: Some(lsp::OneOf::Left(true)),
                document_range_formatting_provider: Some(lsp::OneOf::Left(false)),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );

    let (typescript_buffer, _typescript_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();
    let (rust_buffer, _rust_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/b.rs"), cx)
        })
        .await
        .unwrap();

    let _typescript_language_server = typescript_language_servers.next().await.unwrap();
    let _rust_language_server = rust_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    assert!(project.read_with(cx, |project, cx| {
        project.supports_range_formatting(&typescript_buffer, cx)
    }));
    assert!(!project.read_with(cx, |project, cx| {
        project.supports_range_formatting(&rust_buffer, cx)
    }));
}

#[gpui::test(iterations = 10)]
async fn test_apply_code_actions_with_commands(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.ts": "a",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(typescript_lang());
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "TypeScript",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                code_action_provider: Some(lsp::CodeActionProviderCapability::Options(
                    lsp::CodeActionOptions {
                        resolve_provider: Some(true),
                        ..lsp::CodeActionOptions::default()
                    },
                )),
                execute_command_provider: Some(lsp::ExecuteCommandOptions {
                    commands: vec!["_the/command".to_string()],
                    ..lsp::ExecuteCommandOptions::default()
                }),
                ..lsp::ServerCapabilities::default()
            },
            ..FakeLspAdapter::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.ts"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_language_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    // Language server returns code actions that contain commands, and not edits.
    let actions = project.update(cx, |project, cx| {
        project.code_actions(&buffer, 0..0, None, cx)
    });
    fake_server
        .set_request_handler::<lsp::request::CodeActionRequest, _, _>(|_, _| async move {
            Ok(Some(vec![
                lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
                    title: "The code action".into(),
                    data: Some(serde_json::json!({
                        "command": "_the/command",
                    })),
                    ..lsp::CodeAction::default()
                }),
                lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
                    title: "two".into(),
                    ..lsp::CodeAction::default()
                }),
            ]))
        })
        .next()
        .await;

    let action = actions.await.unwrap().unwrap()[0].clone();
    let apply = project.update(cx, |project, cx| {
        project.apply_code_action(buffer.clone(), action, true, cx)
    });

    // Resolving the code action does not populate its edits. In absence of
    // edits, we must execute the given command.
    fake_server.set_request_handler::<lsp::request::CodeActionResolveRequest, _, _>(
        |mut action, _| async move {
            if action.data.is_some() {
                action.command = Some(lsp::Command {
                    title: "The command".into(),
                    command: "_the/command".into(),
                    arguments: Some(vec![json!("the-argument")]),
                });
            }
            Ok(action)
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
        })
        .next()
        .await;

    // Applying the code action returns a project transaction containing the edits
    // sent by the language server in its `workspaceEdit` request.
    let transaction = apply.await.unwrap();
    assert!(transaction.0.contains_key(&buffer));
    buffer.update(cx, |buffer, cx| {
        assert_eq!(buffer.text(), "Xa");
        buffer.undo(cx);
        assert_eq!(buffer.text(), "a");
    });
}
