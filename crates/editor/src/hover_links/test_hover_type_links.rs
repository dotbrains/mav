use super::*;

#[gpui::test]
async fn test_hover_type_links(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            hover_provider: Some(lsp::HoverProviderCapability::Simple(true)),
            type_definition_provider: Some(lsp::TypeDefinitionProviderCapability::Simple(true)),
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        struct A;
        let vˇariable = A;
    "});
    let screen_coord = cx.editor(|editor, _, cx| editor.pixel_position_of_cursor(cx));

    // Basic hold cmd+shift, expect highlight in region if response contains type definition
    let symbol_range = cx.lsp_range(indoc! {"
        struct A;
        let «variable» = A;
    "});
    let target_range = cx.lsp_range(indoc! {"
        struct «A»;
        let variable = A;
    "});

    cx.run_until_parked();

    let mut requests =
        cx.set_request_handler::<GotoTypeDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoTypeDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: Some(symbol_range),
                    target_uri: url.clone(),
                    target_range,
                    target_selection_range: target_range,
                },
            ])))
        });

    let modifiers = if cfg!(target_os = "macos") {
        Modifiers::command_shift()
    } else {
        Modifiers::control_shift()
    };

    cx.simulate_mouse_move(screen_coord.unwrap(), None, modifiers);

    requests.next().await;
    cx.run_until_parked();
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        struct A;
        let «variable» = A;
    "},
    );

    cx.simulate_modifiers_change(Modifiers::secondary_key());
    cx.run_until_parked();
    // Assert no link highlights
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        struct A;
        let variable = A;
    "},
    );

    cx.simulate_click(screen_coord.unwrap(), modifiers);

    cx.assert_editor_state(indoc! {"
        struct «Aˇ»;
        let variable = A;
    "});
}
