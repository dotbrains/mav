use super::*;

#[gpui::test]
async fn test_remote_cancel_language_server_work(
    cx: &mut TestAppContext,
    server_cx: &mut TestAppContext,
) {
    let fs = FakeFs::new(server_cx.executor());
    fs.insert_tree(
        path!("/code"),
        json!({
            "project1": {
                ".git": {},
                "README.md": "# project 1",
                "src": {
                    "lib.rs": "fn one() -> usize { 1 }"
                }
            },
        }),
    )
    .await;

    let (project, headless) = init_test(&fs, cx, server_cx).await;

    fs.insert_tree(
        path!("/code/project1/.mav"),
        json!({
            "settings.json": r#"
          {
            "languages": {"Rust":{"language_servers":["rust-analyzer"]}},
            "lsp": {
              "rust-analyzer": {
                "binary": {
                  "path": "~/.cargo/bin/rust-analyzer"
                }
              }
            }
          }"#
        }),
    )
    .await;

    cx.update_entity(&project, |project, _| {
        project.languages().register_test_language(LanguageConfig {
            name: "Rust".into(),
            matcher: LanguageMatcher {
                path_suffixes: vec!["rs".into()],
                ..Default::default()
            },
            ..Default::default()
        });
        project.languages().register_fake_lsp_adapter(
            "Rust",
            FakeLspAdapter {
                name: "rust-analyzer",
                ..Default::default()
            },
        )
    });

    let mut fake_lsp = server_cx.update(|cx| {
        headless.read(cx).languages.register_fake_lsp_server(
            LanguageServerName("rust-analyzer".into()),
            Default::default(),
            None,
        )
    });

    cx.run_until_parked();

    let worktree_id = project
        .update(cx, |project, cx| {
            project.find_or_create_worktree(path!("/code/project1"), true, cx)
        })
        .await
        .unwrap()
        .0
        .read_with(cx, |worktree, _| worktree.id());

    cx.run_until_parked();

    let (buffer, _handle) = project
        .update(cx, |project, cx| {
            project.open_buffer_with_lsp((worktree_id, rel_path("src/lib.rs")), cx)
        })
        .await
        .unwrap();

    cx.run_until_parked();

    let mut fake_lsp = fake_lsp.next().await.unwrap();

    // Cancelling all language server work for a given buffer
    {
        // Two operations, one cancellable and one not.
        fake_lsp
            .start_progress_with(
                "another-token",
                lsp::WorkDoneProgressBegin {
                    cancellable: Some(false),
                    ..Default::default()
                },
                DEFAULT_LSP_REQUEST_TIMEOUT,
            )
            .await;

        let progress_token = "the-progress-token";
        fake_lsp
            .start_progress_with(
                progress_token,
                lsp::WorkDoneProgressBegin {
                    cancellable: Some(true),
                    ..Default::default()
                },
                DEFAULT_LSP_REQUEST_TIMEOUT,
            )
            .await;

        cx.executor().run_until_parked();

        project.update(cx, |project, cx| {
            project.cancel_language_server_work_for_buffers([buffer.clone()], cx)
        });

        cx.executor().run_until_parked();

        // Verify the cancellation was received on the server side
        let cancel_notification = fake_lsp
            .receive_notification::<lsp::notification::WorkDoneProgressCancel>()
            .await;
        assert_eq!(
            cancel_notification.token,
            lsp::NumberOrString::String(progress_token.into())
        );
    }

    // Cancelling work by server_id and token
    {
        let server_id = fake_lsp.server.server_id();
        let progress_token = "the-progress-token";

        fake_lsp
            .start_progress_with(
                progress_token,
                lsp::WorkDoneProgressBegin {
                    cancellable: Some(true),
                    ..Default::default()
                },
                DEFAULT_LSP_REQUEST_TIMEOUT,
            )
            .await;

        cx.executor().run_until_parked();

        project.update(cx, |project, cx| {
            project.cancel_language_server_work(
                server_id,
                Some(ProgressToken::String(SharedString::from(progress_token))),
                cx,
            )
        });

        cx.executor().run_until_parked();

        // Verify the cancellation was received on the server side
        let cancel_notification = fake_lsp
            .receive_notification::<lsp::notification::WorkDoneProgressCancel>()
            .await;
        assert_eq!(
            cancel_notification.token,
            lsp::NumberOrString::String(progress_token.to_owned())
        );
    }
}
