use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_managing_language_servers(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "test.rs": "const A: i32 = 1;",
            "test2.rs": "",
            "Cargo.toml": "a = 1",
            "package.json": "{\"a\": 1}",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let mut fake_rust_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-rust-language-server",
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "::".to_string()]),
                    ..Default::default()
                }),
                text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
                    lsp::TextDocumentSyncOptions {
                        save: Some(lsp::TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut fake_json_servers = language_registry.register_fake_lsp(
        "JSON",
        FakeLspAdapter {
            name: "the-json-language-server",
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![":".to_string()]),
                    ..Default::default()
                }),
                text_document_sync: Some(lsp::TextDocumentSyncCapability::Options(
                    lsp::TextDocumentSyncOptions {
                        save: Some(lsp::TextDocumentSyncSaveOptions::Supported(true)),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    // Open a buffer without an associated language server.
    let (toml_buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/Cargo.toml"), cx)
        })
        .await
        .unwrap();

    // Open a buffer with an associated language server before the language for it has been loaded.
    let (rust_buffer, _handle2) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/test.rs"), cx)
        })
        .await
        .unwrap();
    rust_buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.language().map(|l| l.name()), None);
    });

    // Now we add the languages to the project, and ensure they get assigned to all
    // the relevant open buffers.
    language_registry.add(json_lang());
    language_registry.add(rust_lang());
    cx.executor().run_until_parked();
    rust_buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.language().map(|l| l.name()), Some("Rust".into()));
    });

    // A server is started up, and it is notified about Rust files.
    let mut fake_rust_server = fake_rust_servers.next().await.unwrap();
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/test.rs")).unwrap(),
            version: 0,
            text: "const A: i32 = 1;".to_string(),
            language_id: "rust".to_string(),
        }
    );

    // The buffer is configured based on the language server's capabilities.
    rust_buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .completion_triggers()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            &[".".to_string(), "::".to_string()]
        );
    });
    toml_buffer.update(cx, |buffer, _| {
        assert!(buffer.completion_triggers().is_empty());
    });

    // Edit a buffer. The changes are reported to the language server.
    rust_buffer.update(cx, |buffer, cx| buffer.edit([(16..16, "2")], None, cx));
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidChangeTextDocument>()
            .await
            .text_document,
        lsp::VersionedTextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/test.rs")).unwrap(),
            1
        )
    );

    // Open a third buffer with a different associated language server.
    let (json_buffer, _json_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/package.json"), cx)
        })
        .await
        .unwrap();

    // A json language server is started up and is only notified about the json buffer.
    let mut fake_json_server = fake_json_servers.next().await.unwrap();
    assert_eq!(
        fake_json_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/package.json")).unwrap(),
            version: 0,
            text: "{\"a\": 1}".to_string(),
            language_id: "json".to_string(),
        }
    );

    // This buffer is configured based on the second language server's
    // capabilities.
    json_buffer.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .completion_triggers()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            &[":".to_string()]
        );
    });

    // When opening another buffer whose language server is already running,
    // it is also configured based on the existing language server's capabilities.
    let (rust_buffer2, _handle4) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/test2.rs"), cx)
        })
        .await
        .unwrap();
    rust_buffer2.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .completion_triggers()
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            &[".".to_string(), "::".to_string()]
        );
    });

    // Changes are reported only to servers matching the buffer's language.
    toml_buffer.update(cx, |buffer, cx| buffer.edit([(5..5, "23")], None, cx));
    rust_buffer2.update(cx, |buffer, cx| {
        buffer.edit([(0..0, "let x = 1;")], None, cx)
    });
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidChangeTextDocument>()
            .await
            .text_document,
        lsp::VersionedTextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/test2.rs")).unwrap(),
            1
        )
    );

    // Save notifications are reported to all servers.
    project
        .update(cx, |project, cx| project.save_buffer(toml_buffer, cx))
        .await
        .unwrap();
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidSaveTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/Cargo.toml")).unwrap()
        )
    );
    assert_eq!(
        fake_json_server
            .receive_notification::<lsp::notification::DidSaveTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/Cargo.toml")).unwrap()
        )
    );

    // Renames are reported only to servers matching the buffer's language.
    fs.rename(
        Path::new(path!("/dir/test2.rs")),
        Path::new(path!("/dir/test3.rs")),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentIdentifier::new(lsp::Uri::from_file_path(path!("/dir/test2.rs")).unwrap()),
    );
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/test3.rs")).unwrap(),
            version: 0,
            text: rust_buffer2.update(cx, |buffer, _| buffer.text()),
            language_id: "rust".to_string(),
        },
    );

    rust_buffer2.update(cx, |buffer, cx| {
        buffer.update_diagnostics(
            LanguageServerId(0),
            DiagnosticSet::from_sorted_entries(
                vec![DiagnosticEntry {
                    diagnostic: Default::default(),
                    range: Anchor::min_max_range_for_buffer(buffer.remote_id()),
                }],
                &buffer.snapshot(),
            ),
            cx,
        );
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, usize>(0..buffer.len(), false)
                .count(),
            1
        );
    });

    // When the rename changes the extension of the file, the buffer gets closed on the old
    // language server and gets opened on the new one.
    fs.rename(
        Path::new(path!("/dir/test3.rs")),
        Path::new(path!("/dir/test3.json")),
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentIdentifier::new(lsp::Uri::from_file_path(path!("/dir/test3.rs")).unwrap()),
    );
    assert_eq!(
        fake_json_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/test3.json")).unwrap(),
            version: 0,
            text: rust_buffer2.update(cx, |buffer, _| buffer.text()),
            language_id: "json".to_string(),
        },
    );

    // We clear the diagnostics, since the language has changed.
    rust_buffer2.update(cx, |buffer, _| {
        assert_eq!(
            buffer
                .snapshot()
                .diagnostics_in_range::<_, usize>(0..buffer.len(), false)
                .count(),
            0
        );
    });

    // The renamed file's version resets after changing language server.
    rust_buffer2.update(cx, |buffer, cx| buffer.edit([(0..0, "// ")], None, cx));
    assert_eq!(
        fake_json_server
            .receive_notification::<lsp::notification::DidChangeTextDocument>()
            .await
            .text_document,
        lsp::VersionedTextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/test3.json")).unwrap(),
            1
        )
    );

    // Restart language servers
    project.update(cx, |project, cx| {
        project.restart_language_servers_for_buffers(
            vec![rust_buffer.clone(), json_buffer.clone()],
            HashSet::default(),
            true,
            cx,
        );
    });

    let mut rust_shutdown_requests = fake_rust_server
        .set_request_handler::<lsp::request::Shutdown, _, _>(|_, _| future::ready(Ok(())));
    let mut json_shutdown_requests = fake_json_server
        .set_request_handler::<lsp::request::Shutdown, _, _>(|_, _| future::ready(Ok(())));
    futures::join!(rust_shutdown_requests.next(), json_shutdown_requests.next());

    let mut fake_rust_server = fake_rust_servers.next().await.unwrap();
    let mut fake_json_server = fake_json_servers.next().await.unwrap();

    // Ensure rust document is reopened in new rust language server
    assert_eq!(
        fake_rust_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document,
        lsp::TextDocumentItem {
            uri: lsp::Uri::from_file_path(path!("/dir/test.rs")).unwrap(),
            version: 0,
            text: rust_buffer.update(cx, |buffer, _| buffer.text()),
            language_id: "rust".to_string(),
        }
    );

    // Ensure json documents are reopened in new json language server
    assert_set_eq!(
        [
            fake_json_server
                .receive_notification::<lsp::notification::DidOpenTextDocument>()
                .await
                .text_document,
            fake_json_server
                .receive_notification::<lsp::notification::DidOpenTextDocument>()
                .await
                .text_document,
        ],
        [
            lsp::TextDocumentItem {
                uri: lsp::Uri::from_file_path(path!("/dir/package.json")).unwrap(),
                version: 0,
                text: json_buffer.update(cx, |buffer, _| buffer.text()),
                language_id: "json".to_string(),
            },
            lsp::TextDocumentItem {
                uri: lsp::Uri::from_file_path(path!("/dir/test3.json")).unwrap(),
                version: 0,
                text: rust_buffer2.update(cx, |buffer, _| buffer.text()),
                language_id: "json".to_string(),
            }
        ]
    );

    // Close notifications are reported only to servers matching the buffer's language.
    cx.update(|_| drop(_json_handle));
    let close_message = lsp::DidCloseTextDocumentParams {
        text_document: lsp::TextDocumentIdentifier::new(
            lsp::Uri::from_file_path(path!("/dir/package.json")).unwrap(),
        ),
    };
    assert_eq!(
        fake_json_server
            .receive_notification::<lsp::notification::DidCloseTextDocument>()
            .await,
        close_message,
    );
}
