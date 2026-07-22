use super::*;

#[gpui::test]
async fn test_search_with_inlays(cx: &mut TestAppContext) {
    init_test(cx);
    cx.update(|cx| {
        SettingsStore::update_global(cx, |store, cx| {
            store.update_user_settings(cx, |settings| {
                settings.project.all_languages.defaults.inlay_hints =
                    Some(InlayHintSettingsContent {
                        enabled: Some(true),
                        ..InlayHintSettingsContent::default()
                    })
            });
        });
    });
    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/dir"),
        // `\n` , a trailing line on the end, is important for the test case
        json!({
            "main.rs": "fn main() { let a = 2; }\n",
        }),
    )
    .await;

    let requests_count = Arc::new(AtomicUsize::new(0));
    let closure_requests_count = requests_count.clone();
    let project = Project::test(fs.clone(), [path!("/dir").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    let language = rust_lang();
    language_registry.add(language);
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new(move |fake_server| {
                let requests_count = closure_requests_count.clone();
                fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>({
                    move |_, _| {
                        let requests_count = requests_count.clone();
                        async move {
                            requests_count.fetch_add(1, atomic::Ordering::Release);
                            Ok(Some(vec![lsp::InlayHint {
                                position: lsp::Position::new(0, 17),
                                label: lsp::InlayHintLabel::String(": i32".to_owned()),
                                kind: Some(lsp::InlayHintKind::TYPE),
                                text_edits: None,
                                tooltip: None,
                                padding_left: None,
                                padding_right: None,
                                data: None,
                            }]))
                        }
                    }
                });
            })),
            ..FakeLspAdapter::default()
        },
    );

    let window = cx.add_window(|window, cx| MultiWorkspace::test_new(project.clone(), window, cx));
    let workspace = window
        .read_with(cx, |mw, _| mw.workspace().clone())
        .unwrap();
    let cx = &mut VisualTestContext::from_window(window.into(), cx);
    let search = cx.new(|cx| ProjectSearch::new(project.clone(), cx));
    let search_view = cx.add_window(|window, cx| {
        ProjectSearchView::new(workspace.downgrade(), search.clone(), window, cx, None)
    });

    perform_search(search_view, "let ", cx);
    let fake_server = fake_servers.next().await.unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        1,
        "New hints should have been queried",
    );

    // Can do the 2nd search without any panics
    perform_search(search_view, "let ", cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        2,
        "We did drop the previous buffer when cleared the old project search results, hence another query was made",
    );

    let singleton_editor = workspace
        .update_in(cx, |workspace, window, cx| {
            workspace.open_abs_path(
                PathBuf::from(path!("/dir/main.rs")),
                workspace::OpenOptions::default(),
                window,
                cx,
            )
        })
        .await
        .unwrap()
        .downcast::<Editor>()
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "fn main() { let a: i32 = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        2,
        "Opening the same buffer again should reuse the cached hints",
    );

    window
        .update(cx, |_, window, cx| {
            singleton_editor.update(cx, |editor, cx| {
                editor.handle_input("test", window, cx);
            });
        })
        .unwrap();

    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "testfn main() { l: i32et a = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        3,
        "We have edited the buffer and should send a new request",
    );

    window
        .update(cx, |_, window, cx| {
            singleton_editor.update(cx, |editor, cx| {
                editor.undo(&editor::actions::Undo, window, cx);
            });
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        4,
        "We have edited the buffer again and should send a new request again",
    );
    singleton_editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.display_text(cx),
            "fn main() { let a: i32 = 2; }\n",
            "Newly opened editor should have the correct text with hints",
        );
    });
    project.update(cx, |_, cx| {
        cx.emit(project::Event::RefreshInlayHints {
            server_id: fake_server.server.server_id(),
            request_id: Some(1),
        });
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        5,
        "After a simulated server refresh request, we should have sent another request",
    );

    perform_search(search_view, "let ", cx);
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.executor().run_until_parked();
    assert_eq!(
        requests_count.load(atomic::Ordering::Acquire),
        5,
        "New project search should reuse the cached hints",
    );
    search_view
        .update(cx, |search_view, _, cx| {
            assert_eq!(
                search_view
                    .results_editor
                    .update(cx, |editor, cx| editor.display_text(cx)),
                "\n\nfn main() { let a: i32 = 2; }\n"
            );
        })
        .unwrap();
}
