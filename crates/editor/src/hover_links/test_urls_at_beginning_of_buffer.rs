use super::*;

#[gpui::test]
async fn test_urls_at_beginning_of_buffer(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"https://mav.dev/releases is a cool ˇwebpage."});

    let screen_coord = cx.pixel_position(indoc! {"https://mav.dev/relˇeases is a cool webpage."});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"«https://mav.dev/releasesˇ» is a cool webpage."},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    assert_eq!(cx.opened_url(), Some("https://mav.dev/releases".into()));
}
