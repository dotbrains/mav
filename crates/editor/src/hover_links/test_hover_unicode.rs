use super::*;

#[gpui::test]
async fn test_hover_unicode(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    cx.set_state(indoc! {"
        You can't open ˇ\"🤩\" because it's an emoji.
    "});

    // File does not exist
    let screen_coord = cx.pixel_position(indoc! {"
        You can't open ˇ\"🤩\" because it's an emoji.
    "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());

    // No highlight, does not panic...
    cx.update_editor(|editor, window, cx| {
        assert!(
            editor
                .snapshot(window, cx)
                .text_highlight_ranges(HighlightKey::HoveredLinkState)
                .unwrap_or_default()
                .1
                .is_empty()
        );
    });

    // Does not open the directory
    cx.simulate_click(screen_coord, Modifiers::secondary_key());
    cx.update_workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 1));
}
