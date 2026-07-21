use super::*;

#[gpui::test]
async fn test_edit_then_scroll_race(cx: &mut gpui::TestAppContext) {
    // Bug 1: An edit fires with a long debounce, and a scroll brings new lines
    // before that debounce elapses. The edit task's apply_fetched_hints removes
    // ALL visible hints (including the scroll-added ones) but only adds back
    // hints for its own chunks. The scroll chunk remains in hint_chunk_fetching,
    // so it is never re-queried, leaving it permanently empty.
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            edit_debounce_ms: Some(700),
            scroll_debounce_ms: Some(50),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    let mut file_content = String::from("fn main() {\n");
    for i in 0..150 {
        file_content.push_str(&format!("    let v{i} = {i};\n"));
    }
    file_content.push_str("}\n");
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": file_content,
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    let lsp_request_ranges = Arc::new(Mutex::new(Vec::new()));
    let mut fake_servers = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let lsp_request_ranges = lsp_request_ranges.clone();
                move |fake_server| {
                    let lsp_request_ranges = lsp_request_ranges.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |params, _| {
                            let lsp_request_ranges = lsp_request_ranges.clone();
                            async move {
                                lsp_request_ranges.lock().push(params.range);
                                let start_line = params.range.start.line;
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(start_line + 1, 9),
                                    label: lsp::InlayHintLabel::String(format!(
                                        "chunk_{start_line}"
                                    )),
                                    kind: Some(lsp::InlayHintKind::TYPE),
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

    let buffer = project
        .update(cx, |project, cx| {
            project.open_local_buffer(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor = cx.add_window(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));
    cx.executor().run_until_parked();
    let _fake_server = fake_servers.next().await.unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.set_visible_line_count(50.0, window, cx);
            editor.set_visible_column_count(120.0);
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            assert!(
                visible.iter().any(|h| h.starts_with("chunk_0")),
                "Should have chunk_0 hints initially, got: {visible:?}"
            );
        })
        .unwrap();

    lsp_request_ranges.lock().clear();

    // Step 1: Make an edit → triggers BufferEdited with 700ms debounce.
    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(13)..MultiBufferOffset(13)])
            });
            editor.handle_input("x", window, cx);
        })
        .unwrap();
    // Let the BufferEdited event propagate and the edit task get spawned.
    cx.executor().run_until_parked();

    // Step 2: Scroll down to reveal a new chunk, then trigger NewLinesShown.
    // This spawns a scroll task with the shorter 50ms debounce.
    editor
        .update(cx, |editor, window, cx| {
            editor.scroll_screen(&ScrollAmount::Page(1.0), window, cx);
        })
        .unwrap();
    // Explicitly trigger NewLinesShown for the new visible range.
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();

    // Step 3: Advance clock past scroll debounce (50ms) but NOT past edit
    // debounce (700ms). The scroll task completes and adds hints for the
    // new chunk.
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    // The scroll task's apply_fetched_hints also processes
    // invalidate_hints_for_buffers (set by the earlier BufferEdited), which
    // removes the old chunk_0 hint. Only the scroll chunk's hint remains.
    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            assert!(
                visible.iter().any(|h| h.starts_with("chunk_50")),
                "After scroll task completes, the scroll chunk's hints should be \
                 present, got: {visible:?}"
            );
        })
        .unwrap();

    // Step 4: Advance clock past the edit debounce (700ms). The edit task
    // completes, calling apply_fetched_hints with should_invalidate()=true,
    // which removes ALL visible hints (including the scroll chunk's) but only
    // adds back hints for its own chunks (chunk_0).
    cx.executor().advance_clock(Duration::from_millis(700));
    cx.executor().run_until_parked();

    // At this point the edit task has:
    //   - removed chunk_50's hint (via should_invalidate removing all visible)
    //   - added chunk_0's hint (from its own fetch)
    //   - (with fix) cleared chunk_50 from hint_chunk_fetching
    // Without the fix, chunk_50 is stuck in hint_chunk_fetching and will
    // never be re-queried by NewLinesShown.

    // Step 5: Trigger NewLinesShown to give the system a chance to re-fetch
    // any chunks whose hints were lost.
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            assert!(
                visible.iter().any(|h| h.starts_with("chunk_0")),
                "chunk_0 hints (from edit task) should be present. Got: {visible:?}"
            );
            assert!(
                visible.iter().any(|h| h.starts_with("chunk_50")),
                "chunk_50 hints should have been re-fetched after NewLinesShown. \
                 Bug 1: the scroll chunk's hints were removed by the edit task \
                 and the chunk was stuck in hint_chunk_fetching, preventing \
                 re-fetch. Got: {visible:?}"
            );
        })
        .unwrap();
}

