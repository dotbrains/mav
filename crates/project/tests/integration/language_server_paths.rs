use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_late_lsp_adapter_registration(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/dir"),
        json!({
            "test.rs": "const A: i32 = 1;",
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    // Add the language first so the buffer gets assigned a language.
    language_registry.add(rust_lang());
    cx.executor().run_until_parked();

    // Open a buffer — it gets assigned the Rust language but there is no LSP adapter yet.
    let (rust_buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/test.rs"), cx)
        })
        .await
        .unwrap();

    rust_buffer.update(cx, |buffer, _| {
        assert_eq!(buffer.language().map(|l| l.name()), Some("Rust".into()));
    });

    // Now register the LSP adapter late (simulating an extension loading after startup).
    let mut fake_rust_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-rust-language-server",
            capabilities: lsp::ServerCapabilities {
                completion_provider: Some(lsp::CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), "::".to_string()]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    cx.executor().run_until_parked();

    // The language server should start and receive a DidOpenTextDocument notification
    // for the already-open buffer.
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

    // The buffer should be configured with the language server's capabilities.
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
}

#[gpui::test]
async fn test_language_server_relative_path(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let settings_json_contents = json!({
        "languages": {
            "Rust": {
                "language_servers": ["my_fake_lsp", "lsp_on_path"]
            }
        },
        "lsp": {
            "my_fake_lsp": {
                "binary": {
                    // file exists, so this is treated as a relative path
                    "path": path!(".relative_path/to/my_fake_lsp_binary.exe").to_string(),
                }
            },
            "lsp_on_path": {
                "binary": {
                    // file doesn't exist, so it will fall back on PATH env var
                    "path": path!("lsp_on_path.exe").to_string(),
                }
            }
        },
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/the-root"),
        json!({
            ".mav": {
                "settings.json": settings_json_contents.to_string(),
            },
            ".relative_path": {
                "to": {
                    "my_fake_lsp.exe": "",
                },
            },
            "src": {
                "main.rs": "",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/the-root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let mut my_fake_lsp = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "my_fake_lsp",
            ..Default::default()
        },
    );
    let mut lsp_on_path = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "lsp_on_path",
            ..Default::default()
        },
    );

    cx.run_until_parked();

    // Start the language server by opening a buffer with a compatible file extension.
    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/the-root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    let lsp_path = my_fake_lsp.next().await.unwrap().binary.path;
    assert_eq!(
        lsp_path.to_string_lossy(),
        path!("/the-root/.relative_path/to/my_fake_lsp_binary.exe"),
    );

    let lsp_path = lsp_on_path.next().await.unwrap().binary.path;
    assert_eq!(lsp_path.to_string_lossy(), path!("lsp_on_path.exe"));
}

#[gpui::test]
async fn test_language_server_tilde_path(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let settings_json_contents = json!({
        "languages": {
            "Rust": {
                "language_servers": ["tilde_lsp"]
            }
        },
        "lsp": {
            "tilde_lsp": {
                "binary": {
                    "path": "~/.local/bin/rust-analyzer",
                }
            }
        },
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/root"),
        json!({
            ".mav": {
                "settings.json": settings_json_contents.to_string(),
            },
            "src": {
                "main.rs": "fn main() {}",
            }
        }),
    )
    .await;

    let project = Project::test(fs.clone(), [path!("/root").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let mut tilde_lsp = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "tilde_lsp",
            ..Default::default()
        },
    );
    cx.run_until_parked();

    project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/root/src/main.rs"), cx)
        })
        .await
        .unwrap();

    let lsp_path = tilde_lsp.next().await.unwrap().binary.path;
    let expected_path = paths::home_dir().join(".local/bin/rust-analyzer");
    assert_eq!(
        lsp_path, expected_path,
        "Tilde path should expand to home directory"
    );
}
