use super::*;

#[gpui::test]
async fn test_document_link_tooltip_respects_hover_popover_enabled(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});

    cx.update(|cx| {
        use gpui::BorrowAppContext as _;
        cx.update_global::<settings::SettingsStore, _>(|settings, cx| {
            settings.update_user_settings(cx, |settings| {
                settings.editor.hover_popover_enabled = Some(false);
            });
        });
    });

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
    cx.simulate_mouse_move(screen_coord, None, Modifiers::none());
    cx.background_executor
        .advance_clock(std::time::Duration::from_millis(2000));
    cx.run_until_parked();

    cx.update_editor(|editor, _, _| {
        assert!(
            editor.hover_state.info_popovers.is_empty(),
            "no popovers should appear when hover_popover_enabled is false"
        );
    });
}
