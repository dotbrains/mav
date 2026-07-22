use super::*;

#[gpui::test]
async fn test_mouse_hover_cancelled_before_delay(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

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
        hover_at(editor, Some(anchor), None, window, cx);
        hover_at(editor, None, None, window, cx);
    });

    let request_count = Arc::new(AtomicUsize::new(0));
    cx.set_request_handler::<lsp::request::HoverRequest, _, _>({
        let request_count = request_count.clone();
        move |_, _, _| {
            let request_count = request_count.clone();
            async move {
                request_count.fetch_add(1, atomic::Ordering::Release);
                Ok(Some(lsp::Hover {
                    contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                        kind: lsp::MarkupKind::Markdown,
                        value: "some basic docs".to_string(),
                    }),
                    range: None,
                }))
            }
        }
    });

    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    cx.background_executor.run_until_parked();
    cx.run_until_parked();

    assert_eq!(request_count.load(atomic::Ordering::Acquire), 0);
    cx.editor(|editor, _, _| {
        assert!(!editor.hover_state.visible());
    });
}

#[gpui::test]
async fn test_keyboard_hover_info_popover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Hover with keyboard has no delay
    cx.set_state(indoc! {"
            fˇn test() { println!(); }
        "});
    cx.update_editor(|editor, window, cx| hover(editor, &Hover, window, cx));
    let symbol_range = cx.lsp_range(indoc! {"
            «fn» test() { println!(); }
        "});

    cx.editor(|editor, _window, _cx| {
        assert!(!editor.hover_state.visible());

        assert_eq!(
            editor.hover_state.info_popovers.len(),
            0,
            "Expected no hovers but got but got: {:?}",
            editor.hover_state.info_popovers.len()
        );
    });

    let mut requests =
        cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: "some other basic docs".to_string(),
                }),
                range: Some(symbol_range),
            }))
        });

    requests.next().await;
    cx.dispatch_action(Hover);

    cx.condition(|editor, _| editor.hover_state.visible()).await;
    cx.editor(|editor, _, cx| {
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

        assert_eq!(rendered_text, "some other basic docs".to_string())
    });
}

#[gpui::test]
async fn test_empty_hovers_filtered(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Hover with keyboard has no delay
    cx.set_state(indoc! {"
            fˇn test() { println!(); }
        "});
    cx.update_editor(|editor, window, cx| hover(editor, &Hover, window, cx));
    let symbol_range = cx.lsp_range(indoc! {"
            «fn» test() { println!(); }
        "});
    cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| async move {
        Ok(Some(lsp::Hover {
            contents: lsp::HoverContents::Array(vec![
                lsp::MarkedString::String("regular text for hover to show".to_string()),
                lsp::MarkedString::String("".to_string()),
                lsp::MarkedString::LanguageString(lsp::LanguageString {
                    language: "Rust".to_string(),
                    value: "".to_string(),
                }),
            ]),
            range: Some(symbol_range),
        }))
    })
    .next()
    .await;
    cx.dispatch_action(Hover);

    cx.condition(|editor, _| editor.hover_state.visible()).await;
    cx.editor(|editor, _, cx| {
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

        assert_eq!(
            rendered_text,
            "regular text for hover to show".to_string(),
            "No empty string hovers should be shown"
        );
    });
}

#[gpui::test]
async fn test_line_ends_trimmed(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    // Hover with keyboard has no delay
    cx.set_state(indoc! {"
            fˇn test() { println!(); }
        "});
    cx.update_editor(|editor, window, cx| hover(editor, &Hover, window, cx));
    let symbol_range = cx.lsp_range(indoc! {"
            «fn» test() { println!(); }
        "});

    let code_str = "\nlet hovered_point: Vector2F // size = 8, align = 0x4\n";
    let markdown_string = format!("\n```rust\n{code_str}```");

    let closure_markdown_string = markdown_string.clone();
    cx.set_request_handler::<lsp::request::HoverRequest, _, _>(move |_, _, _| {
        let future_markdown_string = closure_markdown_string.clone();
        async move {
            Ok(Some(lsp::Hover {
                contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                    kind: lsp::MarkupKind::Markdown,
                    value: future_markdown_string,
                }),
                range: Some(symbol_range),
            }))
        }
    })
    .next()
    .await;

    cx.dispatch_action(Hover);

    cx.condition(|editor, _| editor.hover_state.visible()).await;
    cx.editor(|editor, _, cx| {
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

        assert_eq!(
            rendered_text, code_str,
            "Should not have extra line breaks at end of rendered hover"
        );
    });
}

#[gpui::test]
// https://github.com/mav-industries/mav/issues/15498
async fn test_info_hover_with_hrs(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
            fn fuˇnc(abc def: i32) -> u32 {
            }
        "});

    cx.lsp
        .set_request_handler::<lsp::request::HoverRequest, _, _>({
            |_, _| async move {
                Ok(Some(lsp::Hover {
                    contents: lsp::HoverContents::Markup(lsp::MarkupContent {
                        kind: lsp::MarkupKind::Markdown,
                        value: indoc!(
                            r#"
                    ### function `errands_data_read`

                    ---
                    → `char *`
                    Function to read a file into a string

                    ---
                    ```cpp
                    static char *errands_data_read()
                    ```
                    "#
                        )
                        .to_string(),
                    }),
                    range: None,
                }))
            }
        });
    cx.update_editor(|editor, window, cx| hover(editor, &Default::default(), window, cx));
    cx.run_until_parked();

    cx.update_editor(|editor, _, cx| {
        let popover = editor.hover_state.info_popovers.first().unwrap();
        let content = popover.get_rendered_text(cx);

        assert!(content.contains("Function to read a file"));
    });
}
