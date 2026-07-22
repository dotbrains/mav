use super::*;

#[gpui::test]
async fn test_hover_filename_with_row_column(cx: &mut gpui::TestAppContext) {
    init_test(cx, |_| {});
    let mut cx = EditorLspTestContext::new_rust(
        lsp::ServerCapabilities {
            ..Default::default()
        },
        cx,
    )
    .await;

    // Insert a new file with multiple lines
    let fs = cx.update_workspace(|workspace, _, cx| workspace.project().read(cx).fs().clone());
    fs.as_fake()
        .insert_file(
            path!("/root/dir/file2.rs"),
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10\n"
                .as_bytes()
                .to_vec(),
        )
        .await;

    // file2.rs:5:3 should be highlighted and clickable
    cx.set_state(indoc! {"
        Go to file2.rs:5:3 for the fix.ˇ
    "});

    let screen_coord = cx.pixel_position(indoc! {"
        Go to filˇe2.rs:5:3 for the fix.
    "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        Go to «file2.rs:5:3ˇ» for the fix.
    "},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());

    cx.update_workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));
    cx.update_workspace(|workspace, window, cx| {
        let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();
        {
            let editor = active_editor.read(cx);
            let buffer = editor.buffer().read(cx).as_singleton().unwrap();
            let file = buffer.read(cx).file().unwrap();
            let file_path = file.as_local().unwrap().abs_path(cx);
            assert_eq!(
                file_path,
                std::path::PathBuf::from(path!("/root/dir/file2.rs"))
            );
        }

        // Check that the cursor is at row 5, column 3 (0-indexed: row 4, col 2)
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
            4,
            "Expected cursor on row 5 (0-indexed: 4)"
        );
        assert_eq!(
            selections.head().column,
            2,
            "Expected cursor on column 3 (0-indexed: 2)"
        );
    });
}
