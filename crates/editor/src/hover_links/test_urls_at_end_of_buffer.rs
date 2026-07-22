use super::*;

#[gpui::test]
async fn test_urls_at_end_of_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"A cool ˇwebpage is https://mav.dev/releases"});

    let screen_coord = cx.pixel_position(indoc! {"A cool webpage is https://mav.dev/releˇases"});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"A cool webpage is «https://mav.dev/releasesˇ»"},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    assert_eq!(cx.opened_url(), Some("https://mav.dev/releases".into()));
}
