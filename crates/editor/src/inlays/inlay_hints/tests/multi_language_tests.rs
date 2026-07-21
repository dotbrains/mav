use super::*;

#[gpui::test]
async fn test_multi_language_multibuffer_no_duplicate_hints(cx: &mut gpui::TestAppContext) {
    init_test(cx, &|settings| {
        settings.defaults.inlay_hints = Some(InlayHintSettingsContent {
            show_value_hints: Some(true),
            enabled: Some(true),
            edit_debounce_ms: Some(0),
            scroll_debounce_ms: Some(0),
            show_type_hints: Some(true),
            show_parameter_hints: Some(true),
            show_other_hints: Some(true),
            show_background: Some(false),
            toggle_on_modifiers_press: None,
        })
    });

    let fs = FakeFs::new(cx.background_executor.clone());
    fs.insert_tree(
        path!("/a"),
        json!({
            "main.rs": "fn main() { let x = 1; } // padding to keep hints from being trimmed",
            "index.ts": "const y = 2; // padding to keep hints from being trimmed in typescript",
        }),
    )
    .await;

    let project = Project::test(fs, [path!("/a").as_ref()], cx).await;
    let language_registry = project.read_with(cx, |project, _| project.languages().clone());

    let mut rs_fake_servers = None;
    let mut ts_fake_servers = None;
    for (name, path_suffix) in [("Rust", "rs"), ("TypeScript", "ts")] {
        language_registry.add(Arc::new(Language::new(
            LanguageConfig {
                name: name.into(),
                matcher: LanguageMatcher {
                    path_suffixes: vec![path_suffix.to_string()],
                    ..Default::default()
                },
                ..Default::default()
            },
            Some(tree_sitter_rust::LANGUAGE.into()),
        )));
        let fake_servers = language_registry.register_fake_lsp(
            name,
            FakeLspAdapter {
                name,
                capabilities: lsp::ServerCapabilities {
                    inlay_hint_provider: Some(lsp::OneOf::Left(true)),
                    ..Default::default()
                },
                initializer: Some(Box::new({
                    move |fake_server| {
                        let request_count = Arc::new(AtomicU32::new(0));
                        fake_server.set_request_handler::<lsp::request::InlayHintRequest, _, _>(
                            move |params, _| {
                                let count = request_count.fetch_add(1, Ordering::Release) + 1;
                                let prefix = match name {
                                    "Rust" => "rs_hint",
                                    "TypeScript" => "ts_hint",
                                    other => panic!("Unexpected language: {other}"),
                                };
                                async move {
                                    Ok(Some(vec![lsp::InlayHint {
                                        position: params.range.start,
                                        label: lsp::InlayHintLabel::String(format!(
                                            "{prefix}_{count}"
                                        )),
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
                ..Default::default()
            },
        );
        match name {
            "Rust" => rs_fake_servers = Some(fake_servers),
            "TypeScript" => ts_fake_servers = Some(fake_servers),
            _ => unreachable!(),
        }
    }

    let (rs_buffer, _rs_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/main.rs"), cx)
        })
        .await
        .unwrap();
    let (ts_buffer, _ts_handle) = project
        .update(cx, |project, cx| {
            project.open_local_buffer_with_lsp(path!("/a/index.ts"), cx)
        })
        .await
        .unwrap();

    let multi_buffer = cx.new(|cx| {
        let mut multibuffer = MultiBuffer::new(Capability::ReadWrite);
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(0),
            rs_buffer.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multibuffer.set_excerpts_for_path(
            PathKey::sorted(1),
            ts_buffer.clone(),
            [Point::new(0, 0)..Point::new(1, 0)],
            0,
            cx,
        );
        multibuffer
    });

    cx.executor().run_until_parked();
    let editor = cx.add_window(|window, cx| {
        Editor::for_multibuffer(multi_buffer, Some(project.clone()), window, cx)
    });

    let _rs_fake_server = rs_fake_servers.unwrap().next().await.unwrap();
    let _ts_fake_server = ts_fake_servers.unwrap().next().await.unwrap();
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.executor().run_until_parked();

    // Verify initial state: both languages have exactly one hint each
    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            let rs_hints: Vec<_> = visible
                .iter()
                .filter(|h| h.starts_with("rs_hint"))
                .collect();
            let ts_hints: Vec<_> = visible
                .iter()
                .filter(|h| h.starts_with("ts_hint"))
                .collect();
            assert_eq!(
                rs_hints.len(),
                1,
                "Should have exactly 1 Rust hint initially, got: {rs_hints:?}"
            );
            assert_eq!(
                ts_hints.len(),
                1,
                "Should have exactly 1 TypeScript hint initially, got: {ts_hints:?}"
            );
        })
        .unwrap();

    // Edit the Rust buffer — triggers BufferEdited(rust_buffer_id).
    // The language filter in refresh_inlay_hints excludes TypeScript excerpts
    // from processing, but the global clear() wipes added_hints for ALL buffers.
    editor
        .update(cx, |editor, window, cx| {
            editor.change_selections(SelectionEffects::no_scroll(), window, cx, |s| {
                s.select_ranges([MultiBufferOffset(0)..MultiBufferOffset(0)])
            });
            editor.handle_input("x", window, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();

    // Trigger NewLinesShown — this causes TypeScript chunks to be re-fetched
    // because hint_chunk_fetching was wiped by clear(). The cached hints pass
    // the added_hints.insert(...).is_none() filter (also wiped) and get inserted
    // alongside the still-displayed copies, causing duplicates.
    editor
        .update(cx, |editor, _window, cx| {
            editor.refresh_inlay_hints(InlayHintRefreshReason::NewLinesShown, cx);
        })
        .unwrap();
    cx.executor().run_until_parked();

    // Assert: TypeScript hints must NOT be duplicated
    editor
        .update(cx, |editor, _window, cx| {
            let visible = visible_hint_labels(editor, cx);
            let ts_hints: Vec<_> = visible
                .iter()
                .filter(|h| h.starts_with("ts_hint"))
                .collect();
            assert_eq!(
                ts_hints.len(),
                1,
                "TypeScript hints should NOT be duplicated after editing Rust buffer \
                 and triggering NewLinesShown. Got: {ts_hints:?}"
            );

            let rs_hints: Vec<_> = visible
                .iter()
                .filter(|h| h.starts_with("rs_hint"))
                .collect();
            assert_eq!(
                rs_hints.len(),
                1,
                "Rust hints should still be present after editing. Got: {rs_hints:?}"
            );
        })
        .unwrap();
}
