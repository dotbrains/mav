use super::*;

#[gpui::test]
async fn test_hover_filename_with_non_numeric_suffix(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    let fs = cx.update_workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file(
            path!("/root/dir/file2.rs"),
            "line 1\nline 2\nline 3\n".as_bytes().to_vec(),
        )
        .await;

    // file2.rs:2:in should resolve to file2.rs line 2 (like Ruby backtraces)
    cx.set_state(indoc! {"
        Error at file2.rs:2:in 'method'ˇ
    "});

    let screen_coord = cx.pixel_position(indoc! {"
        Error at filˇe2.rs:2:in 'method'
    "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        Error at «file2.rs:2:inˇ» 'method'
    "},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());

    cx.update_workspace(|workspace, window, cx| {
        let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();
        let (count, snapshot) = active_editor.update(cx, |editor, cx| {
            (editor.selections.count(), editor.snapshot(window, cx))
        });
        assert_eq!(count, 1);
        let selections = active_editor
            .read(cx)
            .selections
            .newest::<language::Point>(&snapshot.display_snapshot);
        assert_eq!(
            selections.head().row,
            1,
            "Expected cursor on row 2 (0-indexed: 1)"
        );
    });
}
