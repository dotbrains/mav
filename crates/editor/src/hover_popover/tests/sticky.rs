use super::*;

#[gpui::test]
async fn test_hover_popover_hiding_delay(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let custom_delay_ms = 500u64;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_sticky = Some(true);
                settings.editor.hover_popover_hiding_delay = Some(DelayMs(custom_delay_ms));
            });
        });
    });

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

    // Trigger hover on a symbol
    let hover_point = cx.display_point(indoc! {"
            fn test() { printˇln!(); }
        "});
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
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    // Hover should be visible
    cx.editor(|editor, _, _| {
        assert!(editor.hover_state.visible());
    });

    // Move mouse away (hover_at with None anchor triggers the hiding delay)
    cx.update_editor(|editor, window, cx| hover_at(editor, None, None, window, cx));

    // Popover should still be visible before the custom hiding delay expires
    cx.background_executor
        .advance_clock(Duration::from_millis(custom_delay_ms - 100));
    cx.editor(|editor, _, _| {
        assert!(
            editor.hover_state.visible(),
            "Popover should remain visible before the hiding delay expires"
        );
    });

    // After the full custom delay, the popover should be hidden
    cx.background_executor
        .advance_clock(Duration::from_millis(200));
    cx.editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Popover should be hidden after the hiding delay expires"
        );
    });
}

#[gpui::test]
async fn test_hover_popover_sticky_disabled(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_sticky = Some(false);
            });
        });
    });

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

    // Trigger hover on a symbol
    let hover_point = cx.display_point(indoc! {"
            fn test() { printˇln!(); }
        "});
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
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    // Hover should be visible
    cx.editor(|editor, _, _| {
        assert!(editor.hover_state.visible());
    });

    // Move mouse away — with sticky disabled, hide immediately
    cx.update_editor(|editor, window, cx| hover_at(editor, None, None, window, cx));

    // Popover should be hidden immediately without any delay
    cx.editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Popover should be hidden immediately when sticky is disabled"
        );
    });
}

#[gpui::test]
async fn test_hover_popover_hiding_delay_restarts_when_mouse_gets_closer(
    cx: &mut gpui::TestAppContext,
) {
    init_test(cx, |_| {});

    let custom_delay_ms = 600u64;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_sticky = Some(true);
                settings.editor.hover_popover_hiding_delay = Some(DelayMs(custom_delay_ms));
            });
        });
    });

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
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    cx.editor(|editor, _, _| {
        assert!(editor.hover_state.visible());
    });

    cx.update_editor(|editor, _, _| {
        let popover = editor.hover_state.info_popovers.first().unwrap();
        popover.last_bounds.set(Some(Bounds {
            origin: gpui::Point {
                x: px(100.0),
                y: px(100.0),
            },
            size: Size {
                width: px(100.0),
                height: px(60.0),
            },
        }));
    });

    let far_point = gpui::Point {
        x: px(260.0),
        y: px(130.0),
    };
    cx.update_editor(|editor, window, cx| hover_at(editor, None, Some(far_point), window, cx));

    cx.background_executor
        .advance_clock(Duration::from_millis(400));
    cx.background_executor.run_until_parked();

    let closer_point = gpui::Point {
        x: px(220.0),
        y: px(130.0),
    };
    cx.update_editor(|editor, window, cx| hover_at(editor, None, Some(closer_point), window, cx));

    cx.background_executor
        .advance_clock(Duration::from_millis(250));
    cx.background_executor.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(
            editor.hover_state.visible(),
            "Popover should remain visible because moving closer restarts the hiding timer"
        );
    });

    cx.background_executor
        .advance_clock(Duration::from_millis(350));
    cx.background_executor.run_until_parked();

    cx.editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Popover should hide after the restarted hiding timer expires"
        );
    });
}

#[gpui::test]
async fn test_hover_popover_cancel_hide_on_rehover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let custom_delay_ms = 500u64;
    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_sticky = Some(true);
                settings.editor.hover_popover_hiding_delay = Some(DelayMs(custom_delay_ms));
            });
        });
    });

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
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });
    cx.background_executor
        .advance_clock(Duration::from_millis(get_hover_popover_delay(&cx) + 100));
    requests.next().await;

    cx.editor(|editor, _, _| {
        assert!(editor.hover_state.visible());
    });

    // Move mouse away — starts the 500ms hide timer
    cx.update_editor(|editor, window, cx| hover_at(editor, None, None, window, cx));

    cx.background_executor
        .advance_clock(Duration::from_millis(300));
    cx.background_executor.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            editor.hover_state.visible(),
            "Popover should still be visible before hiding delay expires"
        );
    });

    // Move back to the symbol — should cancel the hiding timer
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });

    // Advance past the original deadline — popover should still be visible
    // because re-hovering cleared the hiding_delay_task
    cx.background_executor
        .advance_clock(Duration::from_millis(300));
    cx.background_executor.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            editor.hover_state.visible(),
            "Popover should remain visible after re-hovering the symbol"
        );
        assert!(
            editor.hover_state.hiding_delay_task.is_none(),
            "Hiding delay task should have been cleared by re-hover"
        );
    });

    // Move away again — starts a fresh 500ms timer
    cx.update_editor(|editor, window, cx| hover_at(editor, None, None, window, cx));

    cx.background_executor
        .advance_clock(Duration::from_millis(custom_delay_ms + 100));
    cx.background_executor.run_until_parked();
    cx.editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Popover should hide after the new hiding timer expires"
        );
    });
}

#[gpui::test]
async fn test_hover_popover_enabled_false_ignores_sticky(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        cx.update_global::<SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_enabled = Some(false);
                settings.editor.hover_popover_sticky = Some(true);
                settings.editor.hover_popover_hiding_delay = Some(DelayMs(500));
            });
        });
    });

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

    // Trigger hover_at — should be gated by hover_popover_enabled=false
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.snapshot(window, cx);
        let anchor = snapshot
            .buffer_snapshot()
            .anchor_before(hover_point.to_offset(&snapshot, Bias::Left));
        hover_at(editor, Some(anchor), None, window, cx)
    });

    // No need to advance clock or wait for LSP — the gate should prevent any work
    cx.editor(|editor, _, _| {
        assert!(
            !editor.hover_state.visible(),
            "Popover should not appear when hover_popover_enabled is false"
        );
        assert!(
            editor.hover_state.info_task.is_none(),
            "No hover info task should be scheduled when hover is disabled"
        );
    });
}
