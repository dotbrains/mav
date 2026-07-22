use super::*;

#[gpui::test]
async fn test_document_link_resolve_on_hover(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_link_provider: Some(lsp::DocumentLinkOptions {
                resolve_provider: Some(true),
                work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        // See LICENSE for detailsˇ
    "});

    let link_range = cx.lsp_range(indoc! {"
        // See «LICENSE» for details
    "});
    let resolve_data = serde_json::json!({"id": 42});

    let mut document_link_requests = {
        let resolve_data = resolve_data.clone();
        cx.lsp
            .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |_, _| {
                let resolve_data = resolve_data.clone();
                async move {
                    Ok(Some(vec![lsp::DocumentLink {
                        range: link_range,
                        target: None,
                        tooltip: None,
                        data: Some(resolve_data),
                    }]))
                }
            })
    };

    let mut resolve_requests = cx
        .lsp
        .set_request_handler::<lsp::request::DocumentLinkResolve, _, _>(move |req, _| async move {
            Ok(lsp::DocumentLink {
                range: req.range,
                target: Some(lsp::Uri::from_str("https://opensource.org/licenses/MIT").unwrap()),
                tooltip: Some("Resolved tooltip".to_string()),
                data: None,
            })
        });

    cx.run_until_parked();
    document_link_requests.next().await;
    cx.run_until_parked();

    let screen_coord = cx.pixel_position(indoc! {"
        // See LICˇENSE for details
    "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::none());
    let delay_ms = cx.update(|_, cx| EditorSettings::get_global(cx).hover_popover_delay.0);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(delay_ms + 100));
    cx.run_until_parked();
    // Hover triggers resolve, not a viewport sweep.
    resolve_requests.next().await;
    cx.run_until_parked();

    cx.update_editor(|editor, _, cx| {
        let tooltip_text = editor
            .hover_state
            .info_popovers
            .iter()
            .find_map(|popover| {
                let parsed = popover.parsed_content.as_ref()?;
                let text = parsed.read(cx).parsed_markdown().source().to_string();
                (text == "Resolved tooltip").then_some(text)
            })
            .expect("resolved doc-link tooltip should appear in info_popovers");
        assert_eq!(tooltip_text, "Resolved tooltip");
    });
}
