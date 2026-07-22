use super::*;

#[gpui::test]
async fn test_hover_links(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
            fn ˇtest() { do_work(); }
            fn do_work() { test(); }
        "});

    // Basic hold cmd, expect highlight in region if response contains definition
    let hover_point = cx.pixel_position(indoc! {"
            fn test() { do_wˇork(); }
            fn do_work() { test(); }
        "});
    let symbol_range = cx.lsp_range(indoc! {"
            fn test() { «do_work»(); }
            fn do_work() { test(); }
        "});
    let target_range = cx.lsp_range(indoc! {"
            fn test() { do_work(); }
            fn «do_work»() { test(); }
        "});

    let mut requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: Some(symbol_range),
                    target_uri: url.clone(),
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });

    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    requests.next().await;
    cx.background_executor.run_until_parked();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { «do_work»(); }
            fn do_work() { test(); }
        "},
    );

    // Unpress cmd causes highlight to go away
    cx.simulate_modifiers_change(Modifiers::none());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );

    let mut requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: Some(symbol_range),
                    target_uri: url.clone(),
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });

    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    requests.next().await;
    cx.background_executor.run_until_parked();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { «do_work»(); }
            fn do_work() { test(); }
        "},
    );

    // Moving mouse to location with no response dismisses highlight
    let hover_point = cx.pixel_position(indoc! {"
            fˇn test() { do_work(); }
            fn do_work() { test(); }
        "});
    let mut requests = cx
        .lsp
        .set_request_handler::<GotoDefinition, _, _>(move |_, _| async move {
            // No definitions returned
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![])))
        });
    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());

    requests.next().await;
    cx.background_executor.run_until_parked();

    // Assert no link highlights
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );

    // // Move mouse without cmd and then pressing cmd triggers highlight
    let hover_point = cx.pixel_position(indoc! {"
            fn test() { do_work(); }
            fn do_work() { teˇst(); }
        "});
    cx.simulate_mouse_move(hover_point, None, Modifiers::none());

    // Assert no link highlights
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );

    let symbol_range = cx.lsp_range(indoc! {"
            fn test() { do_work(); }
            fn do_work() { «test»(); }
        "});
    let target_range = cx.lsp_range(indoc! {"
            fn «test»() { do_work(); }
            fn do_work() { test(); }
        "});

    let mut requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: Some(symbol_range),
                    target_uri: url,
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });

    cx.simulate_modifiers_change(Modifiers::secondary_key());

    requests.next().await;
    cx.background_executor.run_until_parked();

    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { «test»(); }
        "},
    );

    cx.deactivate_window();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );

    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { «test»(); }
        "},
    );

    // Moving again within the same symbol range doesn't re-request
    let hover_point = cx.pixel_position(indoc! {"
            fn test() { do_work(); }
            fn do_work() { tesˇt(); }
        "});
    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { «test»(); }
        "},
    );

    // Cmd click with existing definition doesn't re-request and dismisses highlight
    cx.simulate_click(hover_point, Modifiers::secondary_key());
    cx.lsp
        .set_request_handler::<GotoDefinition, _, _>(move |_, _| async move {
            // Empty definition response to make sure we aren't hitting the lsp and using
            // the cached location instead
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![])))
        });
    cx.background_executor.run_until_parked();
    cx.assert_editor_state(indoc! {"
            fn «testˇ»() { do_work(); }
            fn do_work() { test(); }
        "});

    // Assert no link highlights after jump
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );

    // Cmd click without existing definition requests and jumps
    let hover_point = cx.pixel_position(indoc! {"
            fn test() { do_wˇork(); }
            fn do_work() { test(); }
        "});
    let target_range = cx.lsp_range(indoc! {"
            fn test() { do_work(); }
            fn «do_work»() { test(); }
        "});

    let mut requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: None,
                    target_uri: url,
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });
    cx.simulate_click(hover_point, Modifiers::secondary_key());
    requests.next().await;
    cx.background_executor.run_until_parked();
    cx.assert_editor_state(indoc! {"
            fn test() { do_work(); }
            fn «do_workˇ»() { test(); }
        "});

    // 1. We have a pending selection, mouse point is over a symbol that we have a response for, hitting cmd and nothing happens
    // 2. Selection is completed, hovering
    let hover_point = cx.pixel_position(indoc! {"
            fn test() { do_wˇork(); }
            fn do_work() { test(); }
        "});
    let target_range = cx.lsp_range(indoc! {"
            fn test() { do_work(); }
            fn «do_work»() { test(); }
        "});
    let mut requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: None,
                    target_uri: url,
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });

    // create a pending selection
    let selection_range = cx.ranges(indoc! {"
            fn «test() { do_w»ork(); }
            fn do_work() { test(); }
        "})[0]
        .clone();
    cx.update_editor(|editor, window, cx| {
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let anchor_range = snapshot.anchor_before(MultiBufferOffset(selection_range.start))
            ..snapshot.anchor_after(MultiBufferOffset(selection_range.end));
        editor.change_selections(Default::default(), window, cx, |s| {
            s.set_pending_anchor_range(anchor_range, crate::SelectMode::Character)
        });
    });
    cx.simulate_mouse_move(hover_point, None, Modifiers::secondary_key());
    cx.background_executor.run_until_parked();
    assert!(requests.try_recv().is_err());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
            fn test() { do_work(); }
            fn do_work() { test(); }
        "},
    );
    cx.background_executor.run_until_parked();
}
