use super::*;

#[gpui::test]
async fn test_document_links_take_priority_over_url_detection(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            document_link_provider: Some(lsp::DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: lsp::WorkDoneProgressOptions::default(),
            }),
            ..lsp::ServerCapabilities::default()
        },
        cx,
    )
    .await;

    // Text contains a URL, but the LSP provides a document link that
    // covers a broader range and points to a different target.
    cx.set_state(indoc! {"
        // See https://example.com for more infoˇ
    "});

    let link_range = cx.lsp_range(indoc! {"
        // «See https://example.com for more info»
    "});

    let mut requests = cx
        .lsp
        .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |_, _| async move {
            Ok(Some(vec![lsp::DocumentLink {
                range: link_range,
                target: Some(lsp::Uri::from_str("https://lsp-provided.example.com").unwrap()),
                tooltip: None,
                data: None,
            }]))
        });

    cx.run_until_parked();
    requests.next().await;
    cx.run_until_parked();

    let screen_coord = cx.pixel_position(indoc! {"
        // See https://examˇple.com for more info
    "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.run_until_parked();

    // LSP document link range is highlighted, not just the URL portion
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        // «See https://example.com for more infoˇ»
    "},
    );

    // Clicking navigates to the LSP-provided target, not the detected URL.
    // (Uri::to_string normalizes "https://host" to "https://host/")
    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    assert_eq!(
        cx.opened_url(),
        Some("https://lsp-provided.example.com/".into())
    );
}
