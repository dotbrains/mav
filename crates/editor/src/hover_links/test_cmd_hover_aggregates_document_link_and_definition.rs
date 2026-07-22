use super::*;

#[gpui::test]
async fn test_cmd_hover_aggregates_document_link_and_definition(cx: &mut gpui::TestAppContext) {
    // VSCode behavior: when a position carries multiple link sources
    // (LSP document link, go-to-definition, ...), cmd-click should reveal
    // every applicable target. We assert this by inspecting the
    // aggregated `hovered_link_state.links` after a cmd-hover.
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_link_provider: Some(lsp::DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
            }),
            definition_provider: Some(lsp::OneOf::Left(true)),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        // See LICENSE for details
        fn definition() {}ˇ
    "});

    let link_range = cx.lsp_range(indoc! {"
        // See «LICENSE» for details
        fn definition() {}
    "});
    let definition_target_range = cx.lsp_range(indoc! {"
        // See LICENSE for details
        fn «definition»() {}
    "});

    let mut document_link_requests = cx
        .lsp
        .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |_, _| async move {
            Ok(Some(vec![lsp::DocumentLink {
                range: link_range,
                target: Some(lsp::Uri::from_str("https://opensource.org/licenses/MIT").unwrap()),
                tooltip: Some("Open license".to_string()),
                data: None,
            }]))
        });

    let mut definition_requests =
        cx.set_request_handler::<GotoDefinition, _, _>(move |url, _, _| async move {
            Ok(Some(lsp::GotoDefinitionResponse::Link(vec![
                lsp::LocationLink {
                    origin_selection_range: Some(link_range),
                    target_uri: url.clone(),
                    target_range: definition_target_range,
                    target_selection_range: definition_target_range,
                },
            ])))
        });

    cx.run_until_parked();
    document_link_requests.next().await;
    cx.run_until_parked();

    let screen_coord = cx.pixel_position(indoc! {"
        // See LICˇENSE for details
        fn definition() {}
    "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    definition_requests.next().await;
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        let links = &editor
            .hovered_link_state
            .as_ref()
            .expect("cmd-hover should populate `hovered_link_state`")
            .links;
        let url_count = links
            .iter()
            .filter(|link| matches!(link, HoverLink::Url(_)))
            .count();
        let text_count = links
            .iter()
            .filter(|link| matches!(link, HoverLink::Text(_)))
            .count();
        assert_eq!(
            url_count, 1,
            "document link should contribute exactly one Url hover link, got {links:?}"
        );
        assert_eq!(
            text_count, 1,
            "go-to-definition should contribute exactly one Text hover link, got {links:?}"
        );
    });

    // Cmd-click resolves the in-buffer location (definition) since the
    // mixed Url + Text case lets `navigate_to_hover_links` prefer the
    // location target over the external URL.
    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    cx.run_until_parked();
    cx.assert_editor_state(indoc! {"
        // See LICENSE for details
        fn «definitionˇ»() {}
    "});
}
