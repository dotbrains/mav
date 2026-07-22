use super::*;

#[gpui::test]
async fn test_pressure_links(cx: &mut gpui::TestAppContext) {
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

    // Position the mouse over a symbol that has a definition
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

    cx.simulate_mouse_move(hover_point, None, Modifiers::none());

    // First simulate Normal pressure to set up the previous stage
    cx.simulate_event(MousePressureEvent {
        pressure: 0.5,
        stage: PressureStage::Normal,
        position: hover_point,
        modifiers: Modifiers::none(),
    });
    cx.background_executor.run_until_parked();

    // Now simulate Force pressure to trigger the force click and go-to definition
    cx.simulate_event(MousePressureEvent {
        pressure: 1.0,
        stage: PressureStage::Force,
        position: hover_point,
        modifiers: Modifiers::none(),
    });
    requests.next().await;
    cx.background_executor.run_until_parked();

    // Assert that we navigated to the definition
    cx.assert_editor_state(indoc! {"
                fn test() { do_work(); }
                fn «do_workˇ»() { test(); }
            "});
}
