use super::*;
use pretty_assertions::assert_eq;

#[gpui::test]
async fn test_cancel_language_server_work(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let progress_token = "the-progress-token";

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "" })).await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "the-language-server",
            disk_based_diagnostics_sources: vec!["disk".into()],
            disk_based_diagnostics_progress_token: Some(progress_token.into()),
            ..Default::default()
        },
    );

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();

    // Simulate diagnostics starting to update.
    let mut fake_server = fake_servers.next().await.unwrap();
    fake_server
        .start_progress_with(
            "another-token",
            lsp::WorkDoneProgressBegin {
                cancellable: Some(false),
                ..Default::default()
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await;
    // Ensure progress notification is fully processed before starting the next one
    cx.executor().run_until_parked();

    fake_server
        .start_progress_with(
            progress_token,
            lsp::WorkDoneProgressBegin {
                cancellable: Some(true),
                ..Default::default()
            },
            DEFAULT_LSP_REQUEST_TIMEOUT,
        )
        .await;
    // Ensure progress notification is fully processed before cancelling
    cx.executor().run_until_parked();

    project.update(cx, |project, cx| {
        project.cancel_language_server_work_for_buffers([buffer.clone()], cx)
    });
    cx.executor().run_until_parked();

    let cancel_notification = fake_server
        .receive_notification::<lsp::notification::WorkDoneProgressCancel>()
        .await;
    assert_eq!(
        cancel_notification.token,
        NumberOrString::String(progress_token.into())
    );
}

#[gpui::test]
async fn test_toggling_enable_language_server(cx: &mut gpui::TestAppContext) {
    init_test(cx);

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(path!("/dir"), json!({ "a.rs": "", "b.js": "" }))
        .await;

    let project = Project::test(fs, [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let mut fake_rust_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-lsp",
            ..Default::default()
        },
    );
    let mut fake_js_servers = language_registry.register_fake_lsp(
        "JavaScript",
        FakeLspAdapter {
            name: "js-lsp",
            ..Default::default()
        },
    );
    language_registry.add(rust_lang());
    language_registry.add(js_lang());

    let _rs_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/a.rs"), cx)
        })
        .await
        .unwrap();
    let _js_buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/dir/b.js"), cx)
        })
        .await
        .unwrap();

    let mut fake_rust_server_1 = fake_rust_servers.next().await.unwrap();
    assert_eq!(
        fake_rust_server_1
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document
            .uri
            .as_str(),
        uri!("file:///dir/a.rs")
    );

    let mut fake_js_server = fake_js_servers.next().await.unwrap();
    assert_eq!(
        fake_js_server
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document
            .uri
            .as_str(),
        uri!("file:///dir/b.js")
    );

    // Disable Rust language server, ensuring only that server gets stopped.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.languages_mut().insert(
                    "Rust".into(),
                    LanguageSettingsContent {
                        enable_language_server: Some(false),
                        ..Default::default()
                    },
                );
            });
        })
    });
    fake_rust_server_1
        .receive_notification::<lsp::notification::Exit>()
        .await;

    // Enable Rust and disable JavaScript language servers, ensuring that the
    // former gets started again and that the latter stops.
    cx.update(|cx| {
        SettingsStore::update_global(cx, |settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.languages_mut().insert(
                    "Rust".into(),
                    LanguageSettingsContent {
                        enable_language_server: Some(true),
                        ..Default::default()
                    },
                );
                settings.languages_mut().insert(
                    "JavaScript".into(),
                    LanguageSettingsContent {
                        enable_language_server: Some(false),
                        ..Default::default()
                    },
                );
            });
        })
    });
    let mut fake_rust_server_2 = fake_rust_servers.next().await.unwrap();
    assert_eq!(
        fake_rust_server_2
            .receive_notification::<lsp::notification::DidOpenTextDocument>()
            .await
            .text_document
            .uri
            .as_str(),
        uri!("file:///dir/a.rs")
    );
    fake_js_server
        .receive_notification::<lsp::notification::Exit>()
        .await;
}
