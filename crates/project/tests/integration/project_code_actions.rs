use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_code_actions_only_kinds(cx: &mut gpui::TestAppContext) {
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
                code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
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
    cx.executor().run_until_parked();

    let fake_server = fake_language_servers
        .next()
        .await
        .expect("failed to get the language server");

    let mut request_handled = fake_server
        .set_request_handler::<lsp::request::CodeActionRequest, _, _>(move |_, _| async move {
            Ok(Some(vec![
                lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
                    title: "organize imports".to_string(),
                    kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                    ..lsp::CodeAction::default()
                }),
                lsp::CodeActionOrCommand::CodeAction(lsp::CodeAction {
                    title: "fix code".to_string(),
                    kind: Some(CodeActionKind::SOURCE_FIX_ALL),
                    ..lsp::CodeAction::default()
                }),
            ]))
        });

    let code_actions_task = project.update(cx, |project, cx| {
        project.code_actions(
            &buffer,
            0..buffer.read(cx).len(),
            Some(vec![CodeActionKind::SOURCE_ORGANIZE_IMPORTS]),
            cx,
        )
    });

    let () = request_handled
        .next()
        .await
        .expect("The code action request should have been triggered");

    let code_actions = code_actions_task.await.unwrap().unwrap();
    assert_eq!(code_actions.len(), 1);
    assert_eq!(
        code_actions[0].lsp_action.action_kind(),
        Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS)
    );
}

#[gpui::test]
async fn test_code_actions_without_requested_kinds_do_not_send_only_filter(
    cx: &mut gpui::TestAppContext,
) {
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
                        code_action_kinds: Some(vec![
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                            "source.doc".into(),
                        ]),
                        ..lsp::CodeActionOptions::default()
                    },
                )),
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
    cx.executor().run_until_parked();

    let fake_server = fake_language_servers
        .next()
        .await
        .expect("failed to get the language server");

    let mut request_handled = fake_server.set_request_handler::<
        lsp::request::CodeActionRequest,
        _,
        _,
    >(move |params, _| async move {
        assert_eq!(
            params.context.only, None,
            "Code action requests without explicit kind filters should not send `context.only`"
        );
        Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
            lsp::CodeAction {
                title: "Add test".to_string(),
                kind: Some("source.addTest".into()),
                ..lsp::CodeAction::default()
            },
        )]))
    });

    let code_actions_task = project.update(cx, |project, cx| {
        project.code_actions(&buffer, 0..buffer.read(cx).len(), None, cx)
    });

    let () = request_handled
        .next()
        .await
        .expect("The code action request should have been triggered");

    let code_actions = code_actions_task.await.unwrap().unwrap();
    assert_eq!(code_actions.len(), 1);
    assert_eq!(
        code_actions[0].lsp_action.action_kind(),
        Some("source.addTest".into())
    );
}

#[gpui::test]
async fn test_multiple_language_server_actions(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "a.tsx": "a",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(tsx_lang());
    let language_server_names = [
        "TypeScriptServer",
        "TailwindServer",
        "ESLintServer",
        "NoActionsCapabilitiesServer",
    ];

    let mut language_server_rxs = [
        language_registry.register_fake_lsp(
            "tsx",
            FakeLspAdapter {
                name: language_server_names[0],
                capabilities: lsp::ServerCapabilities {
                    code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        ),
        language_registry.register_fake_lsp(
            "tsx",
            FakeLspAdapter {
                name: language_server_names[1],
                capabilities: lsp::ServerCapabilities {
                    code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        ),
        language_registry.register_fake_lsp(
            "tsx",
            FakeLspAdapter {
                name: language_server_names[2],
                capabilities: lsp::ServerCapabilities {
                    code_action_provider: Some(lsp::CodeActionProviderCapability::Simple(true)),
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        ),
        language_registry.register_fake_lsp(
            "tsx",
            FakeLspAdapter {
                name: language_server_names[3],
                capabilities: lsp::ServerCapabilities {
                    code_action_provider: None,
                    ..lsp::ServerCapabilities::default()
                },
                ..FakeLspAdapter::default()
            },
        ),
    ];

    let (buffer, _handle) = project
        .update(cx, |p, cx| {
            p.open_local_buffer_with_lsp(path!("/dir/a.tsx"), cx)
        })
        .await
        .unwrap();
    cx.executor().run_until_parked();

    let mut servers_with_actions_requests = HashMap::default();
    for i in 0..language_server_names.len() {
        let new_server = language_server_rxs[i].next().await.unwrap_or_else(|| {
            panic!(
                "Failed to get language server #{i} with name {}",
                &language_server_names[i]
            )
        });
        let new_server_name = new_server.server.name();

        assert!(
            !servers_with_actions_requests.contains_key(&new_server_name),
            "Unexpected: initialized server with the same name twice. Name: `{new_server_name}`"
        );
        match new_server_name.0.as_ref() {
            "TailwindServer" | "TypeScriptServer" => {
                servers_with_actions_requests.insert(
                    new_server_name.clone(),
                    new_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
                        move |_, _| {
                            let name = new_server_name.clone();
                            async move {
                                Ok(Some(vec![lsp::CodeActionOrCommand::CodeAction(
                                    lsp::CodeAction {
                                        title: format!("{name} code action"),
                                        ..lsp::CodeAction::default()
                                    },
                                )]))
                            }
                        },
                    ),
                );
            }
            "ESLintServer" => {
                servers_with_actions_requests.insert(
                    new_server_name,
                    new_server.set_request_handler::<lsp::request::CodeActionRequest, _, _>(
                        |_, _| async move { Ok(None) },
                    ),
                );
            }
            "NoActionsCapabilitiesServer" => {
                let _never_handled = new_server
                    .set_request_handler::<lsp::request::CodeActionRequest, _, _>(|_, _| async move {
                        panic!(
                            "Should not call for code actions server with no corresponding capabilities"
                        )
                    });
            }
            unexpected => panic!("Unexpected server name: {unexpected}"),
        }
    }

    let code_actions_task = project.update(cx, |project, cx| {
        project.code_actions(&buffer, 0..buffer.read(cx).len(), None, cx)
    });

    // cx.run_until_parked();
    let _: Vec<()> = futures::future::join_all(servers_with_actions_requests.into_values().map(
        |mut code_actions_request| async move {
            code_actions_request
                .next()
                .await
                .expect("All code actions requests should have been triggered")
        },
    ))
    .await;
    assert_eq!(
        vec!["TailwindServer code action", "TypeScriptServer code action"],
        code_actions_task
            .await
            .unwrap()
            .unwrap()
            .into_iter()
            .map(|code_action| code_action.lsp_action.title().to_owned())
            .sorted()
            .collect::<Vec<_>>(),
        "Should receive code actions responses from all related servers with hover capabilities"
    );
}
