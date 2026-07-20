use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_multiple_language_server_hovers(cx: &mut gpui::TestAppContext) {
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
        "NoHoverCapabilitiesServer",
    ];
    let mut language_servers = [
        language_registry.register_fake_lsp(
            "tsx",
            FakeLspAdapter {
                name: language_server_names[0],
                capabilities: lsp::ServerCapabilities {
                    hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
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
                    hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
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
                    hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
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
                    hover_provider: None,
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

    let mut servers_with_hover_requests = HashMap::default();
    for i in 0..language_server_names.len() {
        let new_server = language_servers[i].next().await.unwrap_or_else(|| {
            panic!(
                "Failed to get language server #{i} with name {}",
                &language_server_names[i]
            )
        });
        let new_server_name = new_server.server.name();
        assert!(
            !servers_with_hover_requests.contains_key(&new_server_name),
            "Unexpected: initialized server with the same name twice. Name: `{new_server_name}`"
        );
        match new_server_name.as_ref() {
            "TailwindServer" | "TypeScriptServer" => {
                servers_with_hover_requests.insert(
                    new_server_name.clone(),
                    new_server.set_request_handler::<lsp::request::HoverRequest, _, _>(
                        move |_, _| {
                            let name = new_server_name.clone();
                            async move {
                                Ok(Some(lsp::Hover {
                                    contents: lsp::HoverContents::Scalar(
                                        lsp::MarkedString::String(format!("{name} hover")),
                                    ),
                                    range: None,
                                }))
                            }
                        },
                    ),
                );
            }
            "ESLintServer" => {
                servers_with_hover_requests.insert(
                    new_server_name,
                    new_server.set_request_handler::<lsp::request::HoverRequest, _, _>(
                        |_, _| async move { Ok(None) },
                    ),
                );
            }
            "NoHoverCapabilitiesServer" => {
                let _never_handled = new_server
                    .set_request_handler::<lsp::request::HoverRequest, _, _>(|_, _| async move {
                        panic!(
                            "Should not call for hovers server with no corresponding capabilities"
                        )
                    });
            }
            unexpected => panic!("Unexpected server name: {unexpected}"),
        }
    }

    let hover_task = project.update(cx, |project, cx| {
        project.hover(&buffer, Point::new(0, 0), cx)
    });
    let _: Vec<()> = futures::future::join_all(servers_with_hover_requests.into_values().map(
        |mut hover_request| async move {
            hover_request
                .next()
                .await
                .expect("All hover requests should have been triggered")
        },
    ))
    .await;
    assert_eq!(
        vec!["TailwindServer hover", "TypeScriptServer hover"],
        hover_task
            .await
            .into_iter()
            .flatten()
            .map(|hover| hover.contents.iter().map(|block| &block.text).join("|"))
            .sorted()
            .collect::<Vec<_>>(),
        "Should receive hover responses from all related servers with hover capabilities"
    );
}

#[gpui::test]
async fn test_hovers_with_empty_parts(cx: &mut gpui::TestAppContext) {
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
                hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
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

    let mut request_handled = fake_server.set_request_handler::<lsp::request::HoverRequest, _, _>(
        move |_, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Array(vec![
                    lsp::MarkedString::String("".to_string()),
                    lsp::MarkedString::String("      ".to_string()),
                    lsp::MarkedString::String("\n\n\n".to_string()),
                ]),
                range: None,
            }))
        },
    );

    let hover_task = project.update(cx, |project, cx| {
        project.hover(&buffer, Point::new(0, 0), cx)
    });
    let () = request_handled
        .next()
        .await
        .expect("All hover requests should have been triggered");
    assert_eq!(
        Vec::<String>::new(),
        hover_task
            .await
            .into_iter()
            .flatten()
            .map(|hover| hover.contents.iter().map(|block| &block.text).join("|"))
            .sorted()
            .collect::<Vec<_>>(),
        "Empty hover parts should be ignored"
    );
}
