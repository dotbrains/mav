use super::*;

#[gpui::test]
async fn test_hover_directories(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    // Insert a new file
    let fs = cx.update_workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file("/root/dir/file2.rs", "This is file2.rs".as_bytes().to_vec())
        .await;

    cx.set_state(indoc! {"
        You can't open ../diˇr because it's a directory.
    "});

    // File does not exist
    let screen_coord = cx.pixel_position(indoc! {"
        You can't open ../diˇr because it's a directory.
    "});
    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());

    // No highlight
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
