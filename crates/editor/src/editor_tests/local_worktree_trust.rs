use super::*;

#[gpui::test]
async fn test_local_worktree_trust(cx: &mut TestAppContext) {
    init_test(cx, |_| {});
    cx.update(|cx| project::trusted_worktrees::init(HashMap::default(), cx));

    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    });
            });
        });
    });

    let fs = FakeFs::new(cx.executor());
    fs.insert_tree(
        path!("/project"),
        json!({
            ".mav": {
                "settings.json": r#"{"languages":{"Rust":{"language_servers":["override-rust-analyzer"]}}}"#
            },
            "main.rs": "fn main() {}"
        }),
    )
    .await;

    let lsp_inlay_hint_request_count = Arc::new(AtomicUsize::new(0));
    let server_name = "override-rust-analyzer";
    let project = Project::test_with_worktree_trust(fs, [path!("/project").as_ref()], cx).await;

    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let capabilities = lsp::ServerCapabilities {
        inlay_hint_provider: Some(lsp::OneOf::Left(true)),
        ..lsp::ServerCapabilities::default()
    };
    let mut fake_language_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: server_name,
            capabilities,
            initializer: Some(Box::new({
                let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                move |fake_server| {
                    let lsp_inlay_hint_request_count = lsp_inlay_hint_request_count.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |_params, _| {
                            lsp_inlay_hint_request_count.fetch_add(1, atomic::Ordering::Release);
                            async move {
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(0, 0),
                                    label: lsp::InlayHintLabel::String("hint".to_string()),
                                    kind: None,
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: None,
                                    padding_right: None,
                                    data: None,
                                }]))
                            }
                        },
                    );
                }
            })),
            ..FakeLspAdapter::default()
        },
    );

    cx.run_until_parked();

    let worktree_id = project.read_with(cx, |project, cx| {
        project
            .worktrees(cx)
            .next()
            .map(|wt| wt.read(cx).id())
            .expect("should have a worktree")
    });
    let worktree_store = project.read_with(cx, |project, _| project.worktree_store());

    let trusted_worktrees =
        cx.update(|cx| TrustedWorktrees::try_get_global(cx).expect("trust global should exist"));

    let can_trust = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(!can_trust, "worktree should be restricted initially");

    let buffer_before_approval = project
        .update(cx, |project, cx| {
            project.open_buffer((worktree_id, rel_path("main.rs")), cx)
        })
        .await
        .unwrap();

    let (editor, cx) = cx.add_window_view(|window, cx| {
        Editor::new(
            EditorMode::full(),
            cx.new(|cx| MultiBuffer::singleton(buffer_before_approval.clone(), cx)),
            Some(project.clone()),
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let fake_language_server = fake_language_servers.next();

    cx.read(|cx| {
        assert_eq!(
            language::language_settings::LanguageSettings::for_buffer(
                buffer_before_approval.read(cx),
                cx
            )
            .language_servers,
            ["...".to_string()],
            "local .mav/settings.json must not apply before trust approval"
        )
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert_eq!(
        lsp_inlay_hint_request_count.load(atomic::Ordering::Acquire),
        0,
        "inlay hints must not be queried before trust approval"
    );

    trusted_worktrees.update(cx, |store, cx| {
        store.trust(
            &worktree_store,
            std::collections::HashSet::from_iter([PathTrust::Worktree(worktree_id)]),
            cx,
        );
    });
    cx.run_until_parked();

    cx.read(|cx| {
        assert_eq!(
            language::language_settings::LanguageSettings::for_buffer(
                buffer_before_approval.read(cx),
                cx
            )
            .language_servers,
            ["override-rust-analyzer".to_string()],
            "local .mav/settings.json should apply after trust approval"
        )
    });
    let _fake_language_server = fake_language_server.await.unwrap();
    editor.update_in(cx, |editor, window, cx| {
        editor.handle_input("1", window, cx);
    });
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    assert!(
        lsp_inlay_hint_request_count.load(atomic::Ordering::Acquire) > 0,
        "inlay hints should be queried after trust approval"
    );

    let can_trust_after = trusted_worktrees.update(cx, |store, cx| {
        store.can_trust(&worktree_store, worktree_id, cx)
    });
    assert!(can_trust_after, "worktree should be trusted after trust()");
}
