use super::*;

#[gpui::test]
async fn test_mouse_hover_info_popover_with_autocomplete_popover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            completion_provider: Some(lsp::CompletionOptions {
                trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
    )
    .await;
    let counter = Arc::new(AtomicUsize::new(0));
    // Basic hover delays and then pops without moving the mouse
    cx.set_state(indoc! {"
                oneˇ
                two
                three
                fn test() { println!(); }
            "});

    //prompt autocompletion menu
    cx.simulate_keystroke(".");
    handle_completion_request(
        indoc! {"
                        one.|<>
                        two
                        three
                    "},
        vec!["first_completion", "second_completion"],
        true,
        counter.clone(),
        &mut cx,
    )
    .await;
    cx.condition(|editor, _| editor.context_menu_visible()) // wait until completion menu is visible
        .await;
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1); // 1 completion request

    let hover_point = cx.display_point(indoc! {"
                one.
                two
                three
                fn test() { printˇln!(); }
            "});
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    assert!(!cx.editor(|editor, _window, _cx| editor.hover_state.visible()));

    // After delay, hover should be visible.
    let symbol_range = cx.lsp_range(indoc! {"
                one.
                two
                three
                fn test() { «println!»(); }
            "});
    let mut requests =
        cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: "some basic docs".to_string(),
                }),
                range: Some(symbol_range),
            }))
        });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    cx.editor(|editor, _window, cx| {
        assert!(editor.hover_state.visible());
        assert_eq!(
            editor.hover_state.info_popovers.len(),
            1,
            "Expected exactly one hover but got: {:?}",
            editor.hover_state.info_popovers.len()
        );
        let rendered_text = editor
            .hover_state
            .info_popovers
            .first()
            .unwrap()
            .get_rendered_text(cx);
        assert_eq!(rendered_text, "some basic docs".to_string())
    });

    // check that the completion menu is still visible and that there still has only been 1 completion request
    cx.editor(|editor, _, _| assert!(editor.context_menu_visible()));
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

    //apply a completion and check it was successfully applied
    let _apply_additional_edits = cx.update_editor(|editor, window, cx| {
        editor.context_menu_next(&Default::default(), window, cx);
        editor
            .confirm_completion(&ConfirmCompletion::default(), window, cx)
            .unwrap()
    });
    cx.assert_editor_state(indoc! {"
            one.second_completionˇ
            two
            three
            fn test() { println!(); }
        "});

    // check that the completion menu is no longer visible and that there still has only been 1 completion request
    cx.editor(|editor, _, _| assert!(!editor.context_menu_visible()));
    assert_eq!(counter.load(atomic::Ordering::Acquire), 1);

    //verify the information popover is still visible and unchanged
    cx.editor(|editor, _, cx| {
        assert!(editor.hover_state.visible());
        assert_eq!(
            editor.hover_state.info_popovers.len(),
            1,
            "Expected exactly one hover but got: {:?}",
            editor.hover_state.info_popovers.len()
        );
        let rendered_text = editor
            .hover_state
            .info_popovers
            .first()
            .unwrap()
            .get_rendered_text(cx);

        assert_eq!(rendered_text, "some basic docs".to_string())
    });

    // Mouse moved with no hover response dismisses
    let hover_point = cx.display_point(indoc! {"
                one.second_completionˇ
                two
                three
                fn teˇst() { println!(); }
            "});
    let mut request = cx
        .lsp
        .set_request_handler::<lsp::request::HoverRequest, _, _>(|_, _| async move { Ok(None) });
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    request.next().await;

    // verify that the information popover is no longer visible
    cx.editor(|editor, _, _| {
        assert!(!editor.hover_state.visible());
    });
}

#[gpui::test]
async fn test_mouse_hover_info_popover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Basic hover delays and then pops without moving the mouse
    cx.set_state(indoc! {"
            fn ˇtest() { println!(); }
        "});
    let hover_point = cx.display_point(indoc! {"
            fn test() { printˇln!(); }
        "});

    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    assert!(!cx.editor(|editor, _window, _cx| editor.hover_state.visible()));

    // After delay, hover should be visible.
    let symbol_range = cx.lsp_range(indoc! {"
            fn test() { «println!»(); }
        "});
    let mut requests =
        cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: "some basic docs".to_string(),
                }),
                range: Some(symbol_range),
            }))
        });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    cx.editor(|editor, _, cx| {
        assert!(editor.hover_state.visible());
        assert_eq!(
            editor.hover_state.info_popovers.len(),
            1,
            "Expected exactly one hover but got: {:?}",
            editor.hover_state.info_popovers.len()
        );
        let rendered_text = editor
            .hover_state
            .info_popovers
            .first()
            .unwrap()
            .get_rendered_text(cx);

        assert_eq!(rendered_text, "some basic docs".to_string())
    });

    // Mouse moved with no hover response dismisses
    let hover_point = cx.display_point(indoc! {"
            fn teˇst() { println!(); }
        "});
    let mut request = cx
        .lsp
        .set_request_handler::<lsp::request::HoverRequest, _, _>(|_, _| async move { Ok(None) });
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    request.next().await;
    cx.editor(|editor, _, _| {
        assert!(!editor.hover_state.visible());
    });
}