#[gpui::test]
async fn test_refresh_requested_multi_server(cx: &mut gpui::TestAppContext) {
    // Bug 2: When one LSP server sends workspace/inlayHint/refresh, the editor
    // wipes all tracking state via clear(), then spawns tasks that call
    // LspStore::inlay_hints with for_server=Some(requesting_server). The LspStore
    // filters out other servers' cached hints via the for_server guard, so only
    // the requesting server's hints are returned. apply_fetched_hints removes ALL
    // visible hints (should_invalidate()=true) but only adds back the requesting
    // server's hints. Other servers' hints disappear permanently.
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            ..InlayHintSettingsContent::default()
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let x = 1; } // padding to keep hints from being trimmed",
            "other.rs": "// Test file",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());
    language_registry.add(rust_lang());

    // Server A returns a hint labeled "server_a".
    let server_a_request_count = Arc::new(AtomicU32::new(0));
    let mut fake_servers_a = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "rust-analyzer",
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let server_a_request_count = server_a_request_count.clone();
                move |fake_server| {
                    let server_a_request_count = server_a_request_count.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |_params, _| {
                            let count = server_a_request_count.fetch_add(1, Ordering::Release) + 1;
                            async move {
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(0, 9),
                                    label: lsp::InlayHintLabel::String(format!("server_a_{count}")),
                                    kind: Some(lsp::InlayHintKind::TYPE),
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

    // Server B returns a hint labeled "server_b" at a different position.
    let server_b_request_count = Arc::new(AtomicU32::new(0));
    let mut fake_servers_b = language_registry.register_fake_lsp(
        "Rust",
        FakeLspAdapter {
            name: "secondary-ls",
            capabilities: lsp::ServerCapabilities {
                inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                ..lsp::ServerCapabilities::default()
            },
            initializer: Some(Box::new({
                let server_b_request_count = server_b_request_count.clone();
                move |fake_server| {
                    let server_b_request_count = server_b_request_count.clone();
                    fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                        move |_params, _| {
                            let count = server_b_request_count.fetch_add(1, Ordering::Release) + 1;
                            async move {
                                Ok(Some(vec![lsp::InlayHint {
                                    position: lsp::Position::new(0, 22),
                                    label: lsp::InlayHintLabel::String(format!("server_b_{count}")),
                                    kind: Some(lsp::InlayHintKind::TYPE),
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

    let (buffer, _buffer_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let editor = cx.add_window(|window, cx| Editor::for_buffer(buffer, Some(project), window, cx));
    cx.executor().run_until_parked();

    let fake_server_a = fake_servers_a.next().await.unwrap();
    let _fake_server_b = fake_servers_b.next().await.unwrap();

    editor
        .update(cx, |editor, window, cx| {
            editor.set_visible_line_count(50.0, window, cx);
            editor.set_visible_column_count(120.0);
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    // Verify both servers' hints are present initially.
    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            let has_a = visible.iter().any(|h| h.starts_with("server_a"));
            let has_b = visible.iter().any(|h| h.starts_with("server_b"));
            assert!(
                has_a && has_b,
                "Both servers should have hints initially. Got: {visible:?}"
            );
        })
        .unwrap();

    // Trigger RefreshRequested from server A. This should re-fetch server A's
    // hints while keeping server B's hints intact.
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(
                InlayHintRefreshReason::RefreshRequested {
                    server_id: fake_server_a.server.server_id(),
                    request_id: Some(1),
                },
                cx,
            );
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    // Also trigger NewLinesShown to give the system a chance to recover
    // any chunks that might have been cleared.
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            let has_a = visible.iter().any(|h| h.starts_with("server_a"));
            let has_b = visible.iter().any(|h| h.starts_with("server_b"));
            assert!(
                has_a,
                "Server A hints should be present after its own refresh. Got: {visible:?}"
            );
            assert!(
                has_b,
                "Server B hints should NOT be lost when server A triggers \
                 RefreshRequested. Bug 2: clear() wipes all tracking, then \
                 LspStore filters out server B's cached hints via the for_server \
                 guard, and apply_fetched_hints removes all visible hints but only \
                 adds back server A's. Got: {visible:?}"
            );
        })
        .unwrap();
}
