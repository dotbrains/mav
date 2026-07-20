use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_multiple_did_change_watched_files_registrations(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            "src": {
                "a.rs": "",
                "b.rs": "",
            },
            "docs": {
                "readme.md": "",
            },
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
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
            project.open_local_buffer_with_lsp(path!("/root/src/a.rs"), cx)
        })
        .await
        .unwrap();

    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().run_until_parked();

    let file_changes = Arc::new(Mutex::new(Vec::new()));

    // Register two separate watched file registrations.
    fake_server
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: "reg-1".to_string(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: serde_json::to_value(
                        lsp::DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![lsp::FileSystemWatcher {
                                glob_pattern: lsp::GlobPattern::String(
                                    path!("/root/src/*.rs").to_string(),
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

    fake_server
        .request::<lsp::request::RegisterCapability>(
            lsp::RegistrationParams {
                registrations: vec![lsp::Registration {
                    id: "reg-2".to_string(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: serde_json::to_value(
                        lsp::DidChangeWatchedFilesRegistrationOptions {
                            watchers: vec![lsp::FileSystemWatcher {
                                glob_pattern: lsp::GlobPattern::String(
                                    path!("/root/docs/*.md").to_string(),
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
            file_changes.sort_by(|a, b| a.uri.cmp(&b.uri));
        }
    });

    cx.executor().run_until_parked();

    // Both registrations should match their respective patterns.
    fs.create_file(path!("/root/src/c.rs").as_ref(), Default::default())
        .await
        .unwrap();
    fs.create_file(path!("/root/docs/guide.md").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    assert_eq!(
        &*file_changes.lock(),
        &[
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/root/docs/guide.md")).unwrap(),
                typ: lsp::FileChangeType::CREATED,
            },
            lsp::FileEvent {
                uri: lsp::Uri::from_file_path(path!("/root/src/c.rs")).unwrap(),
                typ: lsp::FileChangeType::CREATED,
            },
        ]
    );
    file_changes.lock().clear();

    // Unregister the first registration.
    fake_server
        .request::<lsp::request::UnregisterCapability>(
            lsp::UnregistrationParams {
                unregisterations: vec![lsp::Unregistration {
                    id: "reg-1".to_string(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                }],
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await
        .into_response()
        .unwrap();
    cx.executor().run_until_parked();

    // Only the second registration should still match.
    fs.create_file(path!("/root/src/d.rs").as_ref(), Default::default())
        .await
        .unwrap();
    fs.create_file(path!("/root/docs/notes.md").as_ref(), Default::default())
        .await
        .unwrap();
    cx.executor().run_until_parked();

    assert_eq!(
        &*file_changes.lock(),
        &[lsp::FileEvent {
            uri: lsp::Uri::from_file_path(path!("/root/docs/notes.md")).unwrap(),
            typ: lsp::FileChangeType::CREATED,
        }]
    );
}
