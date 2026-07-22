use super::*;

#[gpui::test]
async fn test_document_link_tooltip_popover(cx: &mut gpui::TestAppContext) {
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

    cx.set_state(indoc! {"
        // See LICENSE for detailsˇ
    "});

    let link_range = cx.lsp_range(indoc! {"
        // See «LICENSE» for details
    "});

    let mut requests = cx
        .lsp
        .set_request_handler::<lsp::request::DocumentLinkRequest, _, _>(move |_, _| async move {
            Ok(Some(vec![lsp::DocumentLink {
                range: link_range,
                target: Some(lsp::Uri::from_str("https://opensource.org/licenses/MIT").unwrap()),
                tooltip: Some("Open license".to_string()),
                data: None,
            }]))
        });

    cx.run_until_parked();
    requests.next().await;
    cx.run_until_parked();

    let screen_coord = cx.pixel_position(indoc! {"
        // See LICˇENSE for details
    "});
    // Plain hover (no modifier) is enough; the doc-link tooltip stacks
    // alongside the regular LSP hover popovers.
    cx.simulate_mouse_move(screen_coord, None, Modifiers::none());
    let delay_ms = cx.update(|_, cx| EditorSettings::get_global(cx).hover_popover_delay.0);
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(delay_ms + 100));
    cx.run_until_parked();

    cx.update_editor(|editor, _, cx| {
        let tooltip_text = editor
            .hover_state
            .info_popovers
            .iter()
            .find_map(|popover| {
                let parsed = popover.parsed_content.as_ref()?;
                let text = parsed.read(cx).parsed_markdown().source().to_string();
                (text == "Open license").then_some(text)
            })
            .expect("doc-link tooltip should appear in info_popovers on plain hover");
        assert_eq!(tooltip_text, "Open license");
    });

    // Move the mouse off the link; `show_hover` re-fires for the new
    // position and rebuilds `info_popovers` without the tooltip.
    let off_link = cx.pixel_position(indoc! {"
        // ˇSee LICENSE for details
    "});
    cx.simulate_mouse_move(off_link, None, Modifiers::none());
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(delay_ms + 100));
    cx.run_until_parked();
    cx.update_editor(|editor, _, cx| {
        let still_present = editor.hover_state.info_popovers.iter().any(|popover| {
            popover
                .parsed_content
                .as_ref()
                .map(|parsed| *parsed.read(cx).parsed_markdown().source() == "Open license")
                .unwrap_or(false)
        });
        assert!(
            !still_present,
            "doc-link tooltip should be cleared once the mouse leaves the link"
        );
    });
}
