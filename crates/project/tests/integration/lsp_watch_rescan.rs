use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_rescan_fs_change_is_reported_to_language_servers_as_changed(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/the-root"),
        json!({
            "Cargo.lock": "",
            "src": {
                "a.rs": "",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/the-root").as_ref()], cx).await;
    let (language_registry, _lsp_store) = project.read_with(cx, |project, _| {
        (project.languages().clone(), project.lsp_store())
    });
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            ..Default::default()
        },
    );

    cx.executor().run_until_parked();

    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/src/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    let file_changes = Arc::new(Mutex::new(Vec::new()));
    fake_server
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: Default::default(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: serde_json::to_value(
                        lsp::DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![lsp::FileSystemWatcher {
                                glob_pattern: lsp::GlobPattern::String(
                                    path!("/the-root/Cargo.lock").to_string(),
                                ),
                                kind: None,
                            }],
                        },
                    )
                    .ok(),
                }],
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    fake_server.handle_notification::<lsp::notification::DidChangeWatchedFiles, _>({
        let file_changes = file_changes.clone();
        move |params, _| {
            let mut file_changes = file_changes.lock();
            file_changes.extend(params.changes);
        }
    });

    cx.executor().run_until_parked();
    assert_eq!(mem::take(&mut *file_changes.lock()), &[]);

    fs.emit_fs_event(path!("/the-root/Cargo.lock"), Some(PathEventKind::Rescan));
    cx.executor().run_until_parked();

    assert_eq!(
        &*file_changes.lock(),
        &[lsp::FileEvent {
            uri: lsp::Uri::from_file_path(path!("/the-root/Cargo.lock")).unwrap(),
            typ: lsp::FileChangeType::CHANGED,
        }]
    );
}

#[gpui::test]
async fn test_dynamic_semantic_tokens_registration(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/the-root"),
        json!({
            "a.rs": "fn main() {}",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/the-root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            // Crucially, no `semantic_tokens_provider` is advertised statically; the
            // server only offers it through dynamic registration (as Roslyn does).
            ..Default::default()
        },
    );

    let _buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    let server_id = fake_server.server.server_id();
    cx.executor().run_until_parked();

    let semantic_tokens_provider = |cx: &mut gpui::TestAppContext| {
        project.read_with(cx, |project, cx| {
            project
                .lsp_store()
                .read(cx)
                .lsp_server_capabilities
                .get(&server_id)
                .and_then(|capabilities| capabilities.semantic_tokens_provider.clone())
        })
    };

    assert!(
        semantic_tokens_provider(cx).is_none(),
        "server should not advertise semantic tokens before dynamic registration"
    );

    fake_server
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: "semantic-tokens".to_string(),
                    method: "textDocument/semanticTokens".to_string(),
                    register_options: serde_json::to_value(
                        lsp::SemanticTokensRegistrationOptions {
                            text_document_registration_options:
                                lsp::TextDocumentRegistrationOptions {
                                    document_selector: None,
                                },
                            semantic_tokens_options: lsp::SemanticTokensOptions {
                                legend: lsp::SemanticTokensLegend {
                                    token_types: vec!["keyword".into(), "variable".into()],
                                    token_modifiers: vec![],
                                },
                                full: Some(lsp::SemanticTokensFullOptions::Bool(true)),
                                ..Default::default()
                            },
                            static_registration_options: lsp::StaticRegistrationOptions {
                                id: None,
                            },
                        },
                    )
                    .ok(),
                }],
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    cx.executor().run_until_parked();

    let provider = semantic_tokens_provider(cx)
        .expect("semantic tokens provider should be set after dynamic registration");
    // The capability round-trips through capability-sync serialization, which may
    // normalize the registration options into plain options; either shape is fine
    // as long as the legend survives.
    let legend = match provider {
        lsp::SemanticTokensServerCapabilities::SemanticTokensOptions(options) => options.legend,
        lsp::SemanticTokensServerCapabilities::SemanticTokensRegistrationOptions(options) => {
            options.semantic_tokens_options.legend
        }
    };
    assert_eq!(
        legend.token_types,
        vec!["keyword".into(), "variable".into()],
    );

    fake_server
        .request::<lsp::request::UnregisterCapability>(
            lsp::UnregistrationParams {
                unregisterations: vec![lsp::Unregistration {
                    id: "semantic-tokens".to_string(),
                    method: "textDocument/semanticTokens".to_string(),
                }],
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    cx.executor().run_until_parked();

    assert!(
        semantic_tokens_provider(cx).is_none(),
        "semantic tokens provider should be cleared after unregistration"
    );
}
