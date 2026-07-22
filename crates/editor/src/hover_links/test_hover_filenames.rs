use super::*;

#[gpui::test]
async fn test_hover_filenames(cx: &mut gpui::TestAppContext) {
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
        .insert_file(
            path!("/root/dir/file2.rs"),
            "This is file2.rs".as_bytes().to_vec(),
        )
        .await;

    // Base document with {ABS} placeholder for absolute path prefix.
    // Each test case replaces a specific line to add cursor (ˇ) or highlight («»ˇ) markers.
    #[cfg(not(target_os = "windows"))]
    const ABS: &str = "/root/dir";
    #[cfg(target_os = "windows")]
    const ABS: &str = "C:/root/dir";

    let base = format!(
        "\
You can't go to a file that does_not_exist.txt.
Go to file2.rs if you want.
Or go to ../dir/file2.rs if you want.
Or go to {ABS}/file2.rs if project is local.
Or go to {ABS}/file2 if this is a Rust file.
Or `file2.rs` in backticks.
Or (file2.rs) in parens.
Or [link](file2.rs) markdown style.
A file (named file2.rs) in prose.
Read with `cat file2.rs` command.
Sentence ending file2.rs.
"
    );

    cx.set_state(&format!("{base}ˇ"));

    // Test cases: (original_line, cursor_line, highlight_line)
    // - cursor_line: the line with ˇ to position the mouse
    // - highlight_line: None = expect no highlight, Some(...) = expect this highlight
    let test_cases: &[(&str, &str, Option<&str>)] = &[
        // File does not exist - no highlight
        ("does_not_exist.txt", "dˇoes_not_exist.txt", None),
        // Simple filename
        (
            "Go to file2.rs if",
            "Go to fˇile2.rs if",
            Some("Go to «file2.rsˇ» if"),
        ),
        // Relative path
        (
            "Or go to ../dir/file2.rs if",
            "Or go to ../dir/fˇile2.rs if",
            Some("Or go to «../dir/file2.rsˇ» if"),
        ),
        // Absolute path
        (
            &format!("Or go to {ABS}/file2.rs if"),
            &format!("Or go to {ABS}/fiˇle2.rs if"),
            Some(&format!("Or go to «{ABS}/file2.rsˇ» if")),
        ),
        // Path without extension (language suffix added)
        (
            &format!("Or go to {ABS}/file2 if"),
            &format!("Or go to {ABS}/fiˇle2 if"),
            Some(&format!("Or go to «{ABS}/file2ˇ» if")),
        ),
        // Backticks
        (
            "Or `file2.rs` in backticks",
            "Or `fiˇle2.rs` in backticks",
            Some("Or `«file2.rsˇ»` in backticks"),
        ),
        // Parentheses
        (
            "Or (file2.rs) in parens",
            "Or (fiˇle2.rs) in parens",
            Some("Or («file2.rsˇ») in parens"),
        ),
        // Markdown link
        (
            "Or [link](file2.rs) markdown",
            "Or [link](fiˇle2.rs) markdown",
            Some("Or [link](«file2.rsˇ») markdown"),
        ),
        // Partial wrapper: trailing paren in prose like "(named file2.rs)"
        (
            "A file (named file2.rs) in",
            "A file (named fiˇle2.rs) in",
            Some("A file (named «file2.rsˇ») in"),
        ),
        // Partial wrapper: inside code span like "`cat file2.rs`"
        (
            "Read with `cat file2.rs` command",
            "Read with `cat fiˇle2.rs` command",
            Some("Read with `cat «file2.rsˇ»` command"),
        ),
        // Trailing period at end of sentence
        (
            "Sentence ending file2.rs.",
            "Sentence ending fiˇle2.rs.",
            Some("Sentence ending «file2.rsˇ»."),
        ),
    ];

    for (original, cursor_version, highlight_version) in test_cases {
        let position_text = base.replace(original, cursor_version);
        let screen_coord = cx.pixel_position(&position_text);
        cx.simulate_mouse_move(screen_coord, None, Modifiers::secondary_key());

        if let Some(highlight) = highlight_version {
            let expected = base.replace(original, highlight);
            cx.assert_editor_text_highlights(HighlightKey::HoveredLinkState, &expected);
        } else {
            // Expect no highlight
            cx.update_editor(|editor, window, cx| {
                assert!(
                    editor
                        .snapshot(window, cx)
                        .text_highlight_ranges(HighlightKey::HoveredLinkState)
                        .unwrap_or_default()
                        .1
                        .is_empty(),
                    "Expected no highlight for cursor at: {}",
                    cursor_version
                );
            });
        }
    }

    // Test click navigation on markdown link
    let position_text = base.replace(
        "Or [link](file2.rs) markdown",
        "Or [link](fiˇle2.rs) markdown",
    );
    let screen_coord = cx.pixel_position(&position_text);
    cx.simulate_click(screen_coord, Modifiers::secondary_key());

    cx.update_workspace(|workspace, _, cx| assert_eq!(workspace.items(cx).count(), 2));
    cx.update_workspace(|workspace, _, cx| {
        let active_editor = workspace.active_item_as::<Editor>(cx).unwrap();

        let buffer = active_editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
            .unwrap();

        let file = buffer.read(cx).file().unwrap();
        let file_path = file.as_local().unwrap().abs_path(cx);

        assert_eq!(
            file_path,
            std::path::PathBuf::from(path!("/root/dir/file2.rs"))
        );
    });
}
