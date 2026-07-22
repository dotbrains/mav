use super::*;

#[gpui::test]
async fn test_hover_markdown_link_with_row_column(cx: &mut gpui::TestAppContext) {
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
            "line 1\nline 2\nline 3\nline 4\nline 5\n"
                .as_bytes()
                .to_vec(),
        )
        .await;

    // Markdown link [text](file2.rs:3:2) should highlight only the inner link,
    // not the surrounding markdown syntax.
    cx.set_state(indoc! {"
        See [here](file2.rs:3:2) for details.ˇ
    "});

    let screen_coord = cx.pixel_position(indoc! {"
        See [here](filˇe2.rs:3:2) for details.
    "});

    cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());
    cx.assert_editor_text_highlights(
        HighlightKey::HoveredLinkState,
        indoc! {"
        See [here](«file2.rs:3:2ˇ») for details.
    "},
    );

    cx.simulate_click(screen_coord, Modifiers::secondary_key());

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

        // Check cursor is at row 3, column 2 (0-indexed: row 2, col 1)
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
            2,
            "Expected cursor on row 3 (0-indexed: 2)"
        );
        assert_eq!(
            selections.head().column,
            1,
            "Expected cursor on column 2 (0-indexed: 1)"
        );
    });
}
